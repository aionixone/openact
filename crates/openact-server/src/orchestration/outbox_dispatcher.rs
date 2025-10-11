use std::{sync::Arc, time::Duration};

use anyhow::Context;
use chrono::Utc;
use openact_core::orchestration::{
    OrchestratorOutboxInsert, OrchestratorOutboxRecord, OrchestratorRunStatus,
};
use serde_json::Value;
use tokio::time::{sleep, Instant};

use super::{OutboxService, RunService, StepflowCommandAdapter};

const DEFAULT_BATCH_SIZE: usize = 50;
const DEFAULT_INTERVAL_MS: u64 = 1_000;

pub struct OutboxDispatcher {
    pub outbox: OutboxService,
    pub runs: RunService,
    pub endpoint: String,
    pub batch_size: usize,
    pub interval: Duration,
    pub http_client: reqwest::Client,
}

impl OutboxDispatcher {
    pub fn new(outbox: OutboxService, runs: RunService, endpoint: String) -> Self {
        Self {
            outbox,
            runs,
            endpoint,
            batch_size: DEFAULT_BATCH_SIZE,
            interval: Duration::from_millis(DEFAULT_INTERVAL_MS),
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn run_loop(self: Arc<Self>) {
        loop {
            let start = Instant::now();
            if let Err(err) = self.process_batch().await {
                tracing::error!(error = %err, "outbox dispatcher batch failed");
            }
            let elapsed = start.elapsed();
            if elapsed < self.interval {
                sleep(self.interval - elapsed).await;
            }
        }
    }

    async fn process_batch(&self) -> anyhow::Result<()> {
        let now = Utc::now();
        let records = self.outbox.fetch_due(now, self.batch_size).await?;
        for record in records {
            if let Err(err) = self.process_record(record).await {
                tracing::error!(error = %err, "outbox record failed");
            }
        }
        Ok(())
    }

    async fn process_record(&self, record: OrchestratorOutboxRecord) -> anyhow::Result<()> {
        let payload = record.payload.clone();
        let response = self
            .http_client
            .post(&self.endpoint)
            .json(&payload)
            .send()
            .await
            .context("send event to orchestrator")?;

        if response.status().is_success() {
            self.outbox.mark_delivered(record.id, Utc::now()).await?;
        } else {
            let err_body = response.text().await.unwrap_or_else(|_| "<no body>".into());
            let next_attempt = Utc::now() + chrono::Duration::seconds(30);
            self.outbox
                .mark_retry(record.id, next_attempt, record.attempts + 1, Some(err_body))
                .await?;
        }

        Ok(())
    }
}

pub struct HeartbeatSupervisor {
    pub runs: RunService,
    pub outbox: OutboxService,
    pub batch_size: usize,
    pub interval: Duration,
}

impl HeartbeatSupervisor {
    pub fn new(runs: RunService, outbox: OutboxService) -> Self {
        Self { runs, outbox, batch_size: DEFAULT_BATCH_SIZE, interval: Duration::from_millis(DEFAULT_INTERVAL_MS) }
    }

    pub async fn run_loop(self: Arc<Self>) {
        loop {
            let start = Instant::now();
            if let Err(err) = self.process_timeouts().await {
                tracing::error!(error = %err, "heartbeat supervisor batch failed");
            }
            let elapsed = start.elapsed();
            if elapsed < self.interval {
                sleep(self.interval - elapsed).await;
            }
        }
    }

    async fn process_timeouts(&self) -> anyhow::Result<()> {
        let cutoff = Utc::now() - chrono::Duration::seconds(5);
        let candidates = self.runs.list_for_timeout(cutoff, self.batch_size).await?;
        for run in candidates {
            self.runs
                .update_status(
                    &run.run_id,
                    OrchestratorRunStatus::TimedOut,
                    Some("timed_out".to_string()),
                    None,
                    Some(Value::String("heartbeat expired".into())),
                )
                .await?;

            let event = StepflowCommandAdapter::build_timeout_event(&run);
            self.outbox
                .enqueue(OrchestratorOutboxInsert {
                    run_id: Some(run.run_id.clone()),
                    protocol: "aionix.event.stepflow".into(),
                    payload: event,
                    next_attempt_at: Utc::now(),
                    attempts: 0,
                    last_error: None,
                })
                .await?;
        }
        Ok(())
    }
}
