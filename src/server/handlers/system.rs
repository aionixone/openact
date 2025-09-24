#![cfg(feature = "server")]

use crate::app::service::OpenActService;
use crate::interface::error::helpers;
use axum::{Json, response::IntoResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;
#[cfg(feature = "openapi")]
#[allow(unused_imports)] // Used in schema examples via json! macro
use serde_json::json;

/// Client pool statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ClientPoolStats {
    /// Number of cache hits
    pub hits: u64,
    /// Number of new client builds
    pub builds: u64,
    /// Number of evictions
    pub evictions: u64,
    /// Current pool size
    pub size: usize,
    /// Maximum pool capacity
    pub capacity: usize,
    /// Hit rate (0.0 to 1.0)
    pub hit_rate: f64,
}

/// Memory usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct MemoryStats {
    /// Note about memory stats implementation
    pub note: String,
}

/// Version and build information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct VersionInfo {
    /// Application version
    pub version: String,
    /// Git commit hash (if available)
    pub git_hash: Option<String>,
    /// Build timestamp (if available)
    pub build_time: Option<String>,
    /// Rust version used for build
    pub rust_version: String,
}

/// System statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SystemInfo {
    /// System uptime in seconds
    pub uptime_seconds: u64,
    /// Memory usage statistics
    pub memory_usage: MemoryStats,
    /// Version and build information
    pub version: VersionInfo,
}

/// Complete system statistics response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(example = json!({
    "storage": {
        "connections_count": 15,
        "tasks_count": 42,
        "auth_connections_count": 8,
        "total_executions": 156
    },
    "caches": {
        "exec_lookups": 1250,
        "exec_hits": 1100,
        "exec_hit_rate": 0.88,
        "conn_lookups": 890,
        "conn_hits": 850,
        "conn_hit_rate": 0.955
    },
    "client_pool": {
        "hits": 2340,
        "builds": 156,
        "evictions": 12,
        "size": 45,
        "capacity": 100,
        "hit_rate": 0.937
    },
    "system": {
        "uptime_seconds": 86400,
        "memory_usage": {
            "note": "Memory stats require platform-specific implementation"
        },
        "version": {
            "version": "0.1.0",
            "git_hash": "abc123def456",
            "build_time": "2023-12-01T10:00:00Z",
            "rust_version": "1.75.0"
        }
    },
    "timestamp": "2023-12-01T15:30:45Z"
})))]
pub struct SystemStatsResponse {
    /// Storage statistics
    pub storage: serde_json::Value, // Keep as Value since it comes from service
    /// Cache statistics
    pub caches: serde_json::Value, // Keep as Value since it comes from service
    /// Client pool statistics
    pub client_pool: ClientPoolStats,
    /// System information
    pub system: SystemInfo,
    /// Response timestamp
    pub timestamp: DateTime<Utc>,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(example = json!({
    "status": "healthy",
    "timestamp": "2023-12-01T15:30:45Z",
    "components": {
        "database": {
            "status": "healthy",
            "message": null
        },
        "storage": {
            "status": "healthy", 
            "message": null
        }
    }
})))]
pub struct HealthResponse {
    /// Overall health status
    pub status: String,
    /// Health check timestamp
    pub timestamp: DateTime<Utc>,
    /// Detailed component status
    pub components: HealthComponents,
}

/// Health status of system components
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct HealthComponents {
    /// Database connectivity status
    pub database: ComponentHealth,
    /// Storage service status
    pub storage: ComponentHealth,
}

/// Individual component health status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ComponentHealth {
    /// Component status (healthy, degraded, unhealthy)
    pub status: String,
    /// Optional error message
    pub message: Option<String>,
}

/// Cleanup operation response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(example = json!({
    "message": "System cleanup completed successfully",
    "cleaned_count": 15,
    "timestamp": "2023-12-01T15:30:45Z"
})))]
pub struct CleanupResponse {
    /// Success message
    pub message: String,
    /// Number of cleaned items
    pub cleaned_count: u64,
    /// Cleanup operation timestamp
    pub timestamp: DateTime<Utc>,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/system/stats",
    tag = "system",
    operation_id = "system_get_stats",
    summary = "System statistics",
    description = "Get detailed system statistics including storage, cache, client pool, and memory usage",
    responses(
        (status = 200, description = "System statistics", body = SystemStatsResponse),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
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

            let response = SystemStatsResponse {
                storage: serde_json::to_value(s).unwrap_or_default(),
                caches: serde_json::to_value(c).unwrap_or_default(),
                client_pool: ClientPoolStats {
                    hits: cp.hits,
                    builds: cp.builds,
                    evictions: cp.evictions,
                    size: cp.size,
                    capacity: cp.capacity,
                    hit_rate: if cp.hits + cp.builds > 0 {
                        cp.hits as f64 / (cp.hits + cp.builds) as f64
                    } else {
                        0.0
                    },
                },
                system: SystemInfo {
                    uptime_seconds: uptime,
                    memory_usage: get_memory_stats(),
                    version: get_version_info(),
                },
                timestamp: Utc::now(),
            };

            Json(response).into_response()
        }
        (Err(e), _) | (_, Err(e)) => helpers::storage_error(e.to_string()).into_response(),
    }
}

/// Get memory usage statistics
fn get_memory_stats() -> MemoryStats {
    // Basic memory stats - could be enhanced with more detailed metrics
    MemoryStats {
        note: "Memory stats require platform-specific implementation".to_string(),
    }
}

/// Get version and build information
fn get_version_info() -> VersionInfo {
    VersionInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        git_hash: option_env!("GIT_HASH").map(|s| s.to_string()),
        build_time: option_env!("BUILD_TIME").map(|s| s.to_string()),
        rust_version: env!("CARGO_PKG_RUST_VERSION").to_string(),
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/api/v1/system/health",
    tag = "system",
    operation_id = "system_health_check",
    summary = "Health check",
    description = "Get system health status including storage and cache connectivity",
    responses(
        (status = 200, description = "System is healthy", body = HealthResponse),
        (status = 503, description = "System is unhealthy", body = HealthResponse)
    ),
    security(
        // 健康检查无需认证
        ()
    )
))]
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

    let response = HealthResponse {
        status: status.to_string(),
        timestamp: Utc::now(),
        components: HealthComponents {
            database: ComponentHealth {
                status: if storage_ok { "healthy" } else { "unhealthy" }.to_string(),
                message: if storage_ok {
                    None
                } else {
                    Some("Storage connectivity check failed".to_string())
                },
            },
            storage: ComponentHealth {
                status: if cache_ok { "healthy" } else { "unhealthy" }.to_string(),
                message: if cache_ok {
                    None
                } else {
                    Some("Cache connectivity check failed".to_string())
                },
            },
        },
    };

    if storage_ok && cache_ok {
        Json(response).into_response()
    } else {
        (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(response)).into_response()
    }
}

#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/api/v1/system/cleanup",
    tag = "system",
    operation_id = "system_cleanup",
    summary = "System cleanup",
    description = "Perform system cleanup operations including cache clearing and resource optimization",
    responses(
        (status = 200, description = "Cleanup completed successfully", body = CleanupResponse),
        (status = 500, description = "Internal server error", body = crate::interface::error::ApiError)
    )
))]
pub async fn cleanup() -> impl IntoResponse {
    let svc = OpenActService::from_env().await.unwrap();
    match svc.cleanup().await {
        Ok(result) => {
            let response = CleanupResponse {
                message: "System cleanup completed successfully".to_string(),
                cleaned_count: result.expired_auth_connections,
                timestamp: Utc::now(),
            };
            Json(response).into_response()
        }
        Err(e) => helpers::storage_error(e.to_string()).into_response(),
    }
}
