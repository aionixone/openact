//! Application state management for MCP server

use openact_store::SqlStore;
use std::sync::Arc;

/// Shared application state for MCP server
///
/// This contains all the core OpenAct components that the MCP server needs
/// to execute actions and manage configurations.
#[derive(Clone)]
pub struct AppState {
    /// Database store for persistent data
    pub store: Arc<SqlStore>,
}

impl AppState {
    /// Create a new AppState
    pub fn new(store: SqlStore) -> Self {
        Self { store: Arc::new(store) }
    }

    /// Create a new AppState from an Arc<SqlStore> without cloning the inner store
    pub fn from_arc(store: Arc<SqlStore>) -> Self {
        Self { store }
    }

    /// Create AppState from database path
    pub async fn from_db_path(db_path: &str) -> crate::McpResult<Self> {
        // Initialize store
        let store = SqlStore::new(db_path).await?;
        store.migrate().await?;

        Ok(Self::new(store))
    }
}
