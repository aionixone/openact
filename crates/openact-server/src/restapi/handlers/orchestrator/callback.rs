use aionix_contracts::{
    status::{display_label, from_openact_status, ServiceStatus},
    EventEnvelope,
};
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use openact_core::orchestration::{
    OrchestratorOutboxInsert, OrchestratorRunRecord, OrchestratorRunStatus,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    error::ServerError,
    middleware::request_id::RequestId,
    orchestration::{OutboxService, RunService, StepflowCommandAdapter},
    AppState,
};

#[derive(Deserialize)]
pub struct CompletionPayload {
    pub status: String,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<Value>,
}

pub async fn mark_completion(
    State((app_state, _)): State<(AppState, crate::GovernanceConfig)>,
    Extension(request_id): Extension<RequestId>,
    Path(run_id): Path<String>,
    Json(payload): Json<CompletionPayload>,
) -> Result<Json<Value>, (axum::http::StatusCode, Json<crate::error::ErrorResponse>)> {
    let req_id = request_id.0.clone();
    let run_service: RunService = app_state.run_service.clone();
    let outbox_service: OutboxService = app_state.outbox_service.clone();

    let run = run_service
        .get(&run_id)
        .await
        .map_err(|e| ServerError::Internal(e.to_string()).to_http_response(req_id.clone()))?
        .ok_or_else(|| {
            ServerError::NotFound(format!("run {} not found", run_id))
                .to_http_response(req_id.clone())
        })?;

    let status = match from_openact_status(&payload.status) {
        Some(status) => status,
        None => {
            let err =
                ServerError::InvalidInput(format!("unsupported status '{}'.", payload.status));
            return Err(err.to_http_response(req_id.clone()));
        }
    };

    match status {
        ServiceStatus::Succeeded => {
            let result_value = payload.result.unwrap_or(Value::Null);
            run_service
                .update_status(
                    &run.run_id,
                    OrchestratorRunStatus::Succeeded,
                    Some(display_label(ServiceStatus::Succeeded).into_owned()),
                    Some(result_value.clone()),
                    None,
                )
                .await
                .map_err(|e| {
                    ServerError::Internal(e.to_string()).to_http_response(req_id.clone())
                })?;

            let event = StepflowCommandAdapter::build_success_event(&run, &result_value);
            enqueue_event(&outbox_service, &run, event, &req_id).await?;
        }
        ServiceStatus::Failed => {
            let error_value =
                payload.error.unwrap_or_else(|| json!({ "message": "Run reported failure" }));
            run_service
                .update_status(
                    &run.run_id,
                    OrchestratorRunStatus::Failed,
                    Some(display_label(ServiceStatus::Failed).into_owned()),
                    None,
                    Some(error_value.clone()),
                )
                .await
                .map_err(|e| {
                    ServerError::Internal(e.to_string()).to_http_response(req_id.clone())
                })?;

            let event = StepflowCommandAdapter::build_failure_event(&run, &error_value);
            enqueue_event(&outbox_service, &run, event, &req_id).await?;
        }
        ServiceStatus::Cancelled => {
            let error_value =
                payload.error.unwrap_or_else(|| json!({ "message": "Run cancelled" }));
            run_service
                .update_status(
                    &run.run_id,
                    OrchestratorRunStatus::Cancelled,
                    Some(display_label(ServiceStatus::Cancelled).into_owned()),
                    None,
                    Some(error_value.clone()),
                )
                .await
                .map_err(|e| {
                    ServerError::Internal(e.to_string()).to_http_response(req_id.clone())
                })?;

            let event = StepflowCommandAdapter::build_cancelled_event(&run, &error_value);
            enqueue_event(&outbox_service, &run, event, &req_id).await?;
        }
        other => {
            let err = ServerError::InvalidInput(format!(
                "status '{}' cannot be reported via completion endpoint.",
                display_label(other)
            ));
            return Err(err.to_http_response(req_id));
        }
    }

    Ok(Json(json!({ "status": "accepted" })))
}

async fn enqueue_event(
    outbox: &OutboxService,
    run: &OrchestratorRunRecord,
    envelope: EventEnvelope,
    request_id: &str,
) -> Result<(), (axum::http::StatusCode, Json<crate::error::ErrorResponse>)> {
    let payload =
        serde_json::to_value(&envelope).expect("serialize orchestrator callback event envelope");
    outbox
        .enqueue(OrchestratorOutboxInsert {
            run_id: Some(run.run_id.clone()),
            protocol: "aionix.event.stepflow".into(),
            payload,
            next_attempt_at: chrono::Utc::now(),
            attempts: 0,
            last_error: None,
        })
        .await
        .map_err(|e| {
            ServerError::Internal(e.to_string()).to_http_response(request_id.to_string())
        })?;
    Ok(())
}
