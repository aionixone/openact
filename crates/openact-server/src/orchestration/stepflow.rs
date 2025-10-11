use std::time::Duration;

use aionix_protocol::CommandEnvelope;
use chrono::{DateTime, Utc};
use openact_core::orchestration::{OrchestratorRunRecord, OrchestratorRunStatus};
use openact_core::Trn;
use serde_json::{json, Map, Value};
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
        let run_id = envelope
            .parameters
            .get("runId")
            .and_then(|value| value.as_str())
            .or_else(|| envelope.extensions.get("runTrn").and_then(|value| value.as_str()))
            .map(|value| value.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let state_name = envelope
            .parameters
            .get("stateName")
            .and_then(|value| value.as_str())
            .or_else(|| envelope.extensions.get("stateName").and_then(|value| value.as_str()))
            .map(|value| value.to_string());
        let run_trn = envelope
            .extensions
            .get("runTrn")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string());
        let now = Utc::now();
        let deadline_at: Option<DateTime<Utc>> =
            chrono::Duration::from_std(effective_timeout).ok().map(|delta| now + delta);

        let heartbeat_timeout = effective_timeout.as_secs();

        let mut metadata = Map::new();
        metadata.insert("schemaVersion".into(), Value::String(envelope.schema_version.clone()));
        metadata.insert("command".into(), Value::String(envelope.command.clone()));
        metadata.insert("source".into(), Value::String(envelope.source.clone()));
        metadata.insert("target".into(), Value::String(envelope.target.clone()));
        if let Some(state) = state_name.clone() {
            metadata.insert("stateName".into(), Value::String(state));
        }
        if let Some(run_trn) = run_trn.clone() {
            metadata.insert("runTrn".into(), Value::String(run_trn));
        }
        let metadata_value = if metadata.is_empty() { None } else { Some(Value::Object(metadata)) };

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
            metadata: metadata_value,
            created_at: now,
            updated_at: now,
        };

        (record, Some(heartbeat_timeout))
    }

    pub fn build_success_event(run: &OrchestratorRunRecord, output: &Value) -> Value {
        let mut data = Map::new();
        data.insert("status".to_string(), Value::String("succeeded".into()));
        data.insert("output".to_string(), output.clone());
        data.insert("commandId".to_string(), Value::String(run.command_id.clone()));
        Self::build_event(run, "succeeded", data)
    }

    pub fn build_failure_event(run: &OrchestratorRunRecord, error: &Value) -> Value {
        let mut data = Map::new();
        data.insert("status".to_string(), Value::String("failed".into()));
        data.insert("error".to_string(), error.clone());
        data.insert("commandId".to_string(), Value::String(run.command_id.clone()));
        Self::build_event(run, "failed", data)
    }

    pub fn build_timeout_event(run: &OrchestratorRunRecord) -> Value {
        let mut data = Map::new();
        data.insert("status".to_string(), Value::String("timed_out".into()));
        data.insert("reason".to_string(), Value::String("heartbeat expired".into()));
        data.insert("commandId".to_string(), Value::String(run.command_id.clone()));
        Self::build_event(run, "timed_out", data)
    }

    pub fn build_cancelled_event(run: &OrchestratorRunRecord, details: &Value) -> Value {
        let mut data = Map::new();
        data.insert("status".to_string(), Value::String("cancelled".into()));
        match details {
            Value::Object(map) => {
                for (k, v) in map {
                    if k != "status" {
                        data.insert(k.clone(), v.clone());
                    }
                }
            }
            Value::Null => {}
            other => {
                data.insert("reason".to_string(), other.clone());
            }
        }
        data.insert("commandId".to_string(), Value::String(run.command_id.clone()));
        Self::build_event(run, "cancelled", data)
    }

    fn build_event(
        run: &OrchestratorRunRecord,
        outcome: &str,
        mut data: Map<String, Value>,
    ) -> Value {
        let id = Uuid::new_v4().to_string();
        let timestamp = Utc::now().to_rfc3339();
        let tenant = run.tenant.clone();
        let correlation = run.correlation_id.clone().unwrap_or_else(|| run.command_id.clone());

        if !data.contains_key("runId") {
            data.insert("runId".to_string(), Value::String(run.run_id.clone()));
        }

        let state_name = if let Some(existing) = data.get("stateName").and_then(|v| v.as_str()) {
            Some(existing.to_string())
        } else {
            Self::metadata_field(run, "stateName")
        };

        if let Some(state) = state_name.clone() {
            data.entry("stateName".to_string()).or_insert_with(|| Value::String(state));
        }

        let resource_trn = Self::build_resource_trn(run, state_name.as_deref());

        json!({
            "specversion": "1.0",
            "id": id,
            "source": format!("trn:openact:{}:executor", tenant),
            "type": format!("aionix.stepflow.task.{}", outcome),
            "time": timestamp,
            "datacontenttype": "application/json",
            "data": Value::Object(data),
            "aionixSchemaVersion": "0.1.0",
            "tenant": tenant,
            "traceId": run.trace_id,
            "resourceTrn": resource_trn,
            "runId": run.run_id,
            "correlationId": correlation,
        })
    }

    fn metadata_field(run: &OrchestratorRunRecord, key: &str) -> Option<String> {
        run.metadata
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
    }

    fn build_resource_trn(run: &OrchestratorRunRecord, state_name: Option<&str>) -> String {
        let run_segment = Self::metadata_field(run, "runTrn").unwrap_or_else(|| run.run_id.clone());
        if let Some(state) = state_name {
            format!("trn:stepflow:{}:task/{}/{}", run.tenant, run_segment, state)
        } else {
            format!("trn:stepflow:{}:task/{}", run.tenant, run_segment)
        }
    }
}
