use crate::error::CoreResult;
use crate::types::{ActionRecord, AuthConnection, Checkpoint, ConnectionRecord, Trn};
use async_trait::async_trait;

/// Async trait for storing and retrieving connection records
#[async_trait]
pub trait ConnectionStore: Send + Sync {
    /// Insert or update a connection record
    async fn upsert(&self, record: &ConnectionRecord) -> CoreResult<()>;
    /// Get a connection record by TRN
    async fn get(&self, trn: &Trn) -> CoreResult<Option<ConnectionRecord>>;
    /// Delete a connection record by TRN, returns true if deleted
    async fn delete(&self, trn: &Trn) -> CoreResult<bool>;
    /// List all connections for a specific connector type
    async fn list_by_connector(&self, connector: &str) -> CoreResult<Vec<ConnectionRecord>>;
    /// Get list of distinct connector types that have connections
    async fn list_distinct_connectors(&self) -> CoreResult<Vec<crate::ConnectorKind>>;
}

/// Async trait for storing and retrieving action records
#[async_trait]
pub trait ActionRepository: Send + Sync {
    /// Insert or update an action record
    async fn upsert(&self, record: &ActionRecord) -> CoreResult<()>;
    /// Get an action record by TRN
    async fn get(&self, trn: &Trn) -> CoreResult<Option<ActionRecord>>;
    /// Delete an action record by TRN, returns true if deleted
    async fn delete(&self, trn: &Trn) -> CoreResult<bool>;
    /// List all actions for a specific connection
    async fn list_by_connection(&self, connection_trn: &Trn) -> CoreResult<Vec<ActionRecord>>;
    /// List all actions for a specific connector type
    async fn list_by_connector(
        &self,
        connector: &crate::ConnectorKind,
    ) -> CoreResult<Vec<ActionRecord>>;
}

/// Async trait for storing and retrieving execution checkpoints
#[async_trait]
pub trait RunStore: Send + Sync {
    /// Store or update a checkpoint for workflow execution
    async fn put(&self, cp: Checkpoint) -> CoreResult<()>;
    /// Get a checkpoint by run ID
    async fn get(&self, run_id: &str) -> CoreResult<Option<Checkpoint>>;
    /// Delete a checkpoint by run ID, returns true if deleted
    async fn delete(&self, run_id: &str) -> CoreResult<bool>;
}

/// Async trait for storing and retrieving auth connections (OAuth tokens)
#[async_trait]
pub trait AuthConnectionStore: Send + Sync {
    /// Get an auth connection by reference
    async fn get(&self, auth_ref: &str) -> CoreResult<Option<AuthConnection>>;
    /// Store an auth connection
    async fn put(&self, auth_ref: &str, connection: &AuthConnection) -> CoreResult<()>;
    /// Delete an auth connection
    async fn delete(&self, auth_ref: &str) -> CoreResult<bool>;
    /// Compare and swap (atomic operation)
    async fn compare_and_swap(
        &self,
        auth_ref: &str,
        expected: Option<&AuthConnection>,
        new_connection: Option<&AuthConnection>,
    ) -> CoreResult<bool>;
    /// Get fresh connection (bypass cache if any)
    async fn get_fresh(&self, auth_ref: &str) -> CoreResult<Option<AuthConnection>> {
        self.get(auth_ref).await
    }
    /// List all auth connection references
    async fn list_refs(&self) -> CoreResult<Vec<String>>;
    /// Clean up expired auth connections
    async fn cleanup_expired(&self) -> CoreResult<u64>;
    /// Count auth connections
    async fn count(&self) -> CoreResult<u64>;
}
