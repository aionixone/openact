#[cfg(feature = "server")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "server")]
use std::collections::HashMap;
#[cfg(feature = "server")]
use std::sync::{Arc, RwLock};
#[cfg(feature = "server")]
use std::time::SystemTime;
#[cfg(feature = "server")]
use tokio::sync::broadcast;

#[cfg(feature = "server")]
use crate::actions::{ActionRouter, DefaultRouter};
#[cfg(feature = "server")]
use crate::dsl::OpenactDsl;
#[cfg(feature = "server")]
use crate::engine::TaskHandler;
#[cfg(feature = "server")]
use openact_core::store::{AuthConnectionStore, ConnectionStore, RunStore};
#[cfg(feature = "server")]
use openact_store::{MemoryConnectionStore, MemoryRunStore};

#[cfg(all(feature = "server", feature = "openapi"))]
use utoipa::ToSchema;

#[cfg(feature = "server")]
#[derive(Clone)]
pub struct ServerState {
    pub workflows: Arc<RwLock<HashMap<String, WorkflowConfig>>>,
    pub executions: Arc<RwLock<HashMap<String, ExecutionInfo>>>,
    pub connection_store: Arc<dyn ConnectionStore>,
    pub run_store: Arc<dyn RunStore>,
    pub task_handler: Arc<dyn TaskHandler>,
    pub ws_broadcaster: broadcast::Sender<ExecutionEvent>,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionEvent {
    pub event_type: String,
    pub execution_id: String,
    pub timestamp: SystemTime,
    pub data: serde_json::Value,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub dsl: OpenactDsl,
    pub status: WorkflowStatus,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub enum WorkflowStatus {
    Active,
    Inactive,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateHistoryEntry {
    pub state: String,
    pub status: String,
    pub entered_at: SystemTime,
    pub exited_at: Option<SystemTime>,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(all(feature = "server", feature = "openapi"), derive(ToSchema))]
pub enum ExecutionStatus {
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[cfg(feature = "server")]
impl ServerState {
    pub fn new() -> Self {
        let (tx, _rx) = broadcast::channel(100);
        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            connection_store: Arc::new(MemoryConnectionStore::new()),
            run_store: Arc::new(MemoryRunStore::default()),
            task_handler: Arc::new(DefaultRouter),
            ws_broadcaster: tx,
        }
    }

    pub async fn from_env() -> Self {
        let (tx, _rx) = broadcast::channel(100);
        // Use persistent SQLite store for full functionality
        let database_url = std::env::var("OPENACT_DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://data/openact.db".to_string());

        let sql_store = match openact_store::SqlStore::new(&database_url).await {
            Ok(store) => Arc::new(store),
            Err(_) => {
                // Fallback to memory stores if database connection fails
                return Self::new();
            }
        };

        let connection_store: Arc<dyn ConnectionStore> = sql_store.clone();
        let auth_store: Arc<dyn AuthConnectionStore> = sql_store.clone();
        let run_store: Arc<dyn RunStore> = sql_store.clone();

        Self {
            workflows: Arc::new(RwLock::new(HashMap::new())),
            executions: Arc::new(RwLock::new(HashMap::new())),
            connection_store,
            run_store,
            task_handler: Arc::new(ActionRouter::new(auth_store)),
            ws_broadcaster: tx,
        }
    }

    pub fn broadcast_event(&self, event: ExecutionEvent) {
        let _ = self.ws_broadcaster.send(event);
    }
}
