#[cfg(feature = "server")]
use axum::response::IntoResponse;
#[cfg(feature = "server")]
use axum::response::Json;
#[cfg(feature = "server")]
use serde_json::json;
#[cfg(feature = "server")]
use std::time::SystemTime;

#[cfg(all(feature = "server", feature = "openapi"))]
use utoipa;

/// Health check
#[cfg(feature = "server")]
#[cfg_attr(all(feature = "server", feature = "openapi"), utoipa::path(
    get,
    path = "/api/v1/authflow/health",
    tag = "authflow",
    operation_id = "authflow_health_check",
    summary = "AuthFlow health check",
    description = "Check the health and status of the AuthFlow service",
    responses(
        (status = 200, description = "AuthFlow service is healthy", body = serde_json::Value)
    )
))]
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


