use crate::error::CoreResult;
use crate::types::Trn;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;

/// Generic runtime status for orchestrated command executions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestratorRunStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

impl OrchestratorRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrchestratorRunStatus::Running => "running",
            OrchestratorRunStatus::Succeeded => "succeeded",
            OrchestratorRunStatus::Failed => "failed",
            OrchestratorRunStatus::Cancelled => "cancelled",
            OrchestratorRunStatus::TimedOut => "timed_out",
        }
    }
}

impl FromStr for OrchestratorRunStatus {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "running" => Ok(OrchestratorRunStatus::Running),
            "succeeded" => Ok(OrchestratorRunStatus::Succeeded),
            "failed" => Ok(OrchestratorRunStatus::Failed),
            "cancelled" => Ok(OrchestratorRunStatus::Cancelled),
            "timed_out" => Ok(OrchestratorRunStatus::TimedOut),
            _ => Err("unknown orchestrator run status"),
        }
    }
}

/// Persisted record describing the lifecycle of an orchestrated command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorRunRecord {
    pub command_id: String,
    pub run_id: String,
    pub tenant: String,
    pub action_trn: Trn,
    pub status: OrchestratorRunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    pub trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    pub heartbeat_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_ttl_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_poll_at: Option<DateTime<Utc>>,
    pub poll_attempts: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Outbox envelope awaiting delivery to an upstream orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorOutboxRecord {
    pub id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub protocol: String,
    pub payload: Value,
    pub attempts: i32,
    pub next_attempt_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<DateTime<Utc>>,
}

/// Parameters required to enqueue a new outbox message.
#[derive(Debug, Clone)]
pub struct OrchestratorOutboxInsert {
    pub run_id: Option<String>,
    pub protocol: String,
    pub payload: Value,
    pub next_attempt_at: DateTime<Utc>,
    pub attempts: i32,
    pub last_error: Option<String>,
}

#[async_trait]
pub trait OrchestratorRunStore: Send + Sync {
    async fn insert_run(&self, run: &OrchestratorRunRecord) -> CoreResult<()>;
    async fn get_run(&self, run_id: &str) -> CoreResult<Option<OrchestratorRunRecord>>;
    async fn update_status(
        &self,
        run_id: &str,
        status: OrchestratorRunStatus,
        phase: Option<String>,
        result: Option<Value>,
        error: Option<Value>,
    ) -> CoreResult<()>;
    async fn refresh_heartbeat(
        &self,
        run_id: &str,
        heartbeat_at: DateTime<Utc>,
        deadline_at: Option<DateTime<Utc>>,
    ) -> CoreResult<()>;
    async fn update_poll_schedule(
        &self,
        run_id: &str,
        next_poll_at: Option<DateTime<Utc>>,
        poll_attempts: i32,
    ) -> CoreResult<()>;
    async fn list_for_timeout(
        &self,
        heartbeat_cutoff: DateTime<Utc>,
        limit: usize,
    ) -> CoreResult<Vec<OrchestratorRunRecord>>;
    async fn list_due_for_poll(
        &self,
        as_of: DateTime<Utc>,
        limit: usize,
    ) -> CoreResult<Vec<OrchestratorRunRecord>>;
}

#[async_trait]
pub trait OrchestratorOutboxStore: Send + Sync {
    async fn enqueue(&self, insert: OrchestratorOutboxInsert) -> CoreResult<i64>;
    async fn fetch_ready(
        &self,
        as_of: DateTime<Utc>,
        limit: usize,
    ) -> CoreResult<Vec<OrchestratorOutboxRecord>>;
    async fn mark_delivered(&self, id: i64, delivered_at: DateTime<Utc>) -> CoreResult<()>;
    async fn mark_retry(
        &self,
        id: i64,
        next_attempt_at: DateTime<Utc>,
        attempts: i32,
        last_error: Option<String>,
    ) -> CoreResult<()>;
}
