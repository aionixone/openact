#[cfg(feature = "server")]
use axum::response::IntoResponse;
#[cfg(feature = "server")]
use axum::response::Json;
#[cfg(feature = "server")]
use serde_json::json;
#[cfg(feature = "server")]
use std::time::SystemTime;

/// Health check
#[cfg(feature = "server")]
pub async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "healthy",
        "version": "1.0.0",
        "timestamp": SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }))
}


