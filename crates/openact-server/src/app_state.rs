//! Application state shared between MCP and REST API

use openact_core::orchestration::{OrchestratorOutboxStore, OrchestratorRunStore};
use openact_plugins as plugins;
use openact_registry::ConnectorRegistry;
use openact_store::SqlStore;
use std::sync::Arc;

#[cfg(feature = "authflow")]
use crate::flow_runner::FlowRunManager;
use crate::orchestration::{HeartbeatSupervisor, OutboxDispatcher, OutboxService, RunService};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<SqlStore>,
    pub registry: Arc<ConnectorRegistry>,
    pub orchestrator_runs: Arc<dyn OrchestratorRunStore>,
    pub orchestrator_outbox: Arc<dyn OrchestratorOutboxStore>,
    pub run_service: RunService,
    pub outbox_service: OutboxService,
    pub outbox_dispatcher: Arc<OutboxDispatcher>,
    pub heartbeat_supervisor: Arc<HeartbeatSupervisor>,
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
        let run_service = RunService::new(orchestrator_runs.clone());
        let outbox_service = OutboxService::new(orchestrator_outbox.clone());
        let stepflow_endpoint = std::env::var("OPENACT_STEPFLOW_EVENT_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:8080/api/v1/stepflow/events".to_string());
        let outbox_dispatcher = Arc::new(OutboxDispatcher::new(
            outbox_service.clone(),
            run_service.clone(),
            stepflow_endpoint,
        ));
        let heartbeat_supervisor = Arc::new(HeartbeatSupervisor::new(
            run_service.clone(),
            outbox_service.clone(),
        ));

        Ok(Self {
            store,
            registry: Arc::new(registry),
            orchestrator_runs,
            orchestrator_outbox,
            run_service,
            outbox_service,
            outbox_dispatcher,
            heartbeat_supervisor,
            #[cfg(feature = "authflow")]
            flow_manager,
        })
    }
}
