//! Stepflow command handlers

use super::actions::map_registry_error;
use crate::{
    dto::StepflowCommandResponse,
    error::ServerError,
    middleware::{request_id::RequestId, tenant::Tenant},
    orchestration::StepflowCommandAdapter,
    AppState,
};
use aionix_contracts::parse_command_envelope;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use openact_core::orchestration::{OrchestratorOutboxInsert, OrchestratorRunStatus};
use openact_core::types::{ActionTrn, ToolName, Trn};
use openact_mcp::GovernanceConfig;
use openact_registry::ExecutionContext;
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::TryFrom;
use std::time::{Duration, Instant};
use tokio::time::timeout;

const SUPPORTED_SCHEMA_PREFIX: &str = "0.";

#[derive(Deserialize)]
pub struct CancelCommandPayload {
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default, rename = "requestedBy")]
    pub requested_by: Option<String>,
    #[serde(default, rename = "traceId")]
    pub trace_id: Option<String>,
}

/// Handle Stepflow command envelopes via REST
pub async fn execute_command(
    State((app_state, governance)): State<(AppState, GovernanceConfig)>,
    Extension(request_id): Extension<RequestId>,
    Extension(tenant): Extension<Tenant>,
    Json(raw): Json<Value>,
) -> Result<
    Json<StepflowCommandResponse>,
    (axum::http::StatusCode, Json<crate::error::ErrorResponse>),
> {
    let req_id = request_id.0.clone();
    let run_service = app_state.run_service.clone();
    let outbox_service = app_state.outbox_service.clone();
    let async_manager = app_state.async_manager.clone();

    let envelope = parse_command_envelope(&raw).map_err(|err| {
        let server_err = ServerError::InvalidInput(format!("invalid command envelope: {}", err));
        server_err.to_http_response(req_id.clone())
    })?;

    if !envelope.schema_version.starts_with(SUPPORTED_SCHEMA_PREFIX) {
        let err = ServerError::InvalidInput(format!(
            "unsupported schemaVersion: {}",
            envelope.schema_version
        ));
        return Err(err.to_http_response(req_id.clone()));
    }

    let command_tenant = envelope.tenant.clone();
    if command_tenant.is_empty() {
        let err = ServerError::InvalidInput("tenant must be provided".to_string());
        return Err(err.to_http_response(req_id.clone()));
    }

    let header_tenant = tenant.as_str();
    if header_tenant != "default" && header_tenant != command_tenant {
        tracing::warn!(
            request_id = %req_id,
            header_tenant = %header_tenant,
            command_tenant = %command_tenant,
            "tenant mismatch between header and command envelope"
        );
        let err = ServerError::InvalidInput("tenant mismatch between header and command".into());
        return Err(err.to_http_response(req_id.clone()));
    }

    let target_trn = Trn::new(envelope.target.clone());
    let parsed_trn = ActionTrn::try_from(target_trn.clone()).map_err(|_| {
        let err = ServerError::InvalidInput("invalid action TRN".to_string());
        err.to_http_response(req_id.clone())
    })?;

    if parsed_trn.parse_components().map(|c| c.tenant).as_deref() != Some(command_tenant.as_str()) {
        tracing::warn!(
            request_id = %req_id,
            command_tenant = %command_tenant,
            action_trn = %target_trn.as_str(),
            "tenant mismatch between command and target TRN"
        );
        let err = ServerError::NotFound("action not visible for tenant".into());
        return Err(err.to_http_response(req_id.clone()));
    }

    let tool_name = ToolName::normalize_action_ref(target_trn.as_str())
        .map(|t| t.to_dot_string())
        .ok_or_else(|| {
            let err = ServerError::InvalidInput("unable to derive tool name from TRN".into());
            err.to_http_response(req_id.clone())
        })?;

    if !governance.is_tool_allowed(&tool_name) {
        tracing::warn!(
            request_id = %req_id,
            tenant = %command_tenant,
            tool = %tool_name,
            "governance denied stepflow command execution"
        );
        let err = ServerError::Forbidden(format!("tool not allowed: {}", tool_name));
        return Err(err.to_http_response(req_id.clone()));
    }

    let concurrency_permit =
        governance.concurrency_limiter.clone().acquire_owned().await.map_err(|e| {
            let err = ServerError::Internal(format!("failed to acquire permit: {}", e));
            err.to_http_response(req_id.clone())
        })?;

    let input_payload = envelope
        .parameters
        .get("input")
        .cloned()
        .unwrap_or_else(|| Value::Object(envelope.parameters.clone()));

    let requested_timeout =
        envelope.timeout_seconds.and_then(|secs| u64::try_from(secs).ok().map(Duration::from_secs));
    let effective_timeout = requested_timeout
        .filter(|requested| *requested < governance.timeout)
        .unwrap_or(governance.timeout);

    let registry = app_state.registry.clone();
    let (run_record, heartbeat_timeout_secs) = StepflowCommandAdapter::prepare_run(
        &envelope,
        &command_tenant,
        &target_trn,
        effective_timeout,
    );
    let run_id = run_record.run_id.clone();
    let run_snapshot = run_record.clone();

    if let Err(err) = run_service.create_run(run_record).await {
        tracing::error!(error = %err, command_id = %envelope.id, "failed to persist orchestrator run");
        let server_err = ServerError::Internal("failed to persist orchestrator run".into());
        return Err(server_err.to_http_response(req_id.clone()));
    }

    let execution_start = Instant::now();
    let action_trn_str = target_trn.as_str().to_string();

    // Check if this is a fire-forget execution
    let is_fire_forget =
        envelope.parameters.get("mode").and_then(|v| v.as_str()) == Some("fire-forget");

    if is_fire_forget {
        // Fire-forget mode: execute in background and return immediately
        let registry_clone = registry.clone();
        let target_trn_clone = target_trn.clone();
        let input_clone = input_payload.clone();
        let run_service_clone = run_service.clone();
        let run_id_clone = run_id.clone();
        let req_id_clone = req_id.clone();
        let envelope_id = envelope.id.clone();
        let action_trn_bg = action_trn_str.clone();

        tokio::spawn(async move {
            let _permit = concurrency_permit;
            let ctx = ExecutionContext::new();
            let result = registry_clone.execute(&target_trn_clone, input_clone, Some(ctx)).await;

            // Update run status based on result (best effort, don't fail if this fails)
            let status = match result {
                Ok(_) => OrchestratorRunStatus::Succeeded,
                Err(_) => OrchestratorRunStatus::Failed,
            };

            let _ = run_service_clone
                .update_status(
                    &run_id_clone,
                    status,
                    Some("fire_forget_completed".to_string()),
                    None,
                    None,
                )
                .await;

            tracing::info!(
                request_id = %req_id_clone,
                command_id = %envelope_id,
                action = %action_trn_bg,
                run_id = %run_id_clone,
                "fire-forget task completed in background"
            );
        });

        tracing::info!(
            request_id = %req_id,
            command_id = %envelope.id,
            action = %action_trn_str,
            run_id = %run_id,
            "fire-forget task accepted and spawned"
        );

        // Return immediately with accepted status
        let response = StepflowCommandResponse {
            status: "accepted".to_string(),
            result: None,
            run_id: Some(run_id),
            phase: Some("fire_forget".to_string()),
            heartbeat_timeout: None,
            status_ttl: None,
            correlation_id: envelope.correlation_id.clone(),
            request_id: Some(req_id),
        };

        return Ok(Json(response));
    }

    let fut = async move {
        let _permit = concurrency_permit;
        let ctx = ExecutionContext::new();
        registry
            .execute(&target_trn, input_payload, Some(ctx))
            .await
            .map(|exec| exec.output)
            .map_err(map_registry_error)
    };

    let output = match timeout(effective_timeout, fut).await {
        Ok(Ok(out)) => out,
        Ok(Err(server_err)) => {
            if let Err(update_err) = run_service
                .update_status(
                    &run_id,
                    OrchestratorRunStatus::Failed,
                    Some("failed".to_string()),
                    None,
                    Some(json!({ "message": server_err.to_string() })),
                )
                .await
            {
                tracing::error!(error = %update_err, run_id = %run_id, "failed to mark run as failed");
            }
            let failure_payload = json!({ "message": server_err.to_string() });
            let failure_event =
                StepflowCommandAdapter::build_failure_event(&run_snapshot, &failure_payload);
            if let Err(err) = outbox_service
                .enqueue(OrchestratorOutboxInsert {
                    run_id: Some(run_id.clone()),
                    protocol: "aionix.event.stepflow".to_string(),
                    payload: failure_event,
                    next_attempt_at: Utc::now(),
                    attempts: 0,
                    last_error: None,
                })
                .await
            {
                tracing::error!(error = %err, run_id = %run_id, "failed to enqueue failure event");
            }
            return Err(server_err.to_http_response(req_id.clone()));
        }
        Err(_) => {
            if let Err(update_err) = run_service
                .update_status(
                    &run_id,
                    OrchestratorRunStatus::TimedOut,
                    Some("timed_out".to_string()),
                    None,
                    Some(json!({ "message": "execution timed out" })),
                )
                .await
            {
                tracing::error!(error = %update_err, run_id = %run_id, "failed to mark run as timed out");
            }
            let timeout_event = StepflowCommandAdapter::build_timeout_event(&run_snapshot);
            if let Err(err) = outbox_service
                .enqueue(OrchestratorOutboxInsert {
                    run_id: Some(run_id.clone()),
                    protocol: "aionix.event.stepflow".to_string(),
                    payload: timeout_event,
                    next_attempt_at: Utc::now(),
                    attempts: 0,
                    last_error: None,
                })
                .await
            {
                tracing::error!(error = %err, run_id = %run_id, "failed to enqueue timeout event");
            }
            let err = ServerError::Timeout;
            return Err(err.to_http_response(req_id.clone()));
        }
    };

    let status_value = output
        .as_object()
        .and_then(|map| map.get("status"))
        .and_then(|value| value.as_str())
        .map(|value| value.to_ascii_lowercase());

    if let Some(status) = status_value.as_deref() {
        match status {
            "running" | "accepted" => {
                let heartbeat_timeout = output
                    .as_object()
                    .and_then(|map| map.get("heartbeatTimeout"))
                    .and_then(|value| value.as_u64())
                    .or(heartbeat_timeout_secs);
                let status_ttl = output
                    .as_object()
                    .and_then(|map| map.get("statusTtl"))
                    .and_then(|value| value.as_u64());
                let phase = output
                    .as_object()
                    .and_then(|map| map.get("phase"))
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
                    .or_else(|| {
                        if status == "running" {
                            Some("async_waiting".to_string())
                        } else {
                            Some("fire_forget".to_string())
                        }
                    });

                if let Err(update_err) = run_service
                    .update_status(
                        &run_id,
                        OrchestratorRunStatus::Running,
                        phase.clone(),
                        Some(output.clone()),
                        None,
                    )
                    .await
                {
                    tracing::error!(error = %update_err, run_id = %run_id, "failed to persist async status payload");
                }

                tracing::info!(
                    request_id = %req_id,
                    command_id = %envelope.id,
                    action = %action_trn_str,
                    duration_ms = %execution_start.elapsed().as_millis(),
                    status = %status,
                    "stepflow command executing asynchronously"
                );

                let handle_value = output.as_object().and_then(|map| map.get("handle").cloned());

                if status == "running" {
                    match handle_value.clone() {
                        Some(handle) => {
                            if let Err(err) =
                                async_manager.submit(run_snapshot.clone(), handle.clone())
                            {
                                tracing::error!(
                                    error = %err,
                                    run_id = %run_id,
                                    "failed to register async handle"
                                );
                            }
                        }
                        None => tracing::warn!(
                            run_id = %run_id,
                            "async response missing handle payload"
                        ),
                    }
                }

                let external_ref = handle_value
                    .as_ref()
                    .and_then(|value| value.as_object())
                    .and_then(|map| map.get("externalRunId"))
                    .and_then(|value| value.as_str())
                    .map(|s| s.to_string());

                let mut metadata_map = run_snapshot
                    .metadata
                    .as_ref()
                    .and_then(|value| value.as_object())
                    .cloned()
                    .unwrap_or_default();
                metadata_map.insert("asyncMode".to_string(), Value::String(status.to_string()));
                if let Some(handle) = handle_value.clone() {
                    metadata_map.insert("asyncHandle".to_string(), handle);
                }

                if let Err(err) = run_service
                    .update_async_metadata(
                        &run_id,
                        Some(Value::Object(metadata_map)),
                        external_ref.clone(),
                    )
                    .await
                {
                    tracing::error!(
                        error = %err,
                        run_id = %run_id,
                        "failed to persist async metadata"
                    );
                }

                let response = StepflowCommandResponse {
                    status: status.to_string(),
                    result: output
                        .as_object()
                        .and_then(|map| map.get("handle").cloned())
                        .or_else(|| output.as_object().and_then(|map| map.get("result").cloned())),
                    run_id: Some(run_id),
                    phase,
                    heartbeat_timeout,
                    status_ttl,
                    correlation_id: envelope.correlation_id.clone(),
                    request_id: Some(req_id),
                };

                return Ok(Json(response));
            }
            _ => {}
        }
    }

    tracing::info!(
        request_id = %req_id,
        command_id = %envelope.id,
        action = %action_trn_str,
        duration_ms = %execution_start.elapsed().as_millis(),
        "stepflow command executed"
    );

    if let Err(update_err) = run_service
        .update_status(
            &run_id,
            OrchestratorRunStatus::Succeeded,
            Some("succeeded".to_string()),
            Some(output.clone()),
            None,
        )
        .await
    {
        tracing::error!(error = %update_err, run_id = %run_id, "failed to mark run as succeeded");
    }

    let success_event = StepflowCommandAdapter::build_success_event(&run_snapshot, &output);
    if let Err(err) = outbox_service
        .enqueue(OrchestratorOutboxInsert {
            run_id: Some(run_id.clone()),
            protocol: "aionix.event.stepflow".to_string(),
            payload: success_event,
            next_attempt_at: Utc::now(),
            attempts: 0,
            last_error: None,
        })
        .await
    {
        tracing::error!(error = %err, run_id = %run_id, "failed to enqueue success event");
    }

    let response = StepflowCommandResponse {
        status: "succeeded".to_string(),
        result: Some(output),
        run_id: Some(run_id),
        phase: None,
        heartbeat_timeout: heartbeat_timeout_secs,
        status_ttl: None,
        correlation_id: Some(
            envelope.correlation_id.clone().unwrap_or_else(|| envelope.id.clone()),
        ),
        request_id: Some(req_id),
    };

    Ok(Json(response))
}

/// Cancel an in-flight Stepflow command run
pub async fn cancel_command(
    State((app_state, governance)): State<(AppState, GovernanceConfig)>,
    Extension(request_id): Extension<RequestId>,
    Path(run_id): Path<String>,
    Json(payload): Json<CancelCommandPayload>,
) -> Result<
    (StatusCode, Json<crate::dto::CancelCommandResponse>),
    (StatusCode, Json<crate::error::ErrorResponse>),
> {
    let req_id = request_id.0.clone();
    let run_service = app_state.run_service.clone();
    let outbox_service = app_state.outbox_service.clone();
    let async_manager = app_state.async_manager.clone();

    let run = run_service
        .get(&run_id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()).to_http_response(req_id.clone()))?
        .ok_or_else(|| {
            ServerError::NotFound(format!("run {} not found", run_id))
                .to_http_response(req_id.clone())
        })?;

    if matches!(
        run.status,
        OrchestratorRunStatus::Cancelled
            | OrchestratorRunStatus::Failed
            | OrchestratorRunStatus::Succeeded
            | OrchestratorRunStatus::TimedOut
    ) {
        let err = ServerError::InvalidInput("run already finished".into());
        return Err(err.to_http_response(req_id));
    }

    let tool_name = ToolName::normalize_action_ref(run.action_trn.as_str())
        .map(|t| t.to_dot_string())
        .ok_or_else(|| {
            let err = ServerError::InvalidInput("unable to derive tool name from TRN".into());
            err.to_http_response(req_id.clone())
        })?;

    if !governance.is_tool_allowed(&tool_name) {
        let err = ServerError::Forbidden(format!("tool not allowed: {}", tool_name));
        return Err(err.to_http_response(req_id.clone()));
    }

    let mut cancel_details = serde_json::Map::new();
    if let Some(reason) = payload.reason.as_ref() {
        cancel_details.insert("reason".into(), Value::String(reason.clone()));
    }
    if let Some(requested_by) = payload.requested_by.as_ref() {
        cancel_details.insert("requestedBy".into(), Value::String(requested_by.clone()));
    }
    if let Some(trace_id) = payload.trace_id.as_ref() {
        cancel_details.insert("traceId".into(), Value::String(trace_id.clone()));
    }

    let error_value =
        if cancel_details.is_empty() { None } else { Some(Value::Object(cancel_details.clone())) };
    let cancel_payload = error_value.clone().unwrap_or(Value::Null);

    if let Some(handle) = run
        .metadata
        .as_ref()
        .and_then(|value| value.as_object())
        .and_then(|map| map.get("asyncHandle"))
    {
        if let Err(err) = async_manager.cancel_run(&run, handle, payload.reason.as_deref()).await {
            tracing::warn!(run_id = %run.run_id, error = %err, "async cancel request failed");
        }
    }

    run_service
        .update_status(
            &run.run_id,
            OrchestratorRunStatus::Cancelled,
            Some("cancelled".to_string()),
            None,
            error_value.clone(),
        )
        .await
        .map_err(|e| ServerError::Internal(e.to_string()).to_http_response(req_id.clone()))?;

    let event = StepflowCommandAdapter::build_cancelled_event(&run, &cancel_payload);
    outbox_service
        .enqueue(OrchestratorOutboxInsert {
            run_id: Some(run.run_id.clone()),
            protocol: "aionix.event.stepflow".into(),
            payload: event,
            next_attempt_at: chrono::Utc::now(),
            attempts: 0,
            last_error: None,
        })
        .await
        .map_err(|e| ServerError::Internal(e.to_string()).to_http_response(req_id.clone()))?;

    let response = crate::dto::CancelCommandResponse {
        accepted: true,
        phase: Some("cancelled".to_string()),
        request_id: Some(req_id.clone()),
    };

    Ok((StatusCode::ACCEPTED, Json(response)))
}
