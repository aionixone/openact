#![cfg(feature = "server")]

use axum::{http::StatusCode, response::IntoResponse, Json};
use crate::app::service::OpenActService;

pub async fn stats() -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    let storage = svc.stats().await;
    let caches = svc.cache_stats().await;
    let cp = crate::executor::client_pool::get_stats();
    match (storage, caches) {
        (Ok(s), Ok(c)) => (StatusCode::OK, Json(serde_json::json!({
            "storage": s,
            "caches": c,
            "client_pool": {
                "hits": cp.hits,
                "builds": cp.builds,
                "evictions": cp.evictions,
                "size": cp.size,
                "capacity": cp.capacity
            }
        }))).into_response(),
        (Err(e), _) | (_, Err(e)) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"code":"internal.storage_error","message":e.to_string()}))).into_response(),
    }
}

pub async fn cleanup() -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    match svc.cleanup().await {
        Ok(r) => (StatusCode::OK, Json(serde_json::json!(r))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"code":"internal.storage_error","message":e.to_string()}))).into_response(),
    }
}


