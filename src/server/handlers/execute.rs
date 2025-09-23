#![cfg(feature = "server")]

use crate::app::service::OpenActService;
use crate::interface::dto::{ExecuteRequestDto, ExecuteResponseDto, AdhocExecuteRequestDto};
use crate::interface::error::helpers;
use crate::utils::trn;
use axum::{Json, extract::Path, response::IntoResponse};

pub async fn execute(
    Path(trn): Path<String>,
    Json(req): Json<ExecuteRequestDto>,
) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
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

/// Execute ad-hoc action using existing connection
pub async fn execute_adhoc(
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
    
    let svc = OpenActService::from_env().await.unwrap();
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
