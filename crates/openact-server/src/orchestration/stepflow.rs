use std::time::Duration;

use aionix_protocol::{CommandEnvelope, Trn as ProtocolTrn};
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
        let raw_run_id = envelope
            .parameters
            .get("runId")
            .and_then(|value| value.as_str())
            .or_else(|| envelope.extensions.get("runTrn").and_then(|value| value.as_str()))
            .map(|value| value.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let canonical_run_trn = Self::normalise_execution_trn(&raw_run_id, tenant);
        let run_id = canonical_run_trn
            .as_ref()
            .map(|trn| trn.to_string())
            .unwrap_or_else(|| raw_run_id.clone());
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
            .and_then(|raw| Self::normalise_execution_trn(raw, tenant))
            .or_else(|| canonical_run_trn.clone())
            .map(|trn| trn.to_string());
        let now = Utc::now();
        let deadline_at: Option<DateTime<Utc>> =
            chrono::Duration::from_std(effective_timeout).ok().map(|delta| now + delta);

        let heartbeat_timeout = effective_timeout.as_secs();

        let mut metadata = Map::new();
        metadata.insert("schemaVersion".into(), Value::String(envelope.schema_version.clone()));
        metadata.insert("command".into(), Value::String(envelope.command.clone()));
        metadata.insert("source".into(), Value::String(envelope.source.to_string()));
        metadata.insert("target".into(), Value::String(envelope.target.to_string()));
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

        // Extract workflow-level runId from metadata (if available)
        let workflow_run_id = run
            .metadata
            .as_ref()
            .and_then(|m| m.get("runTrn"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| run.run_id.clone());

        if !data.contains_key("runId") {
            data.insert("runId".to_string(), Value::String(workflow_run_id.clone()));
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
            "runId": workflow_run_id,
            "correlationId": correlation,
            "extensions": {
                "taskRunId": run.run_id
            }
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
        let raw = Self::metadata_field(run, "runTrn").unwrap_or_else(|| run.run_id.clone());
        if let Some(exec_trn) = Self::normalise_execution_trn(&raw, &run.tenant) {
            let tenant = exec_trn.tenant().to_string();
            let version = exec_trn.version().unwrap_or("v1");
            let mut path = exec_trn
                .resource_path()
                .map(|p| p.to_string())
                .unwrap_or_else(|| run.run_id.clone());
            if let Some(state) = state_name {
                if !path.is_empty() {
                    path.push('/');
                }
                path.push_str(state);
            }
            return format!("trn:stepflow:{}:task/{}@{}", tenant, path, version);
        }

        if let Some(state) = state_name {
            format!("trn:stepflow:{}:task/{}/{}", run.tenant, raw, state)
        } else {
            format!("trn:stepflow:{}:task/{}", run.tenant, raw)
        }
    }

    fn normalise_execution_trn(raw: &str, tenant_hint: &str) -> Option<ProtocolTrn> {
        if let Ok(trn) = ProtocolTrn::parse(raw) {
            return Some(trn);
        }
        if let Some(converted) = Self::convert_legacy_execution_trn(raw) {
            if let Ok(trn) = ProtocolTrn::parse(&converted) {
                return Some(trn);
            }
        }
        // Fallback: if caller only supplied short run_id, synthesise with tenant hint
        if !raw.contains(':') {
            let synthetic = format!("trn:stepflow:{}:execution/default/{}@v1", tenant_hint, raw);
            if let Ok(trn) = ProtocolTrn::parse(&synthetic) {
                return Some(trn);
            }
        }
        None
    }

    fn convert_legacy_execution_trn(raw: &str) -> Option<String> {
        let trimmed = raw.strip_prefix("trn:stepflow:")?;
        let mut parts = trimmed.splitn(4, ':');
        let tenant = parts.next()?;
        let resource_type = parts.next()?;
        if resource_type != "execution" {
            return None;
        }
        let workflow = parts.next()?;
        let remainder = parts.next().unwrap_or("");
        if workflow.is_empty() || remainder.is_empty() {
            return None;
        }
        let (run_part, version) = remainder
            .split_once('@')
            .map(|(run, ver)| (run, ver.trim_start_matches('@')))
            .unwrap_or((remainder, "v1"));
        Some(format!(
            "trn:stepflow:{}:execution/{}/{}@{}",
            tenant,
            workflow,
            run_part,
            if version.is_empty() { "v1" } else { version }
        ))
    }
}
