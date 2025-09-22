use anyhow::{anyhow, Result};
use std::sync::Arc;

use crate::executor::{ExecutionResult, Executor};
use crate::models::{ConnectionConfig, TaskConfig};
use crate::store::service::StorageService;

use crate::interface::dto::{ExecuteOverridesDto};

pub struct OpenActService {
    storage: Arc<StorageService>,
}

impl OpenActService {
    pub async fn from_env() -> Result<Self> {
        Ok(Self { storage: StorageService::global().await })
    }

    pub fn from_storage(storage: Arc<StorageService>) -> Self { Self { storage } }

    // Connections
    pub async fn upsert_connection(&self, c: &ConnectionConfig) -> Result<()> { self.storage.upsert_connection(c).await }
    pub async fn get_connection(&self, trn: &str) -> Result<Option<ConnectionConfig>> { self.storage.get_connection(trn).await }
    pub async fn list_connections(&self, auth_type: Option<&str>, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<ConnectionConfig>> { self.storage.list_connections(auth_type, limit, offset).await }
    pub async fn delete_connection(&self, trn: &str) -> Result<bool> { self.storage.delete_connection(trn).await }

    // Tasks
    pub async fn upsert_task(&self, t: &TaskConfig) -> Result<()> { self.storage.upsert_task(t).await }
    pub async fn get_task(&self, trn: &str) -> Result<Option<TaskConfig>> { self.storage.get_task(trn).await }
    pub async fn list_tasks(&self, connection_trn: Option<&str>, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<TaskConfig>> { self.storage.list_tasks(connection_trn, limit, offset).await }
    pub async fn delete_task(&self, trn: &str) -> Result<bool> { self.storage.delete_task(trn).await }

    // Execute
    pub async fn execute_task(&self, task_trn: &str, overrides: Option<ExecuteOverridesDto>) -> Result<ExecutionResult> {
        let (conn, mut task) = self
            .storage
            .get_execution_context(task_trn)
            .await?
            .ok_or_else(|| anyhow!("Task not found: {}", task_trn))?;

        if let Some(ov) = overrides {
            if let Some(m) = ov.method { task.method = m; }
            if let Some(ep) = ov.endpoint { task.api_endpoint = ep; }
            if let Some(h) = ov.headers { let mut headers = task.headers.unwrap_or_default(); for (k, vs) in h { headers.insert(k, vs); } task.headers = Some(headers); }
            if let Some(q) = ov.query { let mut qs = task.query_params.unwrap_or_default(); for (k, vs) in q { qs.insert(k, vs); } task.query_params = Some(qs); }
            if let Some(b) = ov.body { task.request_body = Some(b); }
        }

        let executor = Executor::new();
        executor.execute(&conn, &task).await
    }

    // System
    pub async fn stats(&self) -> Result<crate::store::service::StorageStats> { self.storage.get_stats().await }
    pub async fn cleanup(&self) -> Result<crate::store::service::CleanupResult> { self.storage.cleanup().await }
    pub async fn cache_stats(&self) -> Result<crate::store::service::CacheStats> { Ok(self.storage.get_cache_stats().await) }

    // Config
    pub async fn import(&self, connections: Vec<ConnectionConfig>, tasks: Vec<TaskConfig>) -> Result<(usize, usize)> { self.storage.import_configurations(connections, tasks).await }
    pub async fn export(&self) -> Result<(Vec<ConnectionConfig>, Vec<TaskConfig>)> { self.storage.export_configurations().await }
}


