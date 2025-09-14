use crate::error::{CoreError, Result};
use authflow::actions::ActionRouter;
use authflow::dsl::AuthFlowDSL;
use authflow::engine::{run_until_pause_or_end, RunOutcome};
use authflow::store::{create_connection_store, StoreBackend, StoreConfig};
use serde_json::{json, Map, Value};
use sqlx::SqlitePool;
use std::{fs, path::Path};

#[derive(Clone)]
pub struct AuthOrchestrator {
    pub pool: SqlitePool,
}

impl AuthOrchestrator {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn load_dsl_from_file(path: &Path) -> Result<AuthFlowDSL> {
        let content = fs::read_to_string(path).map_err(|e| {
            CoreError::InvalidInput(format!("read {} failed: {}", path.display(), e))
        })?;
        let dsl = if path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.eq_ignore_ascii_case("yaml") || s.eq_ignore_ascii_case("yml"))
            .unwrap_or(false)
        {
            AuthFlowDSL::from_yaml(&content)
                .map_err(|e| CoreError::InvalidInput(format!("parse YAML failed: {}", e)))?
        } else {
            AuthFlowDSL::from_json(&content)
                .map_err(|e| CoreError::InvalidInput(format!("parse JSON failed: {}", e)))?
        };
        Ok(dsl)
    }

    fn read_text(path: &Path) -> Result<String> {
        fs::read_to_string(path).map_err(|e| CoreError::InvalidInput(format!(
            "read {} failed: {}",
            path.display(), e
        )))
    }

    fn extract_secret_keys_from_text(text: &str) -> Vec<String> {
        let mut keys: Vec<String> = Vec::new();
        let needle = "vars.secrets.";
        let bytes = text.as_bytes();
        let mut i = 0usize;
        while let Some(pos) = text[i..].find(needle) {
            let start = i + pos + needle.len();
            let mut end = start;
            while end < bytes.len() {
                let c = bytes[end] as char;
                if c.is_ascii_alphanumeric() || c == '_' || c == '-' { end += 1; } else { break; }
            }
            if end > start {
                let key = &text[start..end];
                if !keys.iter().any(|k| k == key) { keys.push(key.to_string()); }
            }
            i = end;
            if i >= bytes.len() { break; }
        }
        keys
    }

    fn load_secrets_from_file(path: &str) -> Option<Map<String, Value>> {
        let content = fs::read_to_string(path).ok()?;
        if path.ends_with(".json") {
            if let Ok(v) = serde_json::from_str::<Value>(&content) {
                if let Value::Object(obj) = v { return Some(obj); }
            }
        } else {
            // try YAML
            if let Ok(v) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                let j = serde_json::to_value(v).ok()?;
                if let Value::Object(obj) = j { return Some(obj); }
            }
        }
        None
    }

    fn env_var_candidates_for_key(key: &str) -> Vec<String> {
        // Convert key like github_client_id -> GITHUB_CLIENT_ID
        let upper = key.replace('-', "_").to_uppercase();
        vec![upper]
    }


    async fn build_router_from_env() -> Result<ActionRouter> {
        let db_url = std::env::var("OPENACT_DATABASE_URL")
            .or_else(|_| std::env::var("AUTHFLOW_SQLITE_URL"))
            .unwrap_or_else(|_| "sqlite:./data/openact.db".to_string());
        let mut cfg = StoreConfig {
            backend: StoreBackend::Sqlite,
            ..Default::default()
        };
        cfg.sqlite = Some(authflow::store::sqlite_connection_store::SqliteConfig {
            database_url: db_url,
            ..Default::default()
        });
        let store = create_connection_store(cfg)
            .await
            .map_err(|e| CoreError::InvalidInput(format!("init store failed: {}", e)))?;
        Ok(ActionRouter::new(store))
    }

    /// Run OAuth DSL until pause, return authorize URL and pending context for resume
    pub async fn begin_oauth_from_config(
        &self,
        tenant: &str,
        config_path: &Path,
        flow: Option<&str>,
        redirect_uri: Option<&str>,
        scope: Option<&str>,
    ) -> Result<(String, OAuthPending)> {
        // Load DSL and raw text
        let text = Self::read_text(config_path)?;
        let dsl = Self::load_dsl_from_file(config_path)?;

        // choose flow
        let flow_name = flow.unwrap_or("OAuth").to_string();
        let wf = dsl
            .get_flow(&flow_name)
            .ok_or_else(|| CoreError::InvalidInput(format!("flow '{}' not found", flow_name)))?;

        // Build execution context like AuthFlow server does
        let mut ctx = json!({
            "input": {
                "tenant": tenant,
                "redirectUri": redirect_uri.unwrap_or("http://127.0.0.1:8085/oauth/callback"),
                "scope": scope.unwrap_or("user:email")
            }
        });

        // Detect required secrets from DSL usages and populate from env or secrets file
        let required_keys = Self::extract_secret_keys_from_text(&text);
        if !required_keys.is_empty() {
            let mut provided: Map<String, Value> = Map::new();
            let file_map = std::env::var("OPENACT_SECRETS_FILE").ok().and_then(|p| Self::load_secrets_from_file(&p));
            for key in &required_keys {
                let mut val_opt: Option<String> = None;
                // 1) secrets file
                if let Some(ref m) = file_map {
                    if let Some(v) = m.get(key) { if let Some(s) = v.as_str() { val_opt = Some(s.to_string()); } }
                }
                // 2) environment variables (candidates)
                if val_opt.is_none() {
                    for cand in Self::env_var_candidates_for_key(key) {
                        if let Ok(v) = std::env::var(&cand) { val_opt = Some(v); break; }
                    }
                }
                if let Some(v) = val_opt { provided.insert(key.clone(), Value::String(v)); }
            }
            // Check missing
            let missing: Vec<String> = required_keys
                .iter()
                .filter(|k| !provided.contains_key(*k))
                .cloned()
                .collect();
            if !missing.is_empty() {
                let mut hints: Vec<String> = Vec::new();
                for k in &missing {
                    let cands = Self::env_var_candidates_for_key(k);
                    hints.push(format!("{} -> set env: {}", k, cands.join(" | ")));
                }
                return Err(CoreError::InvalidInput(format!(
                    "missing required secrets: [{}]. Hints: {}; or provide OPENACT_SECRETS_FILE (json/yaml) with {{key: value}}",
                    missing.join(", "),
                    hints.join("; ")
                )));
            }
            if let Value::Object(ref mut ctx_obj) = ctx {
                let vars = ctx_obj.entry("vars").or_insert_with(|| json!({}));
                if let Value::Object(ref mut vars_obj) = vars {
                    vars_obj.insert("secrets".into(), Value::Object(provided));
                }
            }
        }

        let router = Self::build_router_from_env().await?;
        let outcome = run_until_pause_or_end(wf, &wf.start_at, ctx, &router, 200)
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?;

        match outcome {
            RunOutcome::Pending(p) => {
                // Try to read authorize URL from context
                let auth_url = p
                    .context
                    .pointer("/states/StartAuth/result/authorize_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if auth_url.is_empty() {
                    return Err(CoreError::InvalidInput(
                        "authorize_url not found in context".into(),
                    ));
                }
                Ok((
                    auth_url,
                    OAuthPending {
                        dsl,
                        flow_name,
                        next_state: p.next_state,
                        context: p.context,
                        router,
                    },
                ))
            }
            RunOutcome::Finished(_) => Err(CoreError::InvalidInput(
                "flow finished without awaiting callback".into(),
            )),
        }
    }

    /// Resume pending OAuth with a callback URL (or code/state), return stored connection TRN
    pub fn complete_oauth_with_callback(
        &self,
        mut pending: OAuthPending,
        callback_url: &str,
    ) -> Result<String> {
        // parse code & state from callback_url
        let parsed = url::Url::parse(callback_url)
            .map_err(|e| CoreError::InvalidInput(format!("invalid callback url: {}", e)))?;
        let mut code = String::new();
        let mut state = String::new();
        for (k, v) in parsed.query_pairs() {
            if k == "code" {
                code = v.to_string();
            }
            if k == "state" {
                state = v.to_string();
            }
        }
        if code.is_empty() {
            return Err(CoreError::InvalidInput(
                "missing code in callback url".into(),
            ));
        }
        // write into pending.context top-level
        if let Value::Object(ref mut obj) = pending.context {
            obj.insert("code".into(), Value::String(code));
            if !state.is_empty() {
                obj.insert("state".into(), Value::String(state));
            }
        }
        let wf = pending
            .dsl
            .get_flow(&pending.flow_name)
            .ok_or_else(|| CoreError::InvalidInput("flow not found on resume".into()))?;
        let outcome = run_until_pause_or_end(
            wf,
            &pending.next_state,
            pending.context,
            &pending.router,
            200,
        )
        .map_err(|e| CoreError::InvalidInput(e.to_string()))?;

        let final_ctx = match outcome {
            RunOutcome::Finished(v) => v,
            RunOutcome::Pending(_) => {
                return Err(CoreError::InvalidInput(
                    "still pending after callback".into(),
                ))
            }
        };
        // Extract TRN from PersistConnection result
        let trn = final_ctx
            .pointer("/states/PersistConnection/result/trn")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CoreError::InvalidInput("connection TRN not found in final context".into())
            })?;
        Ok(trn.to_string())
    }

    /// Resume OAuth from saved parts: reload DSL and router, then run until finish
    pub async fn resume_with_context(
        &self,
        config_path: &Path,
        flow_name: &str,
        next_state: &str,
        context: Value,
    ) -> Result<String> {
        let dsl = Self::load_dsl_from_file(config_path)?;
        let wf = dsl
            .get_flow(flow_name)
            .ok_or_else(|| CoreError::InvalidInput("flow not found on resume".into()))?;
        let router = Self::build_router_from_env().await?;
        let outcome = run_until_pause_or_end(wf, next_state, context, &router, 200)
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
        let final_ctx = match outcome {
            RunOutcome::Finished(v) => v,
            RunOutcome::Pending(_) => {
                return Err(CoreError::InvalidInput(
                    "still pending after resume".into(),
                ))
            }
        };
        let trn = final_ctx
            .pointer("/states/PersistConnection/result/trn")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CoreError::InvalidInput("connection TRN not found in final context".into())
            })?;
        Ok(trn.to_string())
    }
}

pub struct OAuthPending {
    pub dsl: AuthFlowDSL,
    pub flow_name: String,
    pub next_state: String,
    pub context: Value,
    pub router: ActionRouter,
}

// Helper to convert DSL back to json to pick vars
