//! Flow runner utilities for executing StepFlow DSLs with OAuth-style pauses.
//!
//! This module is currently gated behind the `callback` feature because it relies on the
//! built-in callback server to handle redirect-based flows.

#![cfg(feature = "callback")]

use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use serde_json::Value;
use stepflow_dsl::WorkflowDSL;

use crate::{
    engine::TaskHandler,
    server::callback::CallbackServer,
    workflow::{resume_from_pause, start_until_pause},
};
use openact_core::store::RunStore;

/// Flow runner configuration.
#[derive(Clone, Debug)]
pub struct FlowRunnerConfig {
    /// JSON Pointer (RFC6901) pointing to the authorize URL inside the pending context.
    pub authorize_url_ptr: String,
    /// JSON Pointer pointing to the state value inside the pending context.
    pub state_ptr: String,
    /// Optional JSON Pointer to set the callback/redirect URL inside the input payload before execution.
    pub redirect_ptr: Option<String>,
    /// Optional JSON Pointer pointing to `auth_ref` inside the final context.
    pub auth_ref_ptr: Option<String>,
    /// Optional JSON Pointer pointing to `connection_ref` inside the final context.
    pub connection_ref_ptr: Option<String>,
    /// Callback server bind address (use `0.0.0.0:0` or `127.0.0.1:0` for dynamic port).
    pub callback_addr: SocketAddr,
    /// HTTP path for callback requests.
    pub callback_path: String,
    /// Timeout for waiting on the callback.
    pub callback_timeout: Duration,
}

impl Default for FlowRunnerConfig {
    fn default() -> Self {
        Self {
            authorize_url_ptr: "/vars/auth/authorize_url".to_string(),
            state_ptr: "/vars/auth/state".to_string(),
            redirect_ptr: Some("/redirectUri".to_string()),
            auth_ref_ptr: Some("/auth_ref".to_string()),
            connection_ref_ptr: Some("/connection_ref".to_string()),
            callback_addr: "127.0.0.1:0".parse().expect("valid address"),
            callback_path: "/oauth/callback".to_string(),
            callback_timeout: Duration::from_secs(300),
        }
    }
}

/// Convenience builder for running flows.
#[derive(Clone)]
pub struct FlowRunner {
    dsl: Arc<WorkflowDSL>,
    handler: Arc<dyn TaskHandler>,
    run_store: Arc<dyn RunStore>,
    config: FlowRunnerConfig,
}

impl FlowRunner {
    pub fn new(
        dsl: Arc<WorkflowDSL>,
        handler: Arc<dyn TaskHandler>,
        run_store: Arc<dyn RunStore>,
        config: FlowRunnerConfig,
    ) -> Self {
        Self { dsl, handler, run_store, config }
    }

    /// Start the flow and return a handle. Caller can display authorize URL before awaiting
    /// the final result.
    pub async fn start(&self, mut input: Value) -> Result<FlowRunHandle> {
        let server = CallbackServer::new(self.config.callback_addr)
            .with_callback_path(self.config.callback_path.clone())
            .with_timeout(self.config.callback_timeout);
        let server = server.start().await?;

        if let Some(ptr) = &self.config.redirect_ptr {
            set_pointer_value(&mut input, ptr, Value::String(server.callback_url()))?;
        }

        tracing::info!(
            callback = %server.callback_url(),
            authorize_ptr = %self.config.authorize_url_ptr,
            state_ptr = %self.config.state_ptr,
            "Starting flow until pause"
        );

        let context = serde_json::json!({"input": input});
        let pending = start_until_pause(
            self.dsl.as_ref(),
            self.handler.as_ref(),
            self.run_store.as_ref(),
            context,
        )
        .await?;
        let context = &pending.context;
        let authorize_url = context
            .pointer(&self.config.authorize_url_ptr)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                anyhow!("authorize_url not found at {}", self.config.authorize_url_ptr)
            })?;
        let state = context
            .pointer(&self.config.state_ptr)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow!("state not found at {}", self.config.state_ptr))?;

        tracing::info!(run_id = %pending.run_id, %authorize_url, %state, "Flow paused waiting for OAuth callback");

        Ok(FlowRunHandle {
            authorize_url,
            callback_url: server.callback_url(),
            state,
            run_id: pending.run_id,
            dsl: Arc::clone(&self.dsl),
            handler: Arc::clone(&self.handler),
            run_store: Arc::clone(&self.run_store),
            server,
            auth_ref_ptr: self.config.auth_ref_ptr.clone(),
            connection_ref_ptr: self.config.connection_ref_ptr.clone(),
        })
    }

    /// Convenience helper: start the flow, wait for callback, return the final result.
    pub async fn run(&self, input: Value) -> Result<FlowRunResult> {
        let handle = self.start(input).await?;
        handle.wait_for_completion().await
    }
}

pub struct FlowRunHandle {
    pub authorize_url: String,
    pub callback_url: String,
    state: String,
    run_id: String,
    dsl: Arc<WorkflowDSL>,
    handler: Arc<dyn TaskHandler>,
    run_store: Arc<dyn RunStore>,
    server: crate::server::callback::CallbackServerHandle,
    auth_ref_ptr: Option<String>,
    connection_ref_ptr: Option<String>,
}

impl FlowRunHandle {
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn state_token(&self) -> &str {
        &self.state
    }

    /// Wait for the callback, resume the flow, and return the final context and extracted metadata.
    pub async fn wait_for_completion(self) -> Result<FlowRunResult> {
        let params = self.server.wait_for_callback(&self.state, &self.run_id).await?;

        if let Some(error) = params.error {
            let desc = params.error_description.unwrap_or_else(|| "Unknown error".to_string());
            self.server.shutdown().await.ok();
            return Err(anyhow!("OAuth2 error: {} - {}", error, desc));
        }

        let code = params.code.ok_or_else(|| anyhow!("No authorization code received"))?;
        let state = params.state.ok_or_else(|| anyhow!("No state received"))?;

        let resume = resume_from_pause(
            self.dsl.as_ref(),
            self.handler.as_ref(),
            self.run_store.as_ref(),
            &self.run_id,
            serde_json::json!({ "code": code, "state": state }),
        )
        .await?;

        let final_context = match resume {
            crate::engine::RunOutcome::Finished(ctx) => ctx,
            crate::engine::RunOutcome::Pending(_) => {
                self.server.shutdown().await.ok();
                return Err(anyhow!("flow paused again unexpectedly"));
            }
        };

        let auth_ref =
            extract_string_field(&final_context, self.auth_ref_ptr.as_deref(), "auth_ref");
        let connection_ref = extract_string_field(
            &final_context,
            self.connection_ref_ptr.as_deref(),
            "connection_ref",
        );

        self.server.shutdown().await.ok();

        Ok(FlowRunResult { run_id: self.run_id, final_context, auth_ref, connection_ref })
    }
}

pub struct FlowRunResult {
    pub run_id: String,
    pub final_context: Value,
    pub auth_ref: Option<String>,
    pub connection_ref: Option<String>,
}

fn set_pointer_value(root: &mut Value, pointer: &str, value: Value) -> Result<()> {
    if pointer == "" || pointer == "/" {
        *root = value;
        return Ok(());
    }

    let mut tokens = pointer
        .trim_start_matches('/')
        .split('/')
        .map(decode_pointer_token)
        .collect::<Result<Vec<_>, _>>()?;

    if tokens.is_empty() {
        *root = value;
        return Ok(());
    }

    let last = tokens.pop().unwrap();
    let mut current = root;
    for token in tokens {
        match current {
            Value::Object(map) => {
                current = map.entry(token).or_insert(Value::Null);
            }
            Value::Null => {
                *current = Value::Object(Default::default());
                if let Value::Object(map) = current {
                    current = map.entry(token).or_insert(Value::Null);
                }
            }
            _ => {
                return Err(anyhow!("cannot set pointer segment on non-object"));
            }
        }
    }

    match current {
        Value::Object(map) => {
            map.insert(last, value);
            Ok(())
        }
        _ => Err(anyhow!("cannot set final segment on non-object")),
    }
}

fn decode_pointer_token(segment: &str) -> Result<String> {
    let mut result = String::with_capacity(segment.len());
    let chars = segment.chars().collect::<Vec<_>>();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '~' {
            i += 1;
            if i >= chars.len() {
                return Err(anyhow!("invalid escape in pointer segment"));
            }
            match chars[i] {
                '0' => result.push('~'),
                '1' => result.push('/'),
                other => {
                    return Err(anyhow!("invalid escape '~{}'", other));
                }
            }
        } else {
            result.push(c);
        }
        i += 1;
    }
    Ok(result)
}

fn extract_string_field(context: &Value, pointer: Option<&str>, field: &str) -> Option<String> {
    if let Some(ptr) = pointer {
        if let Some(val) = context.pointer(ptr).and_then(|v| v.as_str()) {
            return Some(val.to_string());
        }
    }

    if let Some(val) = context.get(field).and_then(|v| v.as_str()) {
        return Some(val.to_string());
    }

    if let Some(vars) = context.get("vars").and_then(|v| v.as_object()) {
        if let Some(val) = vars.get(field).and_then(|v| v.as_str()) {
            return Some(val.to_string());
        }
    }

    if let Some(states) = context.get("states").and_then(|v| v.as_object()) {
        for state in states.values() {
            if let Some(result) = state.get("result") {
                if let Some(val) =
                    result.as_object().and_then(|obj| obj.get(field)).and_then(|v| v.as_str())
                {
                    return Some(val.to_string());
                }
            }
        }
    }

    None
}
