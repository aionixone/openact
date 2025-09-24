#![cfg(feature = "server")]

use crate::app::service::OpenActService;
use crate::interface::dto::{ExecuteRequestDto, ExecuteResponseDto, AdhocExecuteRequestDto};
use crate::interface::error::helpers;
use crate::utils::trn;
use axum::{Json, extract::{Path, State}, response::IntoResponse};

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/tasks/{trn}/execute",
    tag = "execution",
    operation_id = "tasks_execute",
    summary = "Execute task",
    description = "Execute a task configuration with optional overrides",
    params(
        ("trn" = String, Path, description = "Task TRN identifier")
    ),
    request_body = ExecuteRequestDto,
    responses(
        (status = 200, description = "Task executed successfully", body = ExecuteResponseDto),
        (status = 400, description = "Invalid TRN format or execution parameters", body = crate::interface::error::ApiError),
        (status = 404, description = "Task not found", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error or execution failed", body = crate::interface::error::ApiError)
    )
))]
pub async fn execute(
    State(svc): State<OpenActService>,
    Path(trn): Path<String>,
    Json(req): Json<ExecuteRequestDto>,
) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    match svc.execute_task(&trn, req.overrides).await {
        Ok(res) => {
            let dto = ExecuteResponseDto {
                status: res.status,
                headers: res.headers,
                body: res.body,
            };
            Json(serde_json::json!(dto)).into_response()
        }
        Err(e) => helpers::execution_error(e.to_string()).into_response(),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/execute/adhoc",
    tag = "execution",
    operation_id = "execute_adhoc",
    summary = "Execute ad-hoc action",
    description = "Execute an ad-hoc action using an existing connection without creating a persistent task",
    request_body = AdhocExecuteRequestDto,
    responses(
        (status = 200, description = "Ad-hoc action executed successfully", body = ExecuteResponseDto),
        (status = 400, description = "Invalid connection TRN or execution parameters", body = crate::interface::error::ApiError),
        (status = 404, description = "Connection not found", body = crate::interface::error::ApiError),
        (status = 500, description = "Internal server error or execution failed", body = crate::interface::error::ApiError)
    )
))]
/// Execute ad-hoc action using existing connection
pub async fn execute_adhoc(
    State(svc): State<OpenActService>,
    Json(req): Json<AdhocExecuteRequestDto>,
) -> impl IntoResponse {
    // Validate connection TRN
    if let Err(e) = trn::validate_trn(&req.connection_trn) {
        return helpers::validation_error("invalid_connection_trn", e.to_string()).into_response();
    }
    
    // Basic validation
    if req.method.is_empty() {
        return helpers::validation_error("missing_method", "HTTP method is required").into_response();
    }
    if req.endpoint.is_empty() {
        return helpers::validation_error("missing_endpoint", "API endpoint is required").into_response();
    }
    match svc.execute_adhoc(req).await {
        Ok(res) => {
            let dto = ExecuteResponseDto {
                status: res.status,
                headers: res.headers,
                body: res.body,
            };
            Json(serde_json::json!(dto)).into_response()
        }
        Err(e) => helpers::execution_error(e.to_string()).into_response(),
    }
}
