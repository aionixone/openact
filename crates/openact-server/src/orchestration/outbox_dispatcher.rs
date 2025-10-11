use std::{sync::Arc, time::Duration};

use anyhow::Context;
use chrono::{Duration as ChronoDuration, Utc};
use openact_core::orchestration::{
    OrchestratorOutboxInsert, OrchestratorOutboxRecord, OrchestratorRunRecord,
    OrchestratorRunStatus,
};
use serde_json::Value;
use tokio::time::{sleep, Instant};

use super::{OutboxService, RunService, StepflowCommandAdapter};

#[derive(Debug, Clone)]
pub struct OutboxDispatcherConfig {
    pub batch_size: usize,
    pub interval: Duration,
    pub retry_backoff: ChronoDuration,
}

impl Default for OutboxDispatcherConfig {
    fn default() -> Self {
        Self {
            batch_size: 50,
            interval: Duration::from_millis(1_000),
            retry_backoff: ChronoDuration::seconds(30),
        }
    }
}

pub struct OutboxDispatcher {
    pub outbox: OutboxService,
    pub runs: RunService,
    pub endpoint: String,
    pub config: OutboxDispatcherConfig,
    pub http_client: Option<reqwest::Client>,
}

impl OutboxDispatcher {
    pub fn new(
        outbox: OutboxService,
        runs: RunService,
        endpoint: String,
        config: OutboxDispatcherConfig,
    ) -> Self {
        Self::with_client(outbox, runs, endpoint, config, Some(reqwest::Client::new()))
    }

    pub fn with_client(
        outbox: OutboxService,
        runs: RunService,
        endpoint: String,
        config: OutboxDispatcherConfig,
        http_client: Option<reqwest::Client>,
    ) -> Self {
        Self { outbox, runs, endpoint, config, http_client }
    }

    pub async fn run_loop(self: Arc<Self>) {
        loop {
            let start = Instant::now();
            if let Err(err) = self.process_batch().await {
                tracing::error!(error = %err, "outbox dispatcher batch failed");
            }
            let elapsed = start.elapsed();
            if elapsed < self.config.interval {
                sleep(self.config.interval - elapsed).await;
            }
        }
    }

    async fn process_batch(&self) -> anyhow::Result<()> {
        let now = Utc::now();
        let records = self.outbox.fetch_due(now, self.config.batch_size).await?;
        for record in records {
            if let Err(err) = self.process_record(record).await {
                tracing::error!(error = %err, "outbox record failed");
            }
        }
        Ok(())
    }

    async fn process_record(&self, record: OrchestratorOutboxRecord) -> anyhow::Result<()> {
        let client = match &self.http_client {
            Some(client) => client,
            None => {
                // No HTTP client configured (e.g., unit tests); mark as delivered for observability.
                self.outbox.mark_delivered(record.id, Utc::now()).await?;
                return Ok(());
            }
        };
        let payload = record.payload.clone();
        let response = client
            .post(&self.endpoint)
            .json(&payload)
            .send()
            .await
            .context("send event to orchestrator")?;

        if response.status().is_success() {
            self.outbox.mark_delivered(record.id, Utc::now()).await?;
        } else {
            let err_body = response.text().await.unwrap_or_else(|_| "<no body>".into());
            let next_attempt = Utc::now() + self.config.retry_backoff;
            self.outbox
                .mark_retry(record.id, next_attempt, record.attempts + 1, Some(err_body))
                .await?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct HeartbeatSupervisorConfig {
    pub batch_size: usize,
    pub interval: Duration,
    pub timeout_grace: ChronoDuration,
}

impl Default for HeartbeatSupervisorConfig {
    fn default() -> Self {
        Self {
            batch_size: 50,
            interval: Duration::from_millis(1_000),
            timeout_grace: ChronoDuration::seconds(5),
        }
    }
}

pub struct HeartbeatSupervisor {
    pub runs: RunService,
    pub outbox: OutboxService,
    pub config: HeartbeatSupervisorConfig,
}

impl HeartbeatSupervisor {
    pub fn new(runs: RunService, outbox: OutboxService, config: HeartbeatSupervisorConfig) -> Self {
        Self { runs, outbox, config }
    }

    pub async fn run_loop(self: Arc<Self>) {
        loop {
            let start = Instant::now();
            if let Err(err) = self.process_timeouts().await {
                tracing::error!(error = %err, "heartbeat supervisor batch failed");
            }
            let elapsed = start.elapsed();
            if elapsed < self.config.interval {
                sleep(self.config.interval - elapsed).await;
            }
        }
    }

    async fn process_timeouts(&self) -> anyhow::Result<()> {
        let cutoff = Utc::now() - self.config.timeout_grace;
        let candidates = self.runs.list_for_timeout(cutoff, self.config.batch_size).await?;
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

            self.enqueue_timeout_event(&run).await?;
        }
        Ok(())
    }

    async fn enqueue_timeout_event(&self, run: &OrchestratorRunRecord) -> anyhow::Result<()> {
        let event = StepflowCommandAdapter::build_timeout_event(run);
        self.outbox
            .enqueue(OrchestratorOutboxInsert {
                run_id: Some(run.run_id.clone()),
                protocol: "aionix.event.stepflow".into(),
                payload: event,
                next_attempt_at: Utc::now(),
                attempts: 0,
                last_error: None,
            })
            .await
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!(e))
    }
}
