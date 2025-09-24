#![cfg(feature = "server")]

use crate::app::service::OpenActService;
use crate::interface::dto::{ExecuteRequestDto, ExecuteResponseDto, AdhocExecuteRequestDto};
use crate::interface::error::helpers;
use crate::utils::trn;
use axum::{Json, extract::{Path, State}, response::IntoResponse};

#[cfg(feature = "openapi")]
#[allow(unused_imports)] // Used in utoipa path examples
use serde_json::json;

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/tasks/{trn}/execute",
    tag = "execution",
    operation_id = "tasks_execute",
    summary = "Execute task",
    description = "Execute a task configuration with optional overrides",
    params(
        ("trn" = String, Path, description = "Task TRN identifier", example = "trn:openact:my-tenant:task/api-ping@v1")
    ),
    request_body = ExecuteRequestDto,
    responses(
        (status = 200, description = "Task executed successfully", body = ExecuteResponseDto,
            example = json!({
                "status": 200,
                "headers": {"content-type": "application/json"},
                "body": {"result": "success", "data": {"id": 123}},
                "execution_time_ms": 250
            })
        ),
        (status = 400, description = "Invalid TRN format or execution parameters", body = crate::interface::error::ApiError,
            examples(
                ("invalid_trn" = (summary = "Invalid TRN format", value = json!({
                    "error_code": "validation.invalid_trn",
                    "message": "Invalid TRN format: missing required components",
                    "hints": ["Use format: trn:openact:tenant:task/id@version"]
                }))),
                ("invalid_overrides" = (summary = "Invalid parameter overrides", value = json!({
                    "error_code": "validation.invalid_input",
                    "message": "Invalid override parameters",
                    "hints": ["Check parameter format", "Ensure required fields are present"]
                })))
            )
        ),
        (status = 404, description = "Task not found", body = crate::interface::error::ApiError,
            example = json!({
                "error_code": "not_found.task",
                "message": "not found",
                "hints": ["Verify task TRN", "Check task exists in tenant"]
            })
        ),
        (status = 500, description = "Internal server error or execution failed", body = crate::interface::error::ApiError,
            examples(
                ("execution_failed" = (summary = "Task execution failed", value = json!({
                    "error_code": "internal.execution_failed",
                    "message": "HTTP request timeout",
                    "hints": ["Retry request", "Check target service availability"]
                }))),
                ("storage_error" = (summary = "Database error", value = json!({
                    "error_code": "internal.storage_error",
                    "message": "Failed to load task configuration",
                    "hints": ["Contact administrator", "Check system health"]
                })))
            )
        )
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
        (status = 200, description = "Ad-hoc action executed successfully", body = ExecuteResponseDto,
            example = json!({
                "status": 200,
                "headers": {"content-type": "application/json", "x-ratelimit-remaining": "99"},
                "body": {"users": [{"id": 1, "name": "John"}]},
                "execution_time_ms": 180
            })
        ),
        (status = 400, description = "Invalid connection TRN or execution parameters", body = crate::interface::error::ApiError,
            examples(
                ("invalid_connection_trn" = (summary = "Invalid connection TRN", value = json!({
                    "error_code": "validation.invalid_connection_trn",
                    "message": "Invalid TRN format: missing required components",
                    "hints": ["Use format: trn:openact:tenant:connection/id@version"]
                }))),
                ("missing_method" = (summary = "Missing HTTP method", value = json!({
                    "error_code": "validation.missing_method",
                    "message": "HTTP method is required",
                    "hints": ["Provide method: GET, POST, PUT, DELETE, etc."]
                }))),
                ("missing_endpoint" = (summary = "Missing API endpoint", value = json!({
                    "error_code": "validation.missing_endpoint",
                    "message": "API endpoint is required",
                    "hints": ["Provide full URL including protocol"]
                })))
            )
        ),
        (status = 404, description = "Connection not found", body = crate::interface::error::ApiError,
            example = json!({
                "error_code": "not_found.connection",
                "message": "not found",
                "hints": ["Verify connection TRN", "Check connection exists in tenant"]
            })
        ),
        (status = 422, description = "Invalid request body format", body = crate::interface::error::ApiError,
            example = json!({
                "error_code": "validation.invalid_request_body",
                "message": "Failed to deserialize request body",
                "hints": ["Check JSON format", "Ensure all required fields are present"]
            })
        ),
        (status = 500, description = "Internal server error or execution failed", body = crate::interface::error::ApiError,
            examples(
                ("execution_failed" = (summary = "HTTP request failed", value = json!({
                    "error_code": "internal.execution_failed",
                    "message": "Connection timeout",
                    "hints": ["Check target service availability", "Retry with shorter timeout"]
                }))),
                ("auth_failed" = (summary = "Authentication failed", value = json!({
                    "error_code": "internal.execution_failed",
                    "message": "Invalid API credentials",
                    "hints": ["Check connection configuration", "Verify API key/token validity"]
                })))
            )
        )
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
