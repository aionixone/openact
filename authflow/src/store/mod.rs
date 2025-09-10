// AuthFlow TRN identifier
pub mod auth_trn;
pub use auth_trn::*;

// Encryption service
pub mod encryption;
pub use encryption::*;

// Connection store module
pub mod connection_store;
pub use connection_store::*;

// SQLite connection store
#[cfg(feature = "sqlite")]
pub mod sqlite_connection_store;
#[cfg(feature = "sqlite")]
pub use sqlite_connection_store::*;

// Run store module
pub mod run_store;
pub use run_store::*;

// Store backend configuration and factory
use anyhow::Result;

/// Store backend type
#[derive(Debug, Clone)]
pub enum StoreBackend {
    Memory,
    #[cfg(feature = "sqlite")]
    Sqlite,
}

/// Store configuration
#[derive(Debug, Clone)]
pub struct StoreConfig {
    pub backend: StoreBackend,
    #[cfg(feature = "sqlite")]
    pub sqlite: Option<sqlite_connection_store::SqliteConfig>,
}

impl Default for StoreConfig {
    fn default() -> Self {
        Self {
            backend: StoreBackend::Memory,
            #[cfg(feature = "sqlite")]
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
        #[cfg(feature = "sqlite")]
        StoreBackend::Sqlite => {
            let sqlite_cfg = config.sqlite.unwrap_or_default();
            let store = sqlite_connection_store::SqliteConnectionStore::new(sqlite_cfg).await?;
            Ok(Arc::new(store) as Arc<dyn ConnectionStore>)
        }
    }
}
