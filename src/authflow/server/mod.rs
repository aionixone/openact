//! openact Web Server
//!
//! Provides REST API for workflow management and execution monitoring

// ws handlers moved; remove unused imports
// router implemented in server::router
// use futures::{sink::SinkExt, stream::StreamExt};
#[cfg(feature = "server")]
use serde::{Deserialize, Serialize};
// #[cfg(feature = "server")]
// use serde_json::json;
#[cfg(feature = "server")]
// use uuid::Uuid;
#[cfg(feature = "server")]
use crate::authflow::dsl::OpenactDsl;
#[cfg(feature = "server")]
use crate::{
    authflow::actions::{ActionRouter, DefaultRouter},
    authflow::engine::TaskHandler,
    store::{
        ConnectionStore, MemoryConnectionStore, MemoryRunStore, StoreBackend, StoreConfig,
        create_connection_store,
    },
};
#[cfg(feature = "server")]
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::SystemTime,
};
#[cfg(feature = "server")]
use tokio::sync::broadcast;
pub mod dto;
#[cfg(feature = "server")]
// use chrono::{DateTime, Utc};

// Facade submodules for incremental refactor
pub mod router;
pub mod runtime;
pub mod state;
pub mod utils;
pub mod handlers {
    pub mod executions;
    pub mod health;
    pub mod oauth;
    pub mod workflows;
    pub mod ws;
}

/// Workflow server state
#[cfg(feature = "server")]
#[derive(Clone)]
pub struct ServerState {
    /// Workflow storage
    pub workflows: Arc<RwLock<HashMap<String, WorkflowConfig>>>,
    /// Execution storage
    pub executions: Arc<RwLock<HashMap<String, ExecutionInfo>>>,
    /// Connection storage
    pub connection_store: Arc<dyn ConnectionStore>,
    /// Run storage
    pub run_store: Arc<MemoryRunStore>,
    /// Task handler
    pub task_handler: Arc<dyn TaskHandler>,
    /// WebSocket broadcast channel
    pub ws_broadcaster: broadcast::Sender<ExecutionEvent>,
}

/// Execution event
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionEvent {
    pub event_type: String,
    pub execution_id: String,
    pub timestamp: SystemTime,
    pub data: serde_json::Value,
}

/// Workflow configuration
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowConfig {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub dsl: OpenactDsl,
    pub status: WorkflowStatus,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

/// Workflow status
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowStatus {
    Active,
    Inactive,
    Draft,
}

/// Create workflow request
#[cfg(feature = "server")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkflowRequest {
    pub name: String,
    pub description: Option<String>,
    // Accept raw JSON for normalization (parameters/inlineTemplate folding, provider.config injection)
    pub dsl: serde_json::Value,
}

/// Execution information
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionInfo {
    pub execution_id: String,
    pub workflow_id: String,
    pub flow: String,
    pub status: ExecutionStatus,
    pub current_state: Option<String>,
    pub started_at: SystemTime,
    pub updated_at: SystemTime,
    pub completed_at: Option<SystemTime>,
    pub input: serde_json::Value,
    pub context: Option<serde_json::Value>,
    pub error: Option<String>,
    pub state_history: Vec<StateHistoryEntry>,
}

/// Execution status
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionStatus {
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// State history entry
#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateHistoryEntry {
    pub state: String,
    pub status: String,
    pub entered_at: SystemTime,
    pub exited_at: Option<SystemTime>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Start execution request
#[cfg(feature = "server")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartExecutionRequest {
    pub workflow_id: String,
    pub flow: String,
    pub input: serde_json::Value,
    pub context: Option<serde_json::Value>,
}

/// Resume execution request
#[cfg(feature = "server")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResumeExecutionRequest {
    pub input: serde_json::Value,
}

#[cfg(feature = "server")]
impl ServerState {
    pub fn new() -> Self {
        let (ws_broadcaster, _) = broadcast::channel(1000);
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            connection_store: Arc::new(MemoryConnectionStore::new()) as Arc<dyn ConnectionStore>,
            run_store: Arc::new(MemoryRunStore::default()),
            task_handler: Arc::new(DefaultRouter) as Arc<dyn TaskHandler>,
            ws_broadcaster,
        }
    }

    /// Create from environment variables (supports switching storage backend)
    pub async fn from_env() -> Self {
        let (ws_broadcaster, _) = broadcast::channel(1000);
        // openact_STORE: memory | sqlite
        let store_env = std::env::var("openact_STORE").unwrap_or_else(|_| "memory".to_string());
        println!("[server] openact_STORE environment variable: {}", store_env);
        let mut backend = StoreBackend::Memory;
        
        if store_env.eq_ignore_ascii_case("sqlite") {
            backend = StoreBackend::Sqlite;
            println!("[server] Using SQLite backend");
        } else {
            println!(
                "[server] Using Memory backend (sqlite feature enabled but openact_STORE != 'sqlite')"
            );
        }
        // Note: sqlite feature flag removed; fallback to memory if not configured

        let mut cfg = StoreConfig {
            backend,
            ..Default::default()
        };
        
        if let Ok(db_url) =
            std::env::var("OPENACT_DATABASE_URL").or_else(|_| std::env::var("openact_SQLITE_URL"))
        {
            use crate::store::sqlite_connection_store::SqliteConfig;
            cfg.sqlite = Some(SqliteConfig {
                database_url: db_url,
                ..Default::default()
            });
        }

        let connection_store = create_connection_store(cfg).await.unwrap_or_else(|e| {
                eprintln!("[server] Failed to create connection store: {:?}", e);
                eprintln!("[server] Falling back to MemoryConnectionStore");
                Arc::new(MemoryConnectionStore::new()) as Arc<dyn ConnectionStore>
            });
        let router: Arc<dyn TaskHandler> = Arc::new(ActionRouter::new(connection_store.clone()));
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            connection_store,
            run_store: Arc::new(MemoryRunStore::default()),
            task_handler: router,
            ws_broadcaster,
        }
    }

    /// Send execution event
    pub fn broadcast_event(&self, event: ExecutionEvent) {
        let _ = self.ws_broadcaster.send(event);
    }
}

pub use crate::authflow::server::router::{
    create_router, create_router_async, create_router_with_state,
};

#[cfg(feature = "server")]
async fn execute_workflow(state: ServerState, execution_id: String) {
    crate::authflow::server::runtime::execute_workflow(state, execution_id).await
}

#[cfg(not(feature = "server"))]
pub fn create_router() -> () {
    panic!("Server feature is not enabled. Please compile with --features server");
}
