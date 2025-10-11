//! Application state shared between MCP and REST API

use openact_core::orchestration::{OrchestratorOutboxStore, OrchestratorRunStore};
use openact_plugins as plugins;
use openact_registry::ConnectorRegistry;
use openact_store::SqlStore;
use std::sync::Arc;
use std::time::Duration;

use chrono::Duration as ChronoDuration;

#[cfg(feature = "authflow")]
use crate::flow_runner::FlowRunManager;
use crate::orchestration::{
    AsyncTaskManager, HeartbeatSupervisor, HeartbeatSupervisorConfig, OutboxDispatcher,
    OutboxDispatcherConfig, OutboxService, RunService,
};

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
    pub async_manager: Arc<AsyncTaskManager>,
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
        let dispatcher_cfg = load_outbox_config();
        let heartbeat_cfg = load_heartbeat_config();
        let stepflow_endpoint = std::env::var("OPENACT_STEPFLOW_EVENT_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:8080/api/v1/stepflow/events".to_string());
        let outbox_dispatcher = Arc::new(OutboxDispatcher::new(
            outbox_service.clone(),
            run_service.clone(),
            stepflow_endpoint,
            dispatcher_cfg,
        ));
        let heartbeat_supervisor = Arc::new(HeartbeatSupervisor::new(
            run_service.clone(),
            outbox_service.clone(),
            heartbeat_cfg,
        ));
        let async_manager =
            Arc::new(AsyncTaskManager::new(run_service.clone(), outbox_service.clone()));

        Ok(Self {
            store,
            registry: Arc::new(registry),
            orchestrator_runs,
            orchestrator_outbox,
            run_service,
            outbox_service,
            outbox_dispatcher,
            heartbeat_supervisor,
            async_manager,
            #[cfg(feature = "authflow")]
            flow_manager,
        })
    }
}

impl AppState {
    pub fn spawn_background_tasks(&self) {
        let dispatcher = self.outbox_dispatcher.clone();
        tokio::spawn(async move { dispatcher.run_loop().await });

        let heartbeat = self.heartbeat_supervisor.clone();
        tokio::spawn(async move { heartbeat.run_loop().await });
    }
}

fn load_outbox_config() -> OutboxDispatcherConfig {
    let mut cfg = OutboxDispatcherConfig::default();
    cfg.batch_size = parse_env_usize("OPENACT_OUTBOX_BATCH_SIZE", cfg.batch_size);
    cfg.interval = Duration::from_millis(parse_env_u64(
        "OPENACT_OUTBOX_INTERVAL_MS",
        cfg.interval.as_millis() as u64,
    ));
    cfg.retry_initial_backoff = ChronoDuration::milliseconds(parse_env_u64(
        "OPENACT_OUTBOX_RETRY_INITIAL_MS",
        cfg.retry_initial_backoff.num_milliseconds() as u64,
    ) as i64);
    cfg.retry_max_backoff = ChronoDuration::milliseconds(parse_env_u64(
        "OPENACT_OUTBOX_RETRY_MAX_MS",
        cfg.retry_max_backoff.num_milliseconds() as u64,
    ) as i64);
    cfg.retry_multiplier = parse_env_f64("OPENACT_OUTBOX_RETRY_FACTOR", cfg.retry_multiplier);
    cfg.retry_max_attempts =
        parse_env_u32("OPENACT_OUTBOX_RETRY_MAX_ATTEMPTS", cfg.retry_max_attempts);
    cfg
}

fn load_heartbeat_config() -> HeartbeatSupervisorConfig {
    let mut cfg = HeartbeatSupervisorConfig::default();
    cfg.batch_size = parse_env_usize("OPENACT_HEARTBEAT_BATCH_SIZE", cfg.batch_size);
    cfg.interval = Duration::from_millis(parse_env_u64(
        "OPENACT_HEARTBEAT_INTERVAL_MS",
        cfg.interval.as_millis() as u64,
    ));
    cfg.timeout_grace = ChronoDuration::milliseconds(parse_env_u64(
        "OPENACT_HEARTBEAT_GRACE_MS",
        cfg.timeout_grace.num_milliseconds() as u64,
    ) as i64);
    cfg
}

fn parse_env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(default)
}

fn parse_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(default)
}

fn parse_env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key).ok().and_then(|v| v.parse::<u32>().ok()).unwrap_or(default)
}

fn parse_env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}
