//! Stepflow command handlers

use super::actions::map_registry_error;
use crate::{
    dto::StepflowCommandResponse,
    error::ServerError,
    middleware::{request_id::RequestId, tenant::Tenant},
    AppState,
};
use aionix_protocol::parse_command_envelope;
use axum::{
    extract::{Extension, State},
    Json,
};
use openact_core::types::{ActionTrn, ToolName, Trn};
use openact_mcp::GovernanceConfig;
use openact_registry::ExecutionContext;
use serde_json::Value;
use std::convert::TryFrom;
use std::time::{Duration, Instant};
use tokio::time::timeout;

const SUPPORTED_SCHEMA_PREFIX: &str = "1.";

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
    let execution_start = Instant::now();
    let action_trn_str = target_trn.as_str().to_string();

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
        Ok(res) => res.map_err(|err| err.to_http_response(req_id.clone()))?,
        Err(_) => {
            let err = ServerError::Timeout;
            return Err(err.to_http_response(req_id.clone()));
        }
    };

    tracing::info!(
        request_id = %req_id,
        command_id = %envelope.id,
        action = %action_trn_str,
        duration_ms = %execution_start.elapsed().as_millis(),
        "stepflow command executed"
    );

    let response = StepflowCommandResponse {
        status: "succeeded".to_string(),
        result: Some(output),
        run_id: None,
        phase: None,
        heartbeat_timeout: None,
        status_ttl: None,
        correlation_id: Some(
            envelope.correlation_id.clone().unwrap_or_else(|| envelope.id.clone()),
        ),
        request_id: Some(req_id),
    };

    Ok(Json(response))
}
