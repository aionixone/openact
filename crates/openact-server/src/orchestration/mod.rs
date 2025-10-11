use std::sync::Arc;

use chrono::{DateTime, Utc};
use openact_core::orchestration::{
    OrchestratorOutboxInsert, OrchestratorOutboxRecord, OrchestratorOutboxStore,
    OrchestratorRunRecord, OrchestratorRunStatus, OrchestratorRunStore,
};
use serde_json::Value;

/// High-level service for orchestrated command runs.
#[derive(Clone)]
pub struct RunService {
    runs: Arc<dyn OrchestratorRunStore>,
}

impl RunService {
    pub fn new(runs: Arc<dyn OrchestratorRunStore>) -> Self {
        Self { runs }
    }

    pub async fn create_run(&self, record: OrchestratorRunRecord) -> anyhow::Result<()> {
        self.runs.insert_run(&record).await.map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn update_status(
        &self,
        run_id: &str,
        status: OrchestratorRunStatus,
        phase: Option<String>,
        result: Option<Value>,
        error: Option<Value>,
    ) -> anyhow::Result<()> {
        self.runs
            .update_status(run_id, status, phase, result, error)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn refresh_heartbeat(
        &self,
        run_id: &str,
        heartbeat_at: DateTime<Utc>,
        deadline_at: Option<DateTime<Utc>>,
    ) -> anyhow::Result<()> {
        self.runs
            .refresh_heartbeat(run_id, heartbeat_at, deadline_at)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn get(&self, run_id: &str) -> anyhow::Result<Option<OrchestratorRunRecord>> {
        self.runs.get_run(run_id).await.map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn list_for_timeout(
        &self,
        heartbeat_cutoff: DateTime<Utc>,
        limit: usize,
    ) -> anyhow::Result<Vec<OrchestratorRunRecord>> {
        self.runs
            .list_for_timeout(heartbeat_cutoff, limit)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }
}

/// Service managing outbox envelopes destined for orchestrators.
#[derive(Clone)]
pub struct OutboxService {
    outbox: Arc<dyn OrchestratorOutboxStore>,
}

impl OutboxService {
    pub fn new(outbox: Arc<dyn OrchestratorOutboxStore>) -> Self {
        Self { outbox }
    }

    pub async fn enqueue(&self, insert: OrchestratorOutboxInsert) -> anyhow::Result<i64> {
        self.outbox.enqueue(insert).await.map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn fetch_due(
        &self,
        as_of: DateTime<Utc>,
        limit: usize,
    ) -> anyhow::Result<Vec<OrchestratorOutboxRecord>> {
        self.outbox.fetch_ready(as_of, limit).await.map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn mark_delivered(&self, id: i64, delivered_at: DateTime<Utc>) -> anyhow::Result<()> {
        self.outbox.mark_delivered(id, delivered_at).await.map_err(|e| anyhow::anyhow!(e))
    }

    pub async fn mark_retry(
        &self,
        id: i64,
        next_attempt_at: DateTime<Utc>,
        attempts: i32,
        last_error: Option<String>,
    ) -> anyhow::Result<()> {
        self.outbox
            .mark_retry(id, next_attempt_at, attempts, last_error)
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }
}

mod stepflow;
pub use stepflow::StepflowCommandAdapter;
mod outbox_dispatcher;
pub use outbox_dispatcher::{HeartbeatSupervisor, OutboxDispatcher};
