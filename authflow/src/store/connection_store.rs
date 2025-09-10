use super::auth_trn::AuthConnectionTrn;
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Connection state, including authentication tokens and metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    /// TRN identifier
    pub trn: AuthConnectionTrn,
    /// Access token
    pub access_token: String,
    /// Refresh token (optional)
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Token expiration time (ISO8601 format)
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    /// Token type (usually "Bearer")
    #[serde(default = "default_token_type")]
    pub token_type: String,
    /// Authorization scope
    #[serde(default)]
    pub scope: Option<String>,
    /// Additional metadata
    #[serde(default)]
    pub extra: Value,
    /// Creation time
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    /// Last update time
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

impl Connection {
    /// Create a new connection
    pub fn new(
        tenant: impl Into<String>,
        provider: impl Into<String>,
        user_id: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Result<Self> {
        let now = Utc::now();
        let trn = AuthConnectionTrn::new(tenant, provider, user_id)?;

        Ok(Self {
            trn,
            access_token: access_token.into(),
            refresh_token: None,
            expires_at: None,
            token_type: default_token_type(),
            scope: None,
            extra: Value::Null,
            created_at: now,
            updated_at: now,
        })
    }

    /// Get the TRN identifier of the connection
    pub fn connection_id(&self) -> String {
        self.trn
            .to_trn_string()
            .unwrap_or_else(|_| "invalid-trn".to_string())
    }

    /// Get the unique key of the connection (for database storage)
    pub fn connection_key(&self) -> String {
        self.trn.connection_key()
    }

    /// Set the refresh token
    pub fn with_refresh_token(mut self, refresh_token: impl Into<String>) -> Self {
        self.refresh_token = Some(refresh_token.into());
        self
    }

    /// Set the expiration time
    pub fn with_expires_at(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }

    /// Set the expiration time (in seconds from now)
    pub fn with_expires_in(mut self, expires_in_seconds: i64) -> Self {
        self.expires_at = Some(Utc::now() + chrono::Duration::seconds(expires_in_seconds));
        self
    }

    /// Set the authorization scope
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }

    /// Set additional metadata
    pub fn with_extra(mut self, extra: Value) -> Self {
        self.extra = extra;
        self
    }

    /// Check if the token is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| Utc::now() > exp).unwrap_or(false)
    }

    /// Check if the token is expiring soon (default 5 minutes early)
    pub fn is_expiring_soon(&self, buffer: Option<Duration>) -> bool {
        let buffer = buffer.unwrap_or(Duration::from_secs(300)); // 5 minutes
        self.expires_at
            .map(|exp| Utc::now() + chrono::Duration::from_std(buffer).unwrap() > exp)
            .unwrap_or(false)
    }

    /// Update the access token
    pub fn update_access_token(&mut self, access_token: impl Into<String>) {
        self.access_token = access_token.into();
        self.updated_at = Utc::now();
    }

    /// Update the refresh token
    pub fn update_refresh_token(&mut self, refresh_token: Option<String>) {
        self.refresh_token = refresh_token;
        self.updated_at = Utc::now();
    }
}

impl Default for Connection {
    fn default() -> Self {
        Self::new("default", "unknown", "unknown", "").unwrap_or_else(|_| {
            // If creation fails, create a minimal valid connection
            let trn = AuthConnectionTrn::new("default", "unknown", "unknown").unwrap();
            let now = Utc::now();
            Self {
                trn,
                access_token: String::new(),
                refresh_token: None,
                expires_at: None,
                token_type: default_token_type(),
                scope: None,
                extra: Value::Null,
                created_at: now,
                updated_at: now,
            }
        })
    }
}

/// Abstract interface for connection storage
#[async_trait]
pub trait ConnectionStore: Send + Sync {
    /// Get a connection
    async fn get(&self, connection_ref: &str) -> Result<Option<Connection>>;

    /// Store a connection
    async fn put(&self, connection_ref: &str, connection: &Connection) -> Result<()>;

    /// Delete a connection
    async fn delete(&self, connection_ref: &str) -> Result<bool>;

    /// Compare and swap (atomic operation)
    async fn compare_and_swap(
        &self,
        connection_ref: &str,
        expected: Option<&Connection>,
        new_connection: Option<&Connection>,
    ) -> Result<bool>;

    /// Get a fresh connection (automatically check expiration)
    async fn get_fresh(&self, connection_ref: &str) -> Result<Option<Connection>> {
        if let Some(conn) = self.get(connection_ref).await? {
            if !conn.is_expired() {
                return Ok(Some(conn));
            }
        }
        Ok(None)
    }

    /// List all connection references
    async fn list_refs(&self) -> Result<Vec<String>>;

    /// Clean up expired connections
    async fn cleanup_expired(&self) -> Result<usize>;

    /// Get the number of connections
    async fn count(&self) -> Result<usize>;
}

/// In-memory implementation of connection storage
#[derive(Debug, Clone)]
pub struct MemoryConnectionStore {
    /// Connection data storage
    connections: Arc<RwLock<HashMap<String, Connection>>>,
    /// TTL tracking (connection reference -> expiration time)
    ttl_tracker: Arc<RwLock<HashMap<String, Instant>>>,
    /// Default TTL
    default_ttl: Option<Duration>,
}

impl MemoryConnectionStore {
    /// Create a new in-memory connection store
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            ttl_tracker: Arc::new(RwLock::new(HashMap::new())),
            default_ttl: Some(Duration::from_secs(3600)), // Default 1 hour TTL
        }
    }

    /// Set the default TTL
    pub fn with_default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = Some(ttl);
        self
    }

    /// Disable TTL
    pub fn without_ttl(mut self) -> Self {
        self.default_ttl = None;
        self
    }

    /// Set the TTL for a connection
    pub fn set_ttl(&self, connection_ref: &str, ttl: Duration) -> Result<()> {
        let mut ttl_map = self.ttl_tracker.write().unwrap();
        ttl_map.insert(connection_ref.to_string(), Instant::now() + ttl);
        Ok(())
    }

    /// Check if a connection is expired due to TTL
    fn is_ttl_expired(&self, connection_ref: &str) -> bool {
        let ttl_map = self.ttl_tracker.read().unwrap();
        if let Some(expiry) = ttl_map.get(connection_ref) {
            Instant::now() > *expiry
        } else {
            false
        }
    }

    /// Clean up connections expired due to TTL
    fn cleanup_ttl_expired(&self) -> Result<usize> {
        let now = Instant::now();
        let mut ttl_map = self.ttl_tracker.write().unwrap();
        let mut connections = self.connections.write().unwrap();

        let expired_refs: Vec<String> = ttl_map
            .iter()
            .filter(|(_, expiry)| now > **expiry)
            .map(|(k, _)| k.clone())
            .collect();

        let count = expired_refs.len();
        for ref_id in expired_refs {
            ttl_map.remove(&ref_id);
            connections.remove(&ref_id);
        }

        Ok(count)
    }
}

impl Default for MemoryConnectionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConnectionStore for MemoryConnectionStore {
    async fn get(&self, connection_ref: &str) -> Result<Option<Connection>> {
        // First check if TTL is expired
        if self.is_ttl_expired(connection_ref) {
            // TTL expired, delete connection
            let mut connections = self.connections.write().unwrap();
            let mut ttl_map = self.ttl_tracker.write().unwrap();
            connections.remove(connection_ref);
            ttl_map.remove(connection_ref);
            return Ok(None);
        }

        let connections = self.connections.read().unwrap();
        Ok(connections.get(connection_ref).cloned())
    }

    async fn put(&self, connection_ref: &str, connection: &Connection) -> Result<()> {
        let mut connections = self.connections.write().unwrap();
        connections.insert(connection_ref.to_string(), connection.clone());

        // Set TTL
        if let Some(ttl) = self.default_ttl {
            let mut ttl_map = self.ttl_tracker.write().unwrap();
            ttl_map.insert(connection_ref.to_string(), Instant::now() + ttl);
        }

        Ok(())
    }

    async fn delete(&self, connection_ref: &str) -> Result<bool> {
        let mut connections = self.connections.write().unwrap();
        let mut ttl_map = self.ttl_tracker.write().unwrap();

        let existed = connections.remove(connection_ref).is_some();
        ttl_map.remove(connection_ref);

        Ok(existed)
    }

    async fn compare_and_swap(
        &self,
        connection_ref: &str,
        expected: Option<&Connection>,
        new_connection: Option<&Connection>,
    ) -> Result<bool> {
        let mut connections = self.connections.write().unwrap();

        match (connections.get(connection_ref), expected) {
            // Expected is None, actual is also None -> insert new connection
            (None, None) => {
                if let Some(new_conn) = new_connection {
                    connections.insert(connection_ref.to_string(), new_conn.clone());

                    // Set TTL
                    if let Some(ttl) = self.default_ttl {
                        let mut ttl_map = self.ttl_tracker.write().unwrap();
                        ttl_map.insert(connection_ref.to_string(), Instant::now() + ttl);
                    }

                    // Set TTL
                    if let Some(ttl) = self.default_ttl {
                        let mut ttl_map = self.ttl_tracker.write().unwrap();
                        ttl_map.insert(connection_ref.to_string(), Instant::now() + ttl);
                    }

                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            // Expected connection matches actual connection -> update
            (Some(current), Some(expected_conn)) if current == expected_conn => {
                if let Some(new_conn) = new_connection {
                    connections.insert(connection_ref.to_string(), new_conn.clone());
                    Ok(true)
                } else {
                    connections.remove(connection_ref);
                    Ok(true)
                }
            }
            // Other cases -> CAS failed
            _ => Ok(false),
        }
    }

    async fn list_refs(&self) -> Result<Vec<String>> {
        let connections = self.connections.read().unwrap();
        Ok(connections.keys().cloned().collect())
    }

    async fn cleanup_expired(&self) -> Result<usize> {
        let mut total_cleaned = 0;

        // Clean up connections expired due to TTL
        total_cleaned += self.cleanup_ttl_expired()?;

        // Clean up connections expired due to token expiration
        let mut connections = self.connections.write().unwrap();
        let expired_refs: Vec<String> = connections
            .iter()
            .filter(|(_, conn)| conn.is_expired())
            .map(|(k, _)| k.clone())
            .collect();

        let token_expired_count = expired_refs.len();
        for ref_id in expired_refs {
            connections.remove(&ref_id);
        }

        total_cleaned += token_expired_count;
        Ok(total_cleaned)
    }

    async fn count(&self) -> Result<usize> {
        let connections = self.connections.read().unwrap();
        Ok(connections.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration as TokioDuration, sleep};

    #[tokio::test]
    async fn test_connection_creation() {
        let conn = Connection::new("test_tenant", "github", "user123", "test_token")
            .unwrap()
            .with_refresh_token("refresh_token")
            .with_expires_in(3600)
            .with_scope("read write");

        assert_eq!(conn.access_token, "test_token");
        assert_eq!(conn.refresh_token, Some("refresh_token".to_string()));
        assert_eq!(conn.scope, Some("read write".to_string()));
        assert!(!conn.is_expired());
    }

    #[tokio::test]
    async fn test_connection_expiry() {
        let mut conn = Connection::new("test_tenant", "github", "user123", "test_token").unwrap();

        // Set as expired
        conn.expires_at = Some(Utc::now() - chrono::Duration::seconds(10));
        assert!(conn.is_expired());

        // Set as expiring soon
        conn.expires_at = Some(Utc::now() + chrono::Duration::seconds(60));
        assert!(conn.is_expiring_soon(Some(Duration::from_secs(120))));
    }

    #[tokio::test]
    async fn test_memory_store_basic_operations() {
        let store = MemoryConnectionStore::new();
        let conn = Connection::new("test_tenant", "github", "user123", "test_token").unwrap();
        let conn_id = conn.connection_id();

        // Test storing and retrieving
        store.put(&conn_id, &conn).await.unwrap();
        let retrieved = store.get(&conn_id).await.unwrap();
        assert_eq!(retrieved, Some(conn.clone()));

        // Test deletion
        assert!(store.delete(&conn_id).await.unwrap());
        assert_eq!(store.get(&conn_id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_memory_store_cas() {
        let store = MemoryConnectionStore::new();
        let conn1 = Connection::new("test_tenant", "github", "user1", "token1").unwrap();
        let conn2 = Connection::new("test_tenant", "github", "user2", "token2").unwrap();
        let conn_id = conn1.connection_id();

        // CAS insert new connection
        assert!(
            store
                .compare_and_swap(&conn_id, None, Some(&conn1))
                .await
                .unwrap()
        );

        // CAS update existing connection
        assert!(
            store
                .compare_and_swap(&conn_id, Some(&conn1), Some(&conn2))
                .await
                .unwrap()
        );

        // CAS fail (expected value does not match)
        assert!(
            !store
                .compare_and_swap(&conn_id, Some(&conn1), Some(&conn1))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn test_memory_store_ttl() {
        let store = MemoryConnectionStore::new().with_default_ttl(Duration::from_millis(100));

        let conn = Connection::new("test_tenant", "github", "user123", "test_token").unwrap();
        let conn_id = conn.connection_id();
        store.put(&conn_id, &conn).await.unwrap();

        // Immediate retrieval should succeed
        assert!(store.get(&conn_id).await.unwrap().is_some());

        // Wait for TTL to expire
        sleep(TokioDuration::from_millis(150)).await;

        // After TTL expires, should return None
        assert!(store.get(&conn_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_cleanup_expired() {
        let store = MemoryConnectionStore::new();

        // Add a normal connection
        let conn1 = Connection::new("test_tenant", "github", "user1", "token1").unwrap();
        let conn1_id = conn1.connection_id();
        store.put(&conn1_id, &conn1).await.unwrap();

        // Add an expired connection
        let mut conn2 = Connection::new("test_tenant", "github", "user2", "token2").unwrap();
        conn2.expires_at = Some(Utc::now() - chrono::Duration::seconds(10));
        let conn2_id = conn2.connection_id();
        store.put(&conn2_id, &conn2).await.unwrap();

        assert_eq!(store.count().await.unwrap(), 2);

        // Clean up expired connections
        let cleaned = store.cleanup_expired().await.unwrap();
        assert_eq!(cleaned, 1);
        assert_eq!(store.count().await.unwrap(), 1);

        // Ensure the correct connection is retained (the existing one is conn1)
        assert!(store.get(&conn1_id).await.unwrap().is_some());
        assert!(store.get(&conn2_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_auth_connection_trn() {
        let trn = AuthConnectionTrn::new("test_tenant", "github", "user123").unwrap();
        let trn_string = trn.to_trn_string().unwrap();

        // Validate TRN format
        assert!(trn_string.starts_with("trn:authflow:"));
        assert!(trn_string.contains("test_tenant"));
        assert!(trn_string.contains("github"));
        assert!(trn_string.contains("user123"));

        // Test parsing
        let parsed = AuthConnectionTrn::parse(&trn_string).unwrap();
        assert_eq!(parsed.tenant, "test_tenant");
        assert_eq!(parsed.provider, "github");
        assert_eq!(parsed.user_id, "user123");
    }
}
