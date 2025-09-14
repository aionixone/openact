use crate::error::{CoreError, Result};
use authflow::store::{create_connection_store, ConnectionStore, StoreBackend, StoreConfig};
use std::sync::Arc;

#[derive(Clone)]
pub struct AuthManager {
    store: Arc<dyn ConnectionStore>,
}

impl AuthManager {
    pub async fn from_database_url(database_url: String) -> Result<Self> {
        let mut cfg = StoreConfig {
            backend: StoreBackend::Sqlite,
            sqlite: None,
        };
        cfg.sqlite = Some(authflow::store::sqlite_connection_store::SqliteConfig {
            database_url,
            ..Default::default()
        });
        let store = create_connection_store(cfg)
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
        Ok(Self { store })
    }

    /// Create a PAT-based connection and store it; returns the created TRN
    pub async fn create_pat_connection(
        &self,
        tenant: &str,
        provider: &str,
        user_id: &str,
        token: &str,
    ) -> Result<String> {
        let connection = authflow::store::Connection::new(tenant, provider, user_id, token)
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
        let trn = connection.connection_id();
        self.store
            .put(&trn, &connection)
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
        Ok(trn)
    }

    pub fn store(&self) -> Arc<dyn ConnectionStore> {
        self.store.clone()
    }

    /// List all connection TRNs (references)
    pub async fn list(&self) -> Result<Vec<String>> {
        self.store
            .list_refs()
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))
    }

    /// Get a connection by TRN
    pub async fn get(&self, trn: &str) -> Result<Option<authflow::store::Connection>> {
        self.store
            .get(trn)
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))
    }

    /// Delete a connection by TRN
    pub async fn delete(&self, trn: &str) -> Result<bool> {
        self.store
            .delete(trn)
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))
    }

    /// Cleanup expired connections
    pub async fn cleanup_expired(&self) -> Result<usize> {
        self.store
            .cleanup_expired()
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))
    }

    /// Count connections
    pub async fn count(&self) -> Result<usize> {
        self.store
            .count()
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))
    }

    /// Refresh connection tokens
    pub async fn refresh_connection(
        &self,
        trn: &str,
        access_token: &str,
        refresh_token: Option<&str>,
        expires_in: Option<i64>,
    ) -> Result<()> {
        // Get current connection
        let mut connection = self
            .store
            .get(trn)
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?
            .ok_or_else(|| CoreError::InvalidInput("connection not found".to_string()))?;

        // Update tokens
        connection.update_access_token(access_token);
        if let Some(rt) = refresh_token {
            connection.update_refresh_token(Some(rt.to_string()));
        }
        if let Some(exp) = expires_in {
            connection = connection.with_expires_in(exp);
        }

        // Save updated connection
        self.store
            .put(trn, &connection)
            .await
            .map_err(|e| CoreError::InvalidInput(e.to_string()))?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn init_store_with_memory_db_url() {
        // sqlite::memory: style URL will still attempt sqlite config; ensure it doesn't panic
        let m = AuthManager::from_database_url("sqlite::memory:".to_string()).await;
        assert!(m.is_ok());
        let store = m.unwrap().store();
        // List refs should work and be empty
        let refs = store.list_refs().await.unwrap();
        assert!(refs.is_empty());
    }

    #[tokio::test]
    async fn list_count_cleanup_on_memory() {
        let m = AuthManager::from_database_url("sqlite::memory:".to_string())
            .await
            .unwrap();
        // no data by default
        assert_eq!(m.count().await.unwrap(), 0);
        // cleanup safe
        let _ = m.cleanup_expired().await.unwrap();
        assert!(m.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn real_sqlite_db_smoke_test() {
        // Use project DB; run manually
        let m = AuthManager::from_database_url("sqlite:./data/openact.db".to_string())
            .await
            .unwrap();
        let _ = m.count().await.unwrap();
        let _ = m.cleanup_expired().await.unwrap();
    }
}
