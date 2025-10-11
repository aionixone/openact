//! Application state shared between MCP and REST API

use openact_core::orchestration::{OrchestratorOutboxStore, OrchestratorRunStore};
use openact_plugins as plugins;
use openact_registry::ConnectorRegistry;
use openact_store::SqlStore;
use std::sync::Arc;

#[cfg(feature = "authflow")]
use crate::flow_runner::FlowRunManager;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<SqlStore>,
    pub registry: Arc<ConnectorRegistry>,
    pub orchestrator_runs: Arc<dyn OrchestratorRunStore>,
    pub orchestrator_outbox: Arc<dyn OrchestratorOutboxStore>,
    #[cfg(feature = "authflow")]
    pub flow_manager: Arc<FlowRunManager>,
}

impl AppState {
    /// Create app state from database path
    pub async fn from_db_path(db_path: &str) -> anyhow::Result<Self> {
        let store = Arc::new(SqlStore::new(db_path).await?);

        // Build registry using store for both connections and actions
        let conn_store = store.as_ref().clone();
        let act_repo = store.as_ref().clone();
        let mut registry = ConnectorRegistry::new(conn_store, act_repo);

        // Register connector factories via plugins aggregator
        for registrar in plugins::registrars() {
            registrar(&mut registry);
        }

        #[cfg(feature = "authflow")]
        let flow_manager = Arc::new(FlowRunManager::new(store.clone()));

        let orchestrator_runs: Arc<dyn OrchestratorRunStore> = store.clone();
        let orchestrator_outbox: Arc<dyn OrchestratorOutboxStore> = store.clone();

        Ok(Self {
            store,
            registry: Arc::new(registry),
            orchestrator_runs,
            orchestrator_outbox,
            #[cfg(feature = "authflow")]
            flow_manager,
        })
    }
}
