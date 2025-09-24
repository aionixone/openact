//! Observability HTTP endpoints for metrics and debugging
//! 
//! Provides /metrics and /debug endpoints for monitoring

#[cfg(feature = "server")]
use axum::{http::StatusCode, response::IntoResponse, Json};


/// Health check endpoint with detailed status
#[cfg(feature = "server")]
pub async fn detailed_health() -> impl IntoResponse {
    use crate::app::service::OpenActService;
    
    let service = OpenActService::from_env().await.unwrap();
    let storage_ok = service.stats().await.is_ok();
    let cache_ok = service.cache_stats().await.is_ok();
    
    let overall_status = if storage_ok && cache_ok { "healthy" } else { "unhealthy" };
    let status_code = if storage_ok && cache_ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    
    let response = serde_json::json!({
        "status": overall_status,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "checks": {
            "storage": if storage_ok { "ok" } else { "error" },
            "cache": if cache_ok { "ok" } else { "error" },
            "metrics": "ok"
        },
        "uptime_seconds": get_uptime_seconds(),
        "build_info": {
            "version": env!("CARGO_PKG_VERSION"),
            "name": env!("CARGO_PKG_NAME"),
            "build_profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "features": get_enabled_features()
        }
    });
    
    (status_code, Json(response))
}

/// Metrics endpoint (Prometheus format when metrics feature is enabled)
#[cfg(all(feature = "server", feature = "metrics"))]
pub async fn metrics_endpoint() -> impl IntoResponse {
    
    // This would require setting up the Prometheus recorder
    // For now, return a placeholder
    (StatusCode::OK, "# Prometheus metrics would be here\n")
}

/// Metrics endpoint (JSON format when metrics feature is disabled)
#[cfg(all(feature = "server", not(feature = "metrics")))]
pub async fn metrics_endpoint() -> impl IntoResponse {
    let metrics_snapshot = crate::observability::metrics::get_metrics_snapshot();
    (StatusCode::OK, Json(metrics_snapshot))
}

/// Debug endpoint with internal state information
#[cfg(feature = "server")]
pub async fn debug_info() -> impl IntoResponse {
    use crate::app::service::OpenActService;
    use crate::executor::client_pool;
    
    let service = OpenActService::from_env().await.unwrap();
    
    let storage_stats = service.stats().await.unwrap_or_default();
    let cache_stats = service.cache_stats().await.unwrap_or_default();
    let client_pool_stats = client_pool::get_stats();
    
    let debug_info = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "process": {
            "pid": std::process::id(),
            "uptime_seconds": get_uptime_seconds(),
            "memory_usage": get_memory_info()
        },
        "storage": storage_stats,
        "cache": cache_stats,
        "client_pool": {
            "hits": client_pool_stats.hits,
            "builds": client_pool_stats.builds,
            "evictions": client_pool_stats.evictions,
            "size": client_pool_stats.size,
            "capacity": client_pool_stats.capacity,
            "hit_rate": if client_pool_stats.hits + client_pool_stats.builds > 0 {
                client_pool_stats.hits as f64 / (client_pool_stats.hits + client_pool_stats.builds) as f64
            } else {
                0.0
            }
        },
        "configuration": {
            "log_level": std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
            "json_logs": std::env::var("OPENACT_JSON_LOGS").unwrap_or_else(|_| "false".to_string()),
            "metrics_enabled": std::env::var("OPENACT_METRICS_ENABLED").unwrap_or_else(|_| "true".to_string()),
            "database_url": std::env::var("OPENACT_DATABASE_URL").unwrap_or_else(|_| "memory".to_string()),
        },
        "runtime": {
            "tokio_workers": tokio::runtime::Handle::current().metrics().num_workers(),
            "note": "Some runtime metrics require specific Tokio features"
        }
    });
    
    (StatusCode::OK, Json(debug_info))
}

/// Get application uptime in seconds
fn get_uptime_seconds() -> u64 {
    use std::sync::LazyLock;
    use std::time::Instant;
    
    static START_TIME: LazyLock<Instant> = LazyLock::new(|| Instant::now());
    START_TIME.elapsed().as_secs()
}

/// Get memory usage information
fn get_memory_info() -> serde_json::Value {
    serde_json::json!({
        "note": "Detailed memory stats require platform-specific implementation",
        "rss_hint": "Use external tools like 'ps' or 'top' for memory usage"
    })
}

/// Get enabled features list
fn get_enabled_features() -> Vec<&'static str> {
    let mut features = Vec::new();
    
    #[cfg(feature = "server")]
    features.push("server");
    
    #[cfg(feature = "callback")]
    features.push("callback");
    
    #[cfg(feature = "vault")]
    features.push("vault");
    
    #[cfg(feature = "metrics")]
    features.push("metrics");
    
    features
}
