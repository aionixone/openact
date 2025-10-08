use crate::error::CoreResult;
use crate::types::{
    ActionRecord, AuthConnection, Checkpoint, ConnectionRecord, ConnectorKind, Trn,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Filter for listing actions
#[derive(Debug, Clone, Default)]
pub struct ActionListFilter {
    pub tenant: Option<String>,
    pub connector: Option<ConnectorKind>,
    pub mcp_enabled: Option<bool>,
    pub name_prefix: Option<String>,
    pub created_after: Option<DateTime<Utc>>,
    pub created_before: Option<DateTime<Utc>>,
    /// Text query applied to name or TRN (implementation-defined)
    pub q: Option<String>,
    /// Exact connection TRN filter
    pub connection_trn: Option<Trn>,
    /// Governance allow patterns (e.g., ["http.*", "postgres.query"]) — optional
    pub allow_patterns: Option<Vec<String>>,
    /// Governance deny patterns (e.g., ["*.delete"]) — optional
    pub deny_patterns: Option<Vec<String>>,
}

/// Sorting options for listing actions
#[derive(Debug, Clone, Copy)]
pub enum ActionSortField {
    CreatedAt,
    Name,
    Version,
}

#[derive(Debug, Clone, Copy)]
pub struct ActionListOptions {
    pub sort_field: Option<ActionSortField>,
    pub ascending: bool,
    pub page: Option<u64>,
    pub page_size: Option<u64>,
}

impl Default for ActionListOptions {
    fn default() -> Self {
        Self {
            sort_field: Some(ActionSortField::CreatedAt),
            ascending: true,
            page: None,
            page_size: None,
        }
    }
}

/// Paged list result with total count (pre-pagination)
#[derive(Debug, Clone)]
pub struct ActionListResult {
    pub records: Vec<ActionRecord>,
    pub total: u64,
}

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
    async fn list_by_connector(&self, connector: &ConnectorKind) -> CoreResult<Vec<ActionRecord>>;

    /// List actions with optional filters, sort and pagination.
    async fn list_filtered(
        &self,
        filter: ActionListFilter,
        opts: Option<ActionListOptions>,
    ) -> CoreResult<Vec<ActionRecord>>;

    /// Same as list_filtered but returns total count as well.
    async fn list_filtered_paged(
        &self,
        filter: ActionListFilter,
        opts: ActionListOptions,
    ) -> CoreResult<ActionListResult>;
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
