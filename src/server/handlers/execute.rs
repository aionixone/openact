#![cfg(feature = "server")]

use crate::app::service::OpenActService;
use crate::interface::dto::{ExecuteRequestDto, ExecuteResponseDto};
use axum::{Json, extract::Path, http::StatusCode, response::IntoResponse};

pub async fn execute(
    Path(trn): Path<String>,
    Json(req): Json<ExecuteRequestDto>,
) -> impl IntoResponse {
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
