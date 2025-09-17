// AuthFlow TRN identifier
pub mod auth_trn;
pub use auth_trn::*;

// Connection store module
pub mod connection_store;
pub use connection_store::*;

// Re-export unified encryption from storage
pub use openact_storage::encryption::*;

// DB-backed connection store adapter
pub mod sqlite_connection_store;
pub use sqlite_connection_store::DbConnectionStore;

// Run store module
pub mod run_store;
pub use run_store::*;

// Store backend configuration and factory
use anyhow::Result;
use std::sync::Arc;

/// Store backend type
#[derive(Debug, Clone)]
pub enum StoreBackend {
    Memory,
    Sqlite,
}

/// Store configuration
#[derive(Debug, Clone)]
pub struct StoreConfig {
    pub backend: StoreBackend,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self { backend: StoreBackend::Memory }
    }
}

/// Factory method to create a ConnectionStore
pub async fn create_connection_store(_config: StoreConfig) -> Result<Arc<dyn ConnectionStore>> {
    // For now, always use DB adapter (Memory still available via direct use)
    Ok(Arc::new(DbConnectionStore::new().await?) as Arc<dyn ConnectionStore>)
}
