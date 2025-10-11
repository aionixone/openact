use std::time::Duration;

use aionix_protocol::CommandEnvelope;
use chrono::{DateTime, Utc};
use openact_core::orchestration::{OrchestratorRunRecord, OrchestratorRunStatus};
use openact_core::Trn;
use serde_json::{json, Value};
use uuid::Uuid;

/// Utilities for translating Stepflow commands into the orchestrator runtime model.
pub struct StepflowCommandAdapter;

impl StepflowCommandAdapter {
    pub fn prepare_run(
        envelope: &CommandEnvelope,
        tenant: &str,
        target_trn: &Trn,
        effective_timeout: Duration,
    ) -> (OrchestratorRunRecord, Option<u64>) {
        let run_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let deadline_at: Option<DateTime<Utc>> =
            chrono::Duration::from_std(effective_timeout).ok().map(|delta| now + delta);

        let heartbeat_timeout = effective_timeout.as_secs();

        let metadata = json!({
            "schemaVersion": envelope.schema_version,
            "command": envelope.command,
            "source": envelope.source,
            "target": envelope.target,
        });

        let record = OrchestratorRunRecord {
            command_id: envelope.id.clone(),
            run_id,
            tenant: tenant.to_string(),
            action_trn: target_trn.clone(),
            status: OrchestratorRunStatus::Running,
            phase: Some("execution".to_string()),
            trace_id: envelope.trace_id.clone(),
            correlation_id: envelope.correlation_id.clone(),
            heartbeat_at: now,
            deadline_at,
            status_ttl_seconds: None,
            next_poll_at: None,
            poll_attempts: 0,
            external_ref: None,
            result: None,
            error: None,
            metadata: Some(metadata),
            created_at: now,
            updated_at: now,
        };

        (record, Some(heartbeat_timeout))
    }

    pub fn build_success_event(run: &OrchestratorRunRecord, output: &Value) -> Value {
        let data = json!({
            "status": "succeeded",
            "result": output,
            "commandId": run.command_id,
        });
        Self::build_event(run, "succeeded", data)
    }

    pub fn build_failure_event(run: &OrchestratorRunRecord, error: &Value) -> Value {
        let data = json!({
            "status": "failed",
            "error": error,
            "commandId": run.command_id,
        });
        Self::build_event(run, "failed", data)
    }

    pub fn build_timeout_event(run: &OrchestratorRunRecord) -> Value {
        let data = json!({
            "status": "timed_out",
            "reason": "heartbeat expired",
            "commandId": run.command_id,
        });
        Self::build_event(run, "timed_out", data)
    }

    fn build_event(run: &OrchestratorRunRecord, outcome: &str, data: Value) -> Value {
        let id = Uuid::new_v4().to_string();
        let timestamp = Utc::now().to_rfc3339();
        let tenant = run.tenant.clone();
        let correlation = run.correlation_id.clone().unwrap_or_else(|| run.command_id.clone());

        json!({
            "specversion": "1.0",
            "id": id,
            "source": format!("trn:openact:{}:executor", tenant),
            "type": format!("aionix.openact.action.{}", outcome),
            "time": timestamp,
            "datacontenttype": "application/json",
            "data": data,
            "aionixSchemaVersion": "1.1.0",
            "tenant": tenant,
            "traceId": run.trace_id,
            "resourceTrn": run.action_trn.as_str(),
            "runId": run.run_id,
            "correlationId": correlation,
        })
    }
}
