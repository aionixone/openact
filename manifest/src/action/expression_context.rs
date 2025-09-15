use serde_json::{json, Value, Map};
use std::fs;

use super::auth::AuthContext;
use super::expression_engine::ExpressionContext;
use super::models::{Action, ActionExecutionContext};

pub fn build_expression_context(
    auth: &AuthContext,
    action: &Action,
    exec: &ActionExecutionContext,
) -> ExpressionContext {
    let mut ctx = json!({
        "action": {
            "method": action.method,
            "path": action.path,
            "provider": action.provider,
            "tenant": action.tenant,
        },
        "exec": {
            "action_trn": exec.action_trn,
            "execution_trn": exec.execution_trn,
            "timestamp": exec.timestamp,
        },
        "params": exec.parameters,
    });
    // Add request body to context for x-transform-pre
    if let Some(body) = &exec.request_body {
        if let Value::Object(ref mut o) = ctx {
            o.insert("body".to_string(), body.clone());
        }
    }
    // Inject secrets from OPENACT_SECRETS_FILE if present
    if let Some(secrets) = load_secrets_from_env_file() {
        if let Value::Object(ref mut o) = ctx {
            o.insert("secrets".to_string(), Value::Object(secrets));
        }
    }

    ExpressionContext {
        access_token: Some(auth.access_token.clone()),
        expires_at: auth.expires_at,
        ctx,
    }
}

fn load_secrets_from_env_file() -> Option<Map<String, Value>> {
    let path = std::env::var("OPENACT_SECRETS_FILE").ok()?;
    let content = fs::read_to_string(&path).ok()?;
    if path.ends_with(".json") {
        if let Ok(v) = serde_json::from_str::<Value>(&content) {
            if let Value::Object(obj) = v { return Some(obj); }
        }
    } else {
        if let Ok(v) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
            if let Ok(j) = serde_json::to_value(v) {
                if let Value::Object(obj) = j { return Some(obj); }
            }
        }
    }
    None
}


