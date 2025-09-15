//! OpenAct Core (minimal) - bindings only

pub mod action_registry;
pub mod auth_manager;
pub mod binding;
pub mod config;
pub mod database;
pub mod auth_orchestrator;

pub use auth_manager::AuthManager;
pub use binding::{Binding, BindingManager};
pub use config::{CoreConfig, CoreContext};
pub use database::CoreDatabase;
pub use auth_orchestrator::AuthOrchestrator;
use manifest::action::{Action, ActionExecutionContext, ActionParser, ActionParsingOptions};
pub use manifest::action::{ActionRunner, AuthAdapter};
use manifest::storage::execution_repository::ExecutionRepository;
use std::sync::Arc;

pub mod error {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum CoreError {
        #[error("Database: {0}")]
        Database(#[from] sqlx::Error),
        #[error("Invalid input: {0}")]
        InvalidInput(String),
    }

    pub type Result<T> = std::result::Result<T, CoreError>;
}

/// OpenAct Core (minimal) — execution orchestration
pub struct ExecutionEngine {
    pub db: CoreDatabase,
    pub bindings: BindingManager,
    pub action_registry: crate::action_registry::ActionRegistry,
    pub execution_repo: ExecutionRepository,
}

impl ExecutionEngine {
    pub fn new(db: CoreDatabase) -> Self {
        let bindings = BindingManager::new(db.pool().clone());
        let action_registry = crate::action_registry::ActionRegistry::new(db.pool().clone());
        let execution_repo = ExecutionRepository::new(db.pool().clone());
        Self { 
            db, 
            bindings, 
            action_registry,
            execution_repo,
        }
    }

    /// Convert stored action (DB model) to runtime action (execution model)
    fn convert_stored_to_runtime_action(
        stored_action: &manifest::storage::action_models::Action,
    ) -> error::Result<Action> {
        // Parse the OpenAPI spec to create runtime Actions
        let mut parser = ActionParser::new(ActionParsingOptions {
            default_tenant: stored_action.tenant.clone(),
            default_provider: stored_action.provider.clone(),
            ..Default::default()
        });

        // Stored spec may be YAML or JSON; serde_yaml can read both
        let spec: manifest::spec::api_spec::OpenApi30Spec = serde_yaml::from_str(&stored_action.openapi_spec)
            .map_err(|e| error::CoreError::InvalidInput(format!(
                "failed to parse OpenAPI spec: {} (Only OpenAPI 3.0 is supported)",
                e
            )))?;

        let mut result = parser
            .parse_spec(&spec)
            .map_err(|e| error::CoreError::InvalidInput(format!("failed to parse actions from spec: {}", e)))?;

        // Heuristics to select the intended action and align identifiers
        if result.actions.len() == 1 {
            let mut a = result.actions.remove(0);
            a.trn = stored_action.trn.clone();
            a.tenant = stored_action.tenant.clone();
            a.provider = stored_action.provider.clone();
            return Ok(a);
        }

        if let Some(mut a) = result
            .actions
            .iter()
            .cloned()
            .find(|a| a.name == stored_action.name)
        {
            a.trn = stored_action.trn.clone();
            a.tenant = stored_action.tenant.clone();
            a.provider = stored_action.provider.clone();
            return Ok(a);
        }

        // Fallback: if at least one action was parsed, use the first (clone)
        if let Some(mut a) = result.actions.first().cloned() {
            a.trn = stored_action.trn.clone();
            a.tenant = stored_action.tenant.clone();
            a.provider = stored_action.provider.clone();
            return Ok(a);
        }

        // Last resort: build action from the first path+method in the OpenAPI doc
        for (p, item) in &spec.paths.paths {
            let choose = |method: &str| -> Action {
                let op_name = item
                    .get
                    .as_ref()
                    .and_then(|op| op.operation_id.clone())
                    .unwrap_or_else(|| {
                        let segs: Vec<&str> = p
                            .trim_start_matches('/')
                            .split('/')
                            .filter(|s| !s.is_empty())
                            .collect();
                        let mut name_parts = vec![method.to_lowercase()];
                        name_parts.extend(segs.into_iter().map(|s| s.trim_matches('{').trim_matches('}').to_lowercase()));
                        name_parts.join(".")
                    });
                let a = Action::new(
                    op_name,
                    method.to_string(),
                    p.clone(),
                    stored_action.provider.clone(),
                    stored_action.tenant.clone(),
                    stored_action.trn.clone(),
                );
                a
            };
            if item.get.is_some() { return Ok(choose("GET")); }
            if item.post.is_some() { return Ok(choose("POST")); }
            if item.put.is_some() { return Ok(choose("PUT")); }
            if item.delete.is_some() { return Ok(choose("DELETE")); }
            if item.patch.is_some() { return Ok(choose("PATCH")); }
            if item.head.is_some() { return Ok(choose("HEAD")); }
            if item.options.is_some() { return Ok(choose("OPTIONS")); }
            if item.trace.is_some() { return Ok(choose("TRACE")); }
        }

        Err(error::CoreError::InvalidInput(format!("no actions parsed from spec for trn={}", stored_action.trn)))
    }

    /// Run action by TRN with execution persistence
    pub async fn run_action_by_trn(
        &self,
        tenant: &str,
        action_trn: &str,
        exec_trn: &str,
    ) -> error::Result<manifest::action::models::ActionExecutionResult> {
        use manifest::storage::action_models::{CreateExecutionRequest, ExecutionResult};

        // Ensure execution table exists
        self
            .execution_repo
            .ensure_table_exists()
            .await
            .map_err(|e| error::CoreError::InvalidInput(e.to_string()))?;

        // Fetch and validate action BEFORE creating execution record to avoid FK failures
        let stored_action = self.action_registry.get_by_trn(action_trn).await?;
        if stored_action.tenant != tenant {
            return Err(error::CoreError::InvalidInput(format!(
                "tenant mismatch: requested={}, action tenant={}",
                tenant, stored_action.tenant
            )));
        }
        
        // Convert to runtime action
        let runtime_action = Self::convert_stored_to_runtime_action(&stored_action)?;
        
        // Create execution record (status defaults to pending in repo)
        let create_req = CreateExecutionRequest {
            execution_trn: exec_trn.to_string(),
            action_trn: action_trn.to_string(),
            tenant: tenant.to_string(),
            input_data: None,
        };

        let execution_record = self
            .execution_repo
            .create_execution(create_req)
            .await
            .map_err(|e| error::CoreError::InvalidInput(format!("failed to create execution: {}", e)))?;
        
        let start_time = std::time::Instant::now();
        
        // Run the action using existing run_action method
        let result = self.run_action(tenant, runtime_action, exec_trn).await;
        
        // Update execution record with results
        let duration = start_time.elapsed();
        match result {
            Ok(success_result) => {
                // Persist success result
                let output_data = success_result
                    .response_data
                    .as_ref()
                    .and_then(|v| serde_json::to_string(v).ok());
                let exec_result = ExecutionResult {
                    output_data,
                    status: "completed".to_string(),
                    status_code: success_result.status_code,
                    error_message: None,
                    duration_ms: success_result.duration_ms.map(|v| v as i64),
                };
                self
                    .execution_repo
                    .update_execution_result(execution_record.id.unwrap_or(0), exec_result)
                    .await
                    .map_err(|e| error::CoreError::InvalidInput(format!("failed to update execution: {}", e)))?;

                Ok(success_result)
            }
            Err(err) => {
                // Persist failed result
                let exec_result = ExecutionResult {
                    output_data: None,
                    status: "failed".to_string(),
                    status_code: Some(500),
                    error_message: Some(err.to_string()),
                    duration_ms: Some(duration.as_millis() as i64),
                };
                self
                    .execution_repo
                    .update_execution_result(execution_record.id.unwrap_or(0), exec_result)
                    .await
                    .map_err(|e| error::CoreError::InvalidInput(format!("failed to update execution: {}", e)))?;

                Err(err)
            }
        }
    }

    /// Run a provided Action after wiring auth via bindings
    pub async fn run_action(
        &self,
        tenant: &str,
        mut action: Action,
        execution_trn: &str,
    ) -> error::Result<manifest::action::models::ActionExecutionResult> {
        // Resolve binding → auth_trn
        let auth_trn_opt = self
            .bindings
            .get_auth_trn_for_action(tenant, &action.trn)
            .await?;
        let auth_trn = auth_trn_opt
            .ok_or_else(|| error::CoreError::InvalidInput("No binding for action".into()))?;

        // Ensure action has auth_config with connection_trn
        if let Some(cfg) = &mut action.auth_config {
            cfg.connection_trn = auth_trn;
        } else {
            action.auth_config = Some(manifest::action::auth::AuthConfig {
                connection_trn: auth_trn,
                scheme: Some("oauth2".to_string()),
                injection: manifest::action::auth::InjectionConfig {
                    r#type: "jsonada".to_string(),
                    mapping: "{}".to_string(),
                },
                expiry: None,
                refresh: None,
                failure: None,
            });
        }

        // Build adapter (fallback to mock if store not initialized)
        let mut adapter = AuthAdapter::new(tenant.to_string());
        if let Ok(db_url) =
            std::env::var("OPENACT_DATABASE_URL").or_else(|_| std::env::var("AUTHFLOW_SQLITE_URL"))
        {
            // Try to init sqlite store; ignore error to allow mock fallback
            let _ = adapter.init_store_sqlite(db_url, true).await;
        }

        let mut runner = ActionRunner::new();
        runner.set_auth_adapter(Arc::new(adapter));
        let ctx = ActionExecutionContext::new(
            action.trn.clone(),
            execution_trn.to_string(),
            tenant.to_string(),
            action.provider.clone(),
        );
        let res = runner
            .execute_action(&action, ctx)
            .await
            .map_err(|e| error::CoreError::InvalidInput(e.to_string()))?;
        Ok(res)
    }

    /// Run with optional overrides (timeout_ms, extra headers)
    pub async fn run_action_with_overrides(
        &self,
        tenant: &str,
        mut action: Action,
        execution_trn: &str,
        timeout_ms: Option<u64>,
        extra_headers: Option<std::collections::HashMap<String, String>>,
    ) -> error::Result<manifest::action::models::ActionExecutionResult> {
        if let Some(t) = timeout_ms {
            action.timeout_ms = Some(t);
        }
        // Build adapter and context with headers
        // Resolve binding → auth_trn
        let auth_trn_opt = self
            .bindings
            .get_auth_trn_for_action(tenant, &action.trn)
            .await?;
        let auth_trn = auth_trn_opt
            .ok_or_else(|| error::CoreError::InvalidInput("No binding for action".into()))?;

        if let Some(cfg) = &mut action.auth_config {
            cfg.connection_trn = auth_trn;
        } else {
            action.auth_config = Some(manifest::action::auth::AuthConfig {
                connection_trn: auth_trn,
                scheme: Some("oauth2".to_string()),
                injection: manifest::action::auth::InjectionConfig { r#type: "jsonada".to_string(), mapping: "{}".to_string() },
                expiry: None,
                refresh: None,
                failure: None,
            });
        }

        let mut adapter = AuthAdapter::new(tenant.to_string());
        if let Ok(db_url) = std::env::var("OPENACT_DATABASE_URL").or_else(|_| std::env::var("AUTHFLOW_SQLITE_URL")) {
            let _ = adapter.init_store_sqlite(db_url, true).await;
        }
        let mut ctx = ActionExecutionContext::new(
            action.trn.clone(),
            execution_trn.to_string(),
            tenant.to_string(),
            action.provider.clone(),
        );
        if let Some(h) = extra_headers { for (k,v) in h { ctx.headers.insert(k, v); } }
        let default_ua = std::env::var("OPENACT_DEFAULT_USER_AGENT").unwrap_or_else(|_| "openact-cli/1.0".to_string());
        ctx.headers.entry("User-Agent".to_string()).or_insert(default_ua);

        let mut runner = ActionRunner::new();
        runner.set_auth_adapter(Arc::new(adapter));
        let res = runner
            .execute_action(&action, ctx)
            .await
            .map_err(|e| error::CoreError::InvalidInput(e.to_string()))?;
        Ok(res)
    }
}

#[cfg(test)]
mod tests_engine {
    use super::*;

    fn build_mock_action(tenant: &str, provider: &str, base_url: &str) -> Action {
        let mut action = Action::new(
            "getGithubUser".to_string(),
            "GET".to_string(),
            "/user".to_string(),
            provider.to_string(),
            tenant.to_string(),
            format!("trn:openact:{}:action/{}/getUser@v1", tenant, provider),
        );
        action.timeout_ms = Some(2000);
        action.ok_path = Some("$status >= 200 and $status < 300".to_string());
        action.output_pick = Some("$body".to_string());
        action
            .extensions
            .insert("x-real-http".to_string(), serde_json::json!(true));
        action
            .extensions
            .insert("x-base-url".to_string(), serde_json::json!(base_url));
        action
    }

    #[tokio::test]
    async fn run_action_with_binding_and_mock_auth() {
        // Prepare DB and binding
        let db = CoreDatabase::connect("sqlite::memory:").await.unwrap();
        db.migrate_bindings().await.unwrap();
        let engine = ExecutionEngine::new(db.clone());

        let tenant = "tenant1";
        let provider = "github";
        let action = build_mock_action(tenant, provider, "https://api.github.com");
        // Bind auth to action
        engine
            .bindings
            .bind(
                tenant,
                "trn:authflow:tenant1:connection/github-mock",
                &action.trn,
                Some("test"),
            )
            .await
            .unwrap();

        let result = engine
            .run_action(
                tenant,
                action,
                "trn:stepflow:tenant1:execution:action-execution:core-1",
            )
            .await
            .unwrap();
        assert!(matches!(
            result.status,
            manifest::action::models::ExecutionStatus::Success
        ));
    }
}
