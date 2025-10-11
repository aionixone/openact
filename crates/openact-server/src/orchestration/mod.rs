use std::sync::Arc;

use chrono::{DateTime, Utc};
use openact_core::orchestration::{
    OrchestratorOutboxInsert, OrchestratorOutboxStore, OrchestratorRunRecord,
    OrchestratorRunStatus, OrchestratorRunStore,
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
}
