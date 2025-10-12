use std::time::Duration;

use aionix_contracts::{
    cloudevents::{
        set_extension, BUS_TRN_EXTENSION, OUTBOX_MESSAGE_ID_EXTENSION, TASK_RUN_ID_EXTENSION,
    },
    commands::CommandExtensions,
    idempotency::openact_outbox_key,
    status::{display_label, ServiceStatus},
    CommandEnvelope, EventEnvelope, Trn as ContractTrn,
};
use chrono::{DateTime, Utc};
use openact_core::orchestration::{OrchestratorRunRecord, OrchestratorRunStatus};
use openact_core::Trn;
use serde_json::{Map, Value};
use tracing::warn;
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
        let extensions = CommandExtensions::from_command(envelope);
        let run_trn_ext = extensions.run_trn.clone();
        let state_name_ext = extensions.state_name.map(|value| value.to_string());

        let raw_run_id = envelope
            .parameters
            .get("runId")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .or_else(|| run_trn_ext.as_ref().map(|trn| trn.to_string()))
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let canonical_run_trn =
            run_trn_ext.clone().or_else(|| Self::normalise_execution_trn(&raw_run_id, tenant));
        let run_id = canonical_run_trn
            .as_ref()
            .map(|trn| trn.to_string())
            .unwrap_or_else(|| raw_run_id.clone());
        let state_name = envelope
            .parameters
            .get("stateName")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .or_else(|| state_name_ext.clone());
        let run_trn = canonical_run_trn.clone().map(|trn| trn.to_string());
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

    pub fn build_success_event(run: &OrchestratorRunRecord, output: &Value) -> EventEnvelope {
        let mut data = Map::new();
        data.insert(
            "status".to_string(),
            Value::String(display_label(ServiceStatus::Succeeded).into_owned()),
        );
        data.insert("output".to_string(), output.clone());
        data.insert("commandId".to_string(), Value::String(run.command_id.clone()));
        Self::build_event(run, ServiceStatus::Succeeded, data)
    }

    pub fn build_failure_event(run: &OrchestratorRunRecord, error: &Value) -> EventEnvelope {
        let mut data = Map::new();
        data.insert(
            "status".to_string(),
            Value::String(display_label(ServiceStatus::Failed).into_owned()),
        );
        data.insert("error".to_string(), error.clone());
        data.insert("commandId".to_string(), Value::String(run.command_id.clone()));
        Self::build_event(run, ServiceStatus::Failed, data)
    }

    pub fn build_timeout_event(run: &OrchestratorRunRecord) -> EventEnvelope {
        let mut data = Map::new();
        data.insert(
            "status".to_string(),
            Value::String(display_label(ServiceStatus::TimedOut).into_owned()),
        );
        data.insert("reason".to_string(), Value::String("heartbeat expired".into()));
        data.insert("commandId".to_string(), Value::String(run.command_id.clone()));
        Self::build_event(run, ServiceStatus::TimedOut, data)
    }

    pub fn build_cancelled_event(run: &OrchestratorRunRecord, details: &Value) -> EventEnvelope {
        let mut data = Map::new();
        data.insert(
            "status".to_string(),
            Value::String(display_label(ServiceStatus::Cancelled).into_owned()),
        );
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
        Self::build_event(run, ServiceStatus::Cancelled, data)
    }

    fn build_event(
        run: &OrchestratorRunRecord,
        status: ServiceStatus,
        mut data: Map<String, Value>,
    ) -> EventEnvelope {
        let event_id = Uuid::new_v4().to_string();
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
        if data.get("status").is_none() {
            data.insert("status".to_string(), Value::String(display_label(status).into_owned()));
        }

        if let Some(state) = state_name.clone() {
            data.entry("stateName".to_string()).or_insert_with(|| Value::String(state));
        }

        let resource_trn = Self::build_resource_trn(run, state_name.as_deref());
        let source = ContractTrn::parse(&format!("trn:openact:{}:executor", tenant))
            .unwrap_or_else(|err| {
                warn!(
                    tenant = %tenant,
                    error = %err,
                    "failed to parse OpenAct executor TRN, using action TRN as fallback"
                );
                ContractTrn::parse(run.action_trn.as_str()).unwrap_or_else(|_| {
                    ContractTrn::parse("trn:openact:system:executor@v1").unwrap()
                })
            });

        let mut envelope = EventEnvelope {
            specversion: "1.0".to_string(),
            id: event_id.clone(),
            source,
            r#type: format!("aionix.stepflow.task.{}", event_type_suffix(status)),
            time: timestamp,
            datacontenttype: "application/json".to_string(),
            data: Value::Object(data),
            aionix_schema_version: "0.1.0".to_string(),
            tenant,
            trace_id: run.trace_id.clone(),
            resource_trn,
            subject: None,
            run_id: Some(workflow_run_id),
            correlation_id: Some(correlation),
            authz_scopes: None,
            delivery_attempt: None,
            labels: None,
            related_trns: None,
            actor_trn: None,
            extensions: Map::new(),
        };

        set_extension(&mut envelope, TASK_RUN_ID_EXTENSION, run.run_id.as_str());
        let outbox_key = openact_outbox_key(&run.run_id, &run.command_id);
        set_extension(&mut envelope, OUTBOX_MESSAGE_ID_EXTENSION, outbox_key);

        if let Some(bus_trn) = run
            .metadata
            .as_ref()
            .and_then(|meta| meta.get("busTrn"))
            .and_then(|value| value.as_str())
        {
            set_extension(&mut envelope, BUS_TRN_EXTENSION, bus_trn);
        }

        envelope
    }

    fn metadata_field(run: &OrchestratorRunRecord, key: &str) -> Option<String> {
        run.metadata
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
    }

    fn build_resource_trn(run: &OrchestratorRunRecord, state_name: Option<&str>) -> ContractTrn {
        let raw = Self::metadata_field(run, "runTrn").unwrap_or_else(|| run.run_id.clone());
        let candidate = if let Some(exec_trn) = Self::normalise_execution_trn(&raw, &run.tenant) {
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
            format!("trn:stepflow:{}:task/{}@{}", tenant, path, version)
        } else if let Some(state) = state_name {
            format!("trn:stepflow:{}:task/{}/{}", run.tenant, raw, state)
        } else {
            format!("trn:stepflow:{}:task/{}", run.tenant, raw)
        };

        ContractTrn::parse(&candidate).unwrap_or_else(|err| {
            warn!(
                run_id = %run.run_id,
                candidate = %candidate,
                error = %err,
                "failed to parse resource TRN; falling back to canonical run id"
            );
            ContractTrn::parse(&raw).unwrap_or_else(|_| {
                ContractTrn::parse(run.action_trn.as_str()).unwrap_or_else(|_| {
                    ContractTrn::parse("trn:stepflow:system:task/fallback@v1").unwrap()
                })
            })
        })
    }

    fn normalise_execution_trn(raw: &str, tenant_hint: &str) -> Option<ContractTrn> {
        if let Ok(trn) = ContractTrn::parse(raw) {
            return Some(trn);
        }
        if let Some(converted) = Self::convert_legacy_execution_trn(raw) {
            if let Ok(trn) = ContractTrn::parse(&converted) {
                return Some(trn);
            }
        }
        // Fallback: if caller only supplied short run_id, synthesise with tenant hint
        if !raw.contains(':') {
            let synthetic = format!("trn:stepflow:{}:execution/default/{}@v1", tenant_hint, raw);
            if let Ok(trn) = ContractTrn::parse(&synthetic) {
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

fn event_type_suffix(status: ServiceStatus) -> &'static str {
    match status {
        ServiceStatus::Succeeded => "succeeded",
        ServiceStatus::Failed => "failed",
        ServiceStatus::Cancelled => "cancelled",
        ServiceStatus::TimedOut => "timed_out",
        ServiceStatus::Queued => "queued",
        ServiceStatus::Running => "running",
    }
}
