// openact TRN identifier
pub mod auth_trn;
pub use auth_trn::*;

// Encryption service
pub mod encryption;
pub use encryption::*;

// Auth connection store module (for OAuth tokens)
pub mod connection_store;
pub use connection_store::*;

// Database and repositories
pub mod database;
pub mod sqlite_connection_store;
pub mod connection_repository;
pub mod task_repository;
pub mod service;

pub use database::*;
pub use sqlite_connection_store::*;
pub use connection_repository::*;
pub use task_repository::*;
pub use service::*;

// Run store module
pub mod run_store;
pub use run_store::*;

// Store backend configuration and factory
use anyhow::Result;

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
    
    pub sqlite: Option<sqlite_connection_store::SqliteConfig>,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            backend: StoreBackend::Memory,
            
            sqlite: None,
        }
    }
}

use std::sync::Arc;

/// Factory method to create a ConnectionStore
pub async fn create_connection_store(config: StoreConfig) -> Result<Arc<dyn ConnectionStore>> {
    match config.backend {
        StoreBackend::Memory => {
            Ok(Arc::new(MemoryConnectionStore::new()) as Arc<dyn ConnectionStore>)
        }
        
        StoreBackend::Sqlite => {
            let mut sqlite_cfg = config.sqlite.unwrap_or_default();
            // Align with OPENACT_DB_URL if provided
            if let Ok(url) = std::env::var("OPENACT_DB_URL") {
                sqlite_cfg.database_url = if url.starts_with("sqlite:") { url } else { format!("sqlite:{}", url) };
            }
            // Disable encryption if no master key provided (tests/dev)
            if std::env::var("OPENACT_MASTER_KEY").is_err() {
                sqlite_cfg.enable_encryption = false;
            }
            let store = sqlite_connection_store::SqliteConnectionStore::new(sqlite_cfg).await?;
            Ok(Arc::new(store) as Arc<dyn ConnectionStore>)
        }
    }
}
