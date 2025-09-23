#![cfg(feature = "server")]

use crate::app::service::OpenActService;
use crate::interface::dto::{ExecuteRequestDto, ExecuteResponseDto};
use crate::utils::trn;
use axum::{Json, extract::Path, http::StatusCode, response::IntoResponse};

pub async fn execute(
    Path(trn): Path<String>,
    Json(req): Json<ExecuteRequestDto>,
) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"code":"validation.invalid_trn","message":e.to_string()})),
        ).into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
    match svc.execute_task(&trn, req.overrides).await {
        Ok(res) => {
            let dto = ExecuteResponseDto {
                status: res.status,
                headers: res.headers,
                body: res.body,
            };
            (StatusCode::OK, Json(serde_json::json!(dto))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"code":"internal.execution_failed","message":e.to_string()})),
        )
            .into_response(),
    }
}
