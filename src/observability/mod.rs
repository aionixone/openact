//! Observability module for logging, metrics, and tracing
//! 
//! This module provides unified observability capabilities including:
//! - Structured logging with tracing
//! - Metrics collection and export
//! - Distributed tracing spans
//! - Debug and metrics endpoints

pub mod logging;
pub mod metrics;
pub mod tracing_config;

// No-op metrics implementation when metrics feature is disabled
#[cfg(not(feature = "metrics"))]
pub mod noop_metrics;

#[cfg(feature = "server")]
pub mod endpoints;

use anyhow::Result;

/// Initialize observability systems
pub fn init() -> Result<()> {
    logging::init()?;
    metrics::init()?;
    Ok(())
}

/// Initialize observability with custom configuration
pub fn init_with_config(log_level: &str, json_logs: bool, metrics_enabled: bool) -> Result<()> {
    logging::init_with_config(log_level, json_logs)?;
    if metrics_enabled {
        metrics::init()?;
    }
    Ok(())
}

/// Common observability fields and helpers
pub mod fields {
    /// Request ID field name
    pub const REQUEST_ID: &str = "request_id";
    /// Task TRN field name  
    pub const TASK_TRN: &str = "task_trn";
    /// Connection TRN field name
    pub const CONNECTION_TRN: &str = "connection_trn";
    /// HTTP status code field name
    pub const HTTP_STATUS: &str = "http_status";
    /// HTTP method field name
    pub const HTTP_METHOD: &str = "http_method";
    /// Duration field name
    pub const DURATION_MS: &str = "duration_ms";
    /// Retry attempt field name
    pub const RETRY_ATTEMPT: &str = "retry_attempt";
    /// Error code field name
    pub const ERROR_CODE: &str = "error_code";
}

/// Generate a new request ID
pub fn generate_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
