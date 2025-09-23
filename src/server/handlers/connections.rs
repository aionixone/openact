#![cfg(feature = "server")]

use crate::app::service::OpenActService;
use crate::interface::dto::ConnectionUpsertRequest;
use crate::interface::error::helpers;
use crate::utils::trn;
use axum::{
    Json,
    extract::{Path, Query},
    response::IntoResponse,
};
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub auth_type: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list(Query(q): Query<ListQuery>) -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    match svc
        .list_connections(q.auth_type.as_deref(), q.limit, q.offset)
        .await
    {
        Ok(list) => Json(serde_json::json!(list)).into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}

pub async fn create(Json(req): Json<ConnectionUpsertRequest>) -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    
    // Convert DTO to config with metadata (new creation)
    let config = req.to_config(None, None);
    
    match svc.upsert_connection(&config).await {
        Ok(_) => (axum::http::StatusCode::CREATED, Json(serde_json::json!(config))).into_response(),
        Err(e) => helpers::validation_error("invalid_input", e.to_string()).into_response(),
    }
}

pub async fn get(Path(trn): Path<String>) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
    match svc.get_connection(&trn).await {
        Ok(Some(conn)) => Json(serde_json::json!(conn)).into_response(),
        Ok(None) => helpers::not_found_error("connection").into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}

pub async fn update(
    Path(trn): Path<String>,
    Json(req): Json<ConnectionUpsertRequest>,
) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    if req.trn != trn {
        return helpers::validation_error("trn_mismatch", "trn mismatch").into_response();
    }
    
    let svc = OpenActService::from_env().await.unwrap();
    
    // Get existing version and created_at for proper versioning
    let (existing_version, existing_created_at) = match svc.get_connection(&trn).await {
        Ok(Some(existing)) => (Some(existing.version), Some(existing.created_at)),
        Ok(None) => (None, None), // Treat as creation if doesn't exist
        Err(e) => return helpers::storage_error(e.to_string()).into_response(),
    };
    
    // Convert DTO to config with proper versioning
    let config = req.to_config(existing_version, existing_created_at);
    
    match svc.upsert_connection(&config).await {
        Ok(_) => Json(serde_json::json!(config)).into_response(),
        Err(e) => helpers::validation_error("invalid_input", e.to_string()).into_response(),
    }
}

pub async fn del(Path(trn): Path<String>) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
    match svc.delete_connection(&trn).await {
        Ok(true) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Ok(false) => helpers::not_found_error("connection").into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}

/// Get connection auth status (no network)
pub async fn status(Path(trn): Path<String>) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
    match svc.connection_status(&trn).await {
        Ok(Some(s)) => Json(serde_json::json!(s)).into_response(),
        Ok(None) => helpers::not_found_error("connection").into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ConnectionTestRequest {
    #[serde(default = "default_test_endpoint")]
    pub endpoint: String,
}

fn default_test_endpoint() -> String { "https://httpbin.org/get".to_string() }

/// Test a connection by performing a simple GET to the given endpoint
pub async fn test(
    Path(trn): Path<String>,
    Json(req): Json<ConnectionTestRequest>,
) -> impl IntoResponse {
    if let Err(e) = trn::validate_trn(&trn) {
        return helpers::validation_error("invalid_trn", e.to_string()).into_response();
    }
    let svc = OpenActService::from_env().await.unwrap();
    let exists = match svc.get_connection(&trn).await {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(e) => return helpers::storage_error(e.to_string()).into_response(),
    };
    if !exists {
        return helpers::not_found_error("connection").into_response();
    }
    let req = crate::interface::dto::AdhocExecuteRequestDto {
        connection_trn: trn,
        method: "GET".to_string(),
        endpoint: req.endpoint,
        headers: None,
        query: None,
        body: None,
        timeout_config: None,
        network_config: None,
        http_policy: None,
        response_policy: None,
        retry_policy: None,
    };
    match svc.execute_adhoc(req).await {
        Ok(res) => Json(serde_json::json!({
            "status": res.status,
            "headers": res.headers,
            "body": res.body,
        })).into_response(),
        Err(e) => helpers::execution_error(e.to_string()).into_response(),
    }
}
