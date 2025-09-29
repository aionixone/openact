use async_trait::async_trait;
use openact_core::{
    store::{ActionRepository, AuthConnectionStore, ConnectionStore, RunStore},
    ActionRecord, AuthConnection, Checkpoint, ConnectionRecord, ConnectorKind, CoreResult, Trn,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory implementation of ConnectionStore for testing
#[derive(Debug, Clone)]
pub struct MemoryConnectionStore {
    data: Arc<RwLock<HashMap<String, ConnectionRecord>>>,
}

impl MemoryConnectionStore {
    pub fn new() -> Self {
        Self { data: Arc::new(RwLock::new(HashMap::new())) }
    }
}

impl Default for MemoryConnectionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConnectionStore for MemoryConnectionStore {
    async fn upsert(&self, record: &ConnectionRecord) -> CoreResult<()> {
        let mut data = self.data.write().await;
        data.insert(record.trn.as_str().to_string(), record.clone());
        Ok(())
    }

    async fn get(&self, trn: &Trn) -> CoreResult<Option<ConnectionRecord>> {
        let data = self.data.read().await;
        Ok(data.get(trn.as_str()).cloned())
    }

    async fn delete(&self, trn: &Trn) -> CoreResult<bool> {
        let mut data = self.data.write().await;
        Ok(data.remove(trn.as_str()).is_some())
    }

    async fn list_by_connector(&self, connector: &str) -> CoreResult<Vec<ConnectionRecord>> {
        let data = self.data.read().await;
        let results = data
            .values()
            .filter(|record| record.connector.as_str() == connector)
            .cloned()
            .collect();
        Ok(results)
    }

    async fn list_distinct_connectors(&self) -> CoreResult<Vec<ConnectorKind>> {
        let data = self.data.read().await;
        let mut connectors = std::collections::HashSet::new();
        for record in data.values() {
            connectors.insert(record.connector.clone());
        }
        Ok(connectors.into_iter().collect())
    }
}

/// In-memory implementation of ActionRepository for testing
#[derive(Debug, Clone)]
pub struct MemoryActionRepository {
    data: Arc<RwLock<HashMap<String, ActionRecord>>>,
}

impl MemoryActionRepository {
    pub fn new() -> Self {
        Self { data: Arc::new(RwLock::new(HashMap::new())) }
    }
}

impl Default for MemoryActionRepository {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ActionRepository for MemoryActionRepository {
    async fn upsert(&self, record: &ActionRecord) -> CoreResult<()> {
        let mut data = self.data.write().await;
        data.insert(record.trn.as_str().to_string(), record.clone());
        Ok(())
    }

    async fn get(&self, trn: &Trn) -> CoreResult<Option<ActionRecord>> {
        let data = self.data.read().await;
        Ok(data.get(trn.as_str()).cloned())
    }

    async fn delete(&self, trn: &Trn) -> CoreResult<bool> {
        let mut data = self.data.write().await;
        Ok(data.remove(trn.as_str()).is_some())
    }

    async fn list_by_connection(&self, connection_trn: &Trn) -> CoreResult<Vec<ActionRecord>> {
        let data = self.data.read().await;
        let results = data
            .values()
            .filter(|record| record.connection_trn.as_str() == connection_trn.as_str())
            .cloned()
            .collect();
        Ok(results)
    }

    async fn list_by_connector(&self, connector: &ConnectorKind) -> CoreResult<Vec<ActionRecord>> {
        let data = self.data.read().await;
        let results =
            data.values().filter(|record| record.connector == *connector).cloned().collect();
        Ok(results)
    }
}

/// In-memory implementation of RunStore for testing
#[derive(Debug, Clone)]
pub struct MemoryRunStore {
    data: Arc<RwLock<HashMap<String, Checkpoint>>>,
}

/// In-memory store for auth connections (OAuth tokens)
#[derive(Debug, Clone)]
pub struct MemoryAuthConnectionStore {
    data: Arc<RwLock<HashMap<String, AuthConnection>>>,
}

impl MemoryRunStore {
    pub fn new() -> Self {
        Self { data: Arc::new(RwLock::new(HashMap::new())) }
    }
}

impl MemoryAuthConnectionStore {
    pub fn new() -> Self {
        Self { data: Arc::new(RwLock::new(HashMap::new())) }
    }
}

impl Default for MemoryRunStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for MemoryAuthConnectionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RunStore for MemoryRunStore {
    async fn put(&self, cp: Checkpoint) -> CoreResult<()> {
        let mut data = self.data.write().await;
        data.insert(cp.run_id.clone(), cp);
        Ok(())
    }

    async fn get(&self, run_id: &str) -> CoreResult<Option<Checkpoint>> {
        let data = self.data.read().await;
        Ok(data.get(run_id).cloned())
    }

    async fn delete(&self, run_id: &str) -> CoreResult<bool> {
        let mut data = self.data.write().await;
        Ok(data.remove(run_id).is_some())
    }
}

#[async_trait]
impl AuthConnectionStore for MemoryAuthConnectionStore {
    async fn get(&self, auth_ref: &str) -> CoreResult<Option<AuthConnection>> {
        let data = self.data.read().await;
        Ok(data.get(auth_ref).cloned())
    }

    async fn put(&self, auth_ref: &str, connection: &AuthConnection) -> CoreResult<()> {
        let mut data = self.data.write().await;
        data.insert(auth_ref.to_string(), connection.clone());
        Ok(())
    }

    async fn delete(&self, auth_ref: &str) -> CoreResult<bool> {
        let mut data = self.data.write().await;
        Ok(data.remove(auth_ref).is_some())
    }

    async fn compare_and_swap(
        &self,
        auth_ref: &str,
        expected: Option<&AuthConnection>,
        new_connection: Option<&AuthConnection>,
    ) -> CoreResult<bool> {
        let mut data = self.data.write().await;
        let current = data.get(auth_ref);

        let matches = match (current, expected) {
            (None, None) => true,
            (Some(curr), Some(exp)) => curr == exp,
            _ => false,
        };

        if matches {
            match new_connection {
                Some(new_conn) => {
                    data.insert(auth_ref.to_string(), new_conn.clone());
                }
                None => {
                    data.remove(auth_ref);
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn list_refs(&self) -> CoreResult<Vec<String>> {
        let data = self.data.read().await;
        Ok(data.keys().cloned().collect())
    }

    async fn cleanup_expired(&self) -> CoreResult<u64> {
        let mut data = self.data.write().await;
        let mut expired_keys = Vec::new();

        for (key, connection) in data.iter() {
            if connection.is_expired() {
                expired_keys.push(key.clone());
            }
        }

        let count = expired_keys.len() as u64;
        for key in expired_keys {
            data.remove(&key);
        }

        Ok(count)
    }

    async fn count(&self) -> CoreResult<u64> {
        let data = self.data.read().await;
        Ok(data.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use openact_core::ConnectorKind;
    use serde_json::json;

    #[tokio::test]
    async fn test_memory_connection_store() {
        let store = MemoryConnectionStore::new();

        let record = ConnectionRecord {
            trn: Trn::new("trn:openact:test:connection/http/github@v1".to_string()),
            connector: ConnectorKind::new("http"),
            name: "github".to_string(),
            config_json: json!({"base_url": "https://api.github.com"}),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        };

        // Test upsert
        store.upsert(&record).await.unwrap();

        // Test get
        let retrieved = store.get(&record.trn).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "github");

        // Test list by connector
        let connections = store.list_by_connector("http").await.unwrap();
        assert_eq!(connections.len(), 1);

        // Test delete
        let deleted = store.delete(&record.trn).await.unwrap();
        assert!(deleted);

        // Verify deletion
        let retrieved = store.get(&record.trn).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_memory_action_repository() {
        let repo = MemoryActionRepository::new();

        let record = ActionRecord {
            trn: Trn::new("trn:openact:test:action/http/get-user@v1".to_string()),
            connector: ConnectorKind::new("http"),
            name: "get-user".to_string(),
            connection_trn: Trn::new("trn:openact:test:connection/http/github@v1".to_string()),
            config_json: json!({"method": "GET", "path": "/user"}),
            mcp_enabled: false,
            mcp_overrides: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        };

        // Test upsert
        repo.upsert(&record).await.unwrap();

        // Test get
        let retrieved = repo.get(&record.trn).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "get-user");

        // Test list by connection
        let actions = repo.list_by_connection(&record.connection_trn).await.unwrap();
        assert_eq!(actions.len(), 1);

        // Test delete
        let deleted = repo.delete(&record.trn).await.unwrap();
        assert!(deleted);
    }

    #[tokio::test]
    async fn test_memory_run_store() {
        let store = MemoryRunStore::new();

        let checkpoint = Checkpoint {
            run_id: "test-run-123".to_string(),
            paused_state: "waiting_for_auth".to_string(),
            context_json: json!({"step": 1, "data": "test"}),
            await_meta_json: Some(json!({"state": "abc123"})),
        };

        // Test put
        store.put(checkpoint.clone()).await.unwrap();

        // Test get
        let retrieved = store.get(&checkpoint.run_id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().paused_state, "waiting_for_auth");

        // Test delete
        let deleted = store.delete(&checkpoint.run_id).await.unwrap();
        assert!(deleted);

        // Verify deletion
        let retrieved = store.get(&checkpoint.run_id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_memory_auth_connection_store() {
        let store = MemoryAuthConnectionStore::new();

        let auth_connection =
            AuthConnection::new("test_tenant", "github", "user123", "access_token_456");
        let trn = auth_connection.trn.clone();

        // Test put and get
        store.put(&trn, &auth_connection).await.unwrap();
        let retrieved = store.get(&trn).await.unwrap();
        assert!(retrieved.is_some());

        let retrieved_auth = retrieved.unwrap();
        assert_eq!(retrieved_auth.tenant, "test_tenant");
        assert_eq!(retrieved_auth.provider, "github");
        assert_eq!(retrieved_auth.user_id, "user123");
        assert_eq!(retrieved_auth.access_token, "access_token_456");
        assert_eq!(retrieved_auth.token_type, "Bearer");

        // Test list_refs
        let refs = store.list_refs().await.unwrap();
        assert_eq!(refs.len(), 1);
        assert!(refs.contains(&trn));

        // Test delete
        let deleted = store.delete(&trn).await.unwrap();
        assert!(deleted);

        let refs = store.list_refs().await.unwrap();
        assert_eq!(refs.len(), 0);
    }
}
