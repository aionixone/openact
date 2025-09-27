use crate::error::CoreResult;
use crate::types::{ActionRecord, ConnectionRecord, Trn, Checkpoint};
use async_trait::async_trait;

#[async_trait]
pub trait ConnectionStore: Send + Sync {
    async fn upsert(&self, record: &ConnectionRecord) -> CoreResult<()>;
    async fn get(&self, trn: &Trn) -> CoreResult<Option<ConnectionRecord>>;
    async fn delete(&self, trn: &Trn) -> CoreResult<bool>;
    async fn list_by_connector(&self, connector: &str) -> CoreResult<Vec<ConnectionRecord>>;
}

#[async_trait]
pub trait ActionRepository: Send + Sync {
    async fn upsert(&self, record: &ActionRecord) -> CoreResult<()>;
    async fn get(&self, trn: &Trn) -> CoreResult<Option<ActionRecord>>;
    async fn delete(&self, trn: &Trn) -> CoreResult<bool>;
    async fn list_by_connection(&self, connection_trn: &Trn) -> CoreResult<Vec<ActionRecord>>;
}

#[async_trait]
pub trait RunStore: Send + Sync {
    async fn put(&self, cp: Checkpoint) -> CoreResult<()>;
    async fn get(&self, run_id: &str) -> CoreResult<Option<Checkpoint>>;
    async fn delete(&self, run_id: &str) -> CoreResult<bool>;
}


