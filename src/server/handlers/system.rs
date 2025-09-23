#![cfg(feature = "server")]

use axum::{response::IntoResponse, Json};
use crate::app::service::OpenActService;
use crate::interface::error::helpers;

pub async fn stats() -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    let storage = svc.stats().await;
    let caches = svc.cache_stats().await;
    let cp = crate::executor::client_pool::get_stats();
    
    match (storage, caches) {
        (Ok(s), Ok(c)) => {
            let uptime = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
                
            let memory = get_memory_stats();
            let version_info = get_version_info();
            
            Json(serde_json::json!({
                "storage": s,
                "caches": c,
                "client_pool": {
                    "hits": cp.hits,
                    "builds": cp.builds,
                    "evictions": cp.evictions,
                    "size": cp.size,
                    "capacity": cp.capacity,
                    "hit_rate": if cp.hits + cp.builds > 0 { 
                        cp.hits as f64 / (cp.hits + cp.builds) as f64 
                    } else { 
                        0.0 
                    }
                },
                "system": {
                    "uptime_seconds": uptime,
                    "memory_usage": memory,
                    "version": version_info
                },
                "timestamp": chrono::Utc::now().to_rfc3339()
            })).into_response()
        },
        (Err(e), _) | (_, Err(e)) => helpers::storage_error(e.to_string()).into_response(),
    }
}

/// Get memory usage statistics
fn get_memory_stats() -> serde_json::Value {
    // Basic memory stats - could be enhanced with more detailed metrics
    serde_json::json!({
        "note": "Memory stats require platform-specific implementation"
    })
}

/// Get version and build information
fn get_version_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "name": env!("CARGO_PKG_NAME"),
        "rust_version": env!("CARGO_PKG_RUST_VERSION"),
        "build_profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "features": get_enabled_features()
    })
}

/// Get list of enabled Cargo features
fn get_enabled_features() -> Vec<&'static str> {
    let mut features = Vec::new();
    
    #[cfg(feature = "server")]
    features.push("server");
    
    #[cfg(feature = "callback")]
    features.push("callback");
    
    #[cfg(feature = "vault")]
    features.push("vault");
    
    features
}

/// Add health check endpoint
pub async fn health() -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    
    // Quick health checks
    let storage_ok = svc.stats().await.is_ok();
    let cache_ok = svc.cache_stats().await.is_ok();
    
    let status = if storage_ok && cache_ok {
        "healthy"
    } else {
        "unhealthy"
    };
    
    if storage_ok && cache_ok {
        Json(serde_json::json!({
            "status": status,
            "checks": {
                "storage": if storage_ok { "ok" } else { "error" },
                "cache": if cache_ok { "ok" } else { "error" }
            },
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "version": env!("CARGO_PKG_VERSION")
        })).into_response()
    } else {
        (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({
            "status": status,
            "checks": {
                "storage": if storage_ok { "ok" } else { "error" },
                "cache": if cache_ok { "ok" } else { "error" }
            },
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "version": env!("CARGO_PKG_VERSION")
        }))).into_response()
    }
}

pub async fn cleanup() -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    match svc.cleanup().await {
        Ok(r) => Json(serde_json::json!(r)).into_response(),
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}


