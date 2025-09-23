#![cfg(feature = "server")]

use crate::app::service::OpenActService;
use crate::interface::dto::TaskUpsertRequest;
use crate::interface::error::helpers;
use crate::utils::trn;
use axum::{
    Json,
    extract::{Path, Query},
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
        Ok(list) => Json(serde_json::json!(list)).into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}

pub async fn create(Json(req): Json<TaskUpsertRequest>) -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    
    // Convert DTO to config with metadata (new creation)
    let config = req.to_config(None, None);
    
    match svc.upsert_task(&config).await {
        Ok(_) => (axum::http::StatusCode::CREATED, Json(serde_json::json!(config))).into_response(),
        Err(e) => helpers::validation_error("invalid_input", e.to_string()).into_response(),
    }
}

pub async fn get(Path(trn): Path<String>) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
    match svc.get_task(&trn).await {
        Ok(Some(task)) => Json(serde_json::json!(task)).into_response(),
        Ok(None) => helpers::not_found_error("task").into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}

pub async fn update(
    Path(trn): Path<String>,
    Json(req): Json<TaskUpsertRequest>,
) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    if req.trn != trn {
        return helpers::validation_error("trn_mismatch", "trn mismatch").into_response();
    }
    
    let svc = OpenActService::from_env().await.unwrap();
    
    // Get existing version and created_at for proper versioning
    let (existing_version, existing_created_at) = match svc.get_task(&trn).await {
        Ok(Some(existing)) => (Some(existing.version), Some(existing.created_at)),
        Ok(None) => (None, None), // Treat as creation if doesn't exist
        Err(e) => return helpers::storage_error(e.to_string()).into_response(),
    };
    
    // Convert DTO to config with proper versioning
    let config = req.to_config(existing_version, existing_created_at);
    
    match svc.upsert_task(&config).await {
        Ok(_) => Json(serde_json::json!(config)).into_response(),
        Err(e) => helpers::validation_error("invalid_input", e.to_string()).into_response(),
    }
}

pub async fn del(Path(trn): Path<String>) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
    match svc.delete_task(&trn).await {
        Ok(true) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Ok(false) => helpers::not_found_error("task").into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}
