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
use manifest::action::{Action, ActionExecutionContext};
pub use manifest::action::{ActionRunner, AuthAdapter};
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
}

impl ExecutionEngine {
    pub fn new(db: CoreDatabase) -> Self {
        let bindings = BindingManager::new(db.pool().clone());
        Self { db, bindings }
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
