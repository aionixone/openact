#![cfg(feature = "server")]

use crate::app::service::OpenActService;
use crate::models::TaskConfig;
use axum::{
    Json,
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub connection_trn: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list(Query(q): Query<ListQuery>) -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    match svc
        .list_tasks(q.connection_trn.as_deref(), q.limit, q.offset)
        .await
    {
        Ok(list) => (StatusCode::OK, Json(serde_json::json!(list))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"code":"internal.storage_error","message":e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn create(Json(body): Json<TaskConfig>) -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    match svc.upsert_task(&body).await {
        Ok(_) => (StatusCode::CREATED, Json(serde_json::json!(body))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"code":"validation.invalid_input","message":e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn get(Path(trn): Path<String>) -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    match svc.get_task(&trn).await {
        Ok(Some(task)) => (StatusCode::OK, Json(serde_json::json!(task))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"code":"not_found.task","message":"not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"code":"internal.storage_error","message":e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn update(
    Path(trn): Path<String>,
    Json(body): Json<TaskConfig>,
) -> impl IntoResponse {
    if body.trn != trn {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"code":"validation.trn_mismatch","message":"trn mismatch"})),
        )
            .into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
    match svc.upsert_task(&body).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!(body))).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"code":"validation.invalid_input","message":e.to_string()})),
        )
            .into_response(),
    }
}

pub async fn del(Path(trn): Path<String>) -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    match svc.delete_task(&trn).await {
        Ok(true) => (StatusCode::NO_CONTENT, Json(serde_json::Value::Null)).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"code":"not_found.task","message":"not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"code":"internal.storage_error","message":e.to_string()})),
        )
            .into_response(),
    }
}
