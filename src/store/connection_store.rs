//! Connection store interfaces and implementations
//!
//! This module provides storage abstraction for OAuth tokens and authentication state.

use crate::models::AuthConnection;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Abstract interface for connection storage
#[async_trait]
pub trait ConnectionStore: Send + Sync {
    /// Get a connection
    async fn get(&self, connection_ref: &str) -> Result<Option<AuthConnection>>;

    /// Store a connection
    async fn put(&self, connection_ref: &str, connection: &AuthConnection) -> Result<()>;

    /// Delete a connection
    async fn delete(&self, connection_ref: &str) -> Result<bool>;

    /// Compare and swap (atomic operation)
    async fn compare_and_swap(
        &self,
        connection_ref: &str,
        expected: Option<&AuthConnection>,
        new_connection: Option<&AuthConnection>,
    ) -> Result<bool>;

    /// Get fresh connection (bypass cache if any)
    async fn get_fresh(&self, connection_ref: &str) -> Result<Option<AuthConnection>> {
        self.get(connection_ref).await
    }

    /// List all connection refs
    async fn list_refs(&self) -> Result<Vec<String>>;

    /// Cleanup expired connections
    async fn cleanup_expired(&self) -> Result<u64>;

    /// Count connections
    async fn count(&self) -> Result<u64>;
}

/// In-memory connection store implementation
#[derive(Clone)]
pub struct MemoryConnectionStore {
    /// Connection data storage
    connections: Arc<RwLock<HashMap<String, AuthConnection>>>,
    /// Connection access times for TTL
    access_times: Arc<RwLock<HashMap<String, Instant>>>,
    /// Default TTL for connections
    default_ttl: Option<Duration>,
}

impl MemoryConnectionStore {
    /// Create a new memory connection store
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            access_times: Arc::new(RwLock::new(HashMap::new())),
            default_ttl: None,
        }
    }

    /// Create a new memory connection store with default TTL
    pub fn with_default_ttl(ttl: Duration) -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            access_times: Arc::new(RwLock::new(HashMap::new())),
            default_ttl: Some(ttl),
        }
    }

    /// Cleanup expired entries based on TTL
    fn cleanup_by_ttl(&self) -> Result<u64> {
        if let Some(ttl) = self.default_ttl {
            let now = Instant::now();
            let mut access_times = self.access_times.write().unwrap();
            let mut connections = self.connections.write().unwrap();

            let expired_keys: Vec<String> = access_times
                .iter()
                .filter(|(_, access_time)| now.duration_since(**access_time) > ttl)
                .map(|(key, _)| key.clone())
                .collect();

            let count = expired_keys.len() as u64;
            for key in expired_keys {
                access_times.remove(&key);
                connections.remove(&key);
            }

            Ok(count)
        } else {
            Ok(0)
        }
    }
}

impl Default for MemoryConnectionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConnectionStore for MemoryConnectionStore {
    async fn get(&self, connection_ref: &str) -> Result<Option<AuthConnection>> {
        self.cleanup_by_ttl()?;

        let connections = self.connections.read().unwrap();
        let result = connections.get(connection_ref).cloned();

        if result.is_some() {
            let mut access_times = self.access_times.write().unwrap();
            access_times.insert(connection_ref.to_string(), Instant::now());
        }

        Ok(result)
    }

    async fn put(&self, connection_ref: &str, connection: &AuthConnection) -> Result<()> {
        {
            let mut connections = self.connections.write().unwrap();
            connections.insert(connection_ref.to_string(), connection.clone());
        }
        {
            let mut access_times = self.access_times.write().unwrap();
            access_times.insert(connection_ref.to_string(), Instant::now());
        }
        Ok(())
    }

    async fn delete(&self, connection_ref: &str) -> Result<bool> {
        let existed = {
            let mut connections = self.connections.write().unwrap();
            connections.remove(connection_ref).is_some()
        };
        {
            let mut access_times = self.access_times.write().unwrap();
            access_times.remove(connection_ref);
        }
        Ok(existed)
    }

    async fn compare_and_swap(
        &self,
        connection_ref: &str,
        expected: Option<&AuthConnection>,
        new_connection: Option<&AuthConnection>,
    ) -> Result<bool> {
        let mut connections = self.connections.write().unwrap();
        let current = connections.get(connection_ref);

        let matches = match (current, expected) {
            (None, None) => true,
            (Some(curr), Some(exp)) => curr == exp,
            _ => false,
        };

        if matches {
            match new_connection {
                Some(new_conn) => {
                    connections.insert(connection_ref.to_string(), new_conn.clone());
                    let mut access_times = self.access_times.write().unwrap();
                    access_times.insert(connection_ref.to_string(), Instant::now());
                }
                None => {
                    connections.remove(connection_ref);
                    let mut access_times = self.access_times.write().unwrap();
                    access_times.remove(connection_ref);
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn list_refs(&self) -> Result<Vec<String>> {
        self.cleanup_by_ttl()?;
        let connections = self.connections.read().unwrap();
        Ok(connections.keys().cloned().collect())
    }

    async fn cleanup_expired(&self) -> Result<u64> {
        self.cleanup_by_ttl()
    }

    async fn count(&self) -> Result<u64> {
        self.cleanup_by_ttl()?;
        let connections = self.connections.read().unwrap();
        Ok(connections.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::AuthConnectionTrn;

    #[tokio::test]
    async fn test_memory_store_basic_operations() {
        let store = MemoryConnectionStore::new();
        let conn = AuthConnection::new("test_tenant", "github", "user123", "test_token").unwrap();

        // Test put and get
        store.put("test_ref", &conn).await.unwrap();
        let retrieved = store.get("test_ref").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().access_token, "test_token");
    }

    #[tokio::test]
    async fn test_memory_store_update() {
        let store = MemoryConnectionStore::new();
        let mut conn =
            AuthConnection::new("test_tenant", "github", "user123", "test_token").unwrap();

        // Store initial connection
        store.put("test_ref", &conn).await.unwrap();

        // Update the connection
        conn.update_token("new_token".to_string(), None);
        store.put("test_ref", &conn).await.unwrap();

        // Verify the update
        let retrieved = store.get("test_ref").await.unwrap().unwrap();
        assert_eq!(retrieved.access_token, "new_token");
    }

    #[tokio::test]
    async fn test_memory_store_list_and_delete() {
        let store = MemoryConnectionStore::new();
        let conn1 = AuthConnection::new("test_tenant", "github", "user1", "token1").unwrap();
        let conn2 = AuthConnection::new("test_tenant", "github", "user2", "token2").unwrap();

        // Store connections
        store.put("ref1", &conn1).await.unwrap();
        store.put("ref2", &conn2).await.unwrap();

        // Test list
        let refs = store.list_refs().await.unwrap();
        assert_eq!(refs.len(), 2);
        assert!(refs.contains(&"ref1".to_string()));
        assert!(refs.contains(&"ref2".to_string()));

        // Test delete
        let deleted = store.delete("ref1").await.unwrap();
        assert!(deleted);

        let refs = store.list_refs().await.unwrap();
        assert_eq!(refs.len(), 1);
        assert!(refs.contains(&"ref2".to_string()));

        // Test delete non-existent
        let deleted = store.delete("non_existent").await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn test_memory_store_ttl() {
        let store = MemoryConnectionStore::with_default_ttl(Duration::from_millis(100));

        let conn = AuthConnection::new("test_tenant", "github", "user123", "test_token").unwrap();
        store.put("test_ref", &conn).await.unwrap();

        // Should be available immediately
        let retrieved = store.get("test_ref").await.unwrap();
        assert!(retrieved.is_some());

        // Wait for TTL expiration
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should be expired and removed
        let retrieved = store.get("test_ref").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_memory_store_compare_and_swap() {
        let store = MemoryConnectionStore::new();

        // Test CAS with non-existent key
        let conn1 = AuthConnection::new("test_tenant", "github", "user1", "token1").unwrap();
        let success = store
            .compare_and_swap("test_ref", None, Some(&conn1))
            .await
            .unwrap();
        assert!(success);

        // Test CAS with existing key and correct expected value
        let mut conn2 = AuthConnection::new("test_tenant", "github", "user2", "token2").unwrap();
        conn2.trn = conn1.trn.clone(); // Same TRN for comparison
        let success = store
            .compare_and_swap("test_ref", Some(&conn1), Some(&conn2))
            .await
            .unwrap();
        assert!(success);

        // Test CAS with incorrect expected value
        let conn3 = AuthConnection::new("test_tenant", "github", "user3", "token3").unwrap();
        let success = store
            .compare_and_swap("test_ref", Some(&conn1), Some(&conn3))
            .await
            .unwrap();
        assert!(!success);
    }

    #[test]
    fn test_auth_connection_trn_parsing() {
        let trn = AuthConnectionTrn::new("test_tenant", "github", "user123").unwrap();
        let trn_string = trn.to_string();

        // Test that we can parse it back
        let parsed = AuthConnectionTrn::parse(&trn_string).unwrap();
        assert_eq!(parsed.tenant, "test_tenant");
        assert_eq!(parsed.provider, "github");
        assert_eq!(parsed.user_id, "user123");
    }
}
