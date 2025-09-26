//! Logging configuration and initialization
//!
//! Provides structured logging with tracing-subscriber and optional JSON output

use anyhow::Result;
use std::str::FromStr;
use tracing_subscriber::{
    EnvFilter,
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

/// Initialize logging with default configuration
pub fn init() -> Result<()> {
    let log_level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
    let json_logs = std::env::var("OPENACT_JSON_LOGS")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    init_with_config(&log_level, json_logs)
}

/// Initialize logging with custom configuration
pub fn init_with_config(log_level: &str, json_logs: bool) -> Result<()> {
    let env_filter = EnvFilter::from_str(log_level).unwrap_or_else(|_| EnvFilter::new("info"));

    let registry = tracing_subscriber::registry().with(env_filter);

    if json_logs {
        // JSON structured logging for production
        registry
            .with(
                fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_target(true)
                    .with_level(true)
                    .with_thread_ids(true)
                    .with_thread_names(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_span_events(FmtSpan::ENTER | FmtSpan::EXIT),
            )
            .init();
    } else {
        // Pretty console logging for development
        registry
            .with(
                fmt::layer()
                    .pretty()
                    .with_target(true)
                    .with_level(true)
                    .with_thread_ids(false)
                    .with_thread_names(false)
                    .with_file(false)
                    .with_line_number(false)
                    .with_span_events(FmtSpan::ENTER | FmtSpan::EXIT),
            )
            .init();
    }

    tracing::info!(
        log_level = %log_level,
        json_logs = %json_logs,
        "Logging initialized"
    );

    Ok(())
}

/// Macro for creating structured log entries with common fields
#[macro_export]
macro_rules! log_execution {
    ($level:ident, $message:expr, $($field:ident = $value:expr),*) => {
        tracing::$level!(
            $($field = $value,)*
            $message
        )
    };
}

/// Log a request start with timing
pub fn log_request_start(request_id: &str, method: &str, path: &str) -> std::time::Instant {
    let start = std::time::Instant::now();
    tracing::info!(
        request_id = %request_id,
        http_method = %method,
        http_path = %path,
        "Request started"
    );
    start
}

/// Log a request completion with timing
pub fn log_request_end(
    request_id: &str,
    method: &str,
    path: &str,
    status: u16,
    start: std::time::Instant,
) {
    let duration = start.elapsed();
    tracing::info!(
        request_id = %request_id,
        http_method = %method,
        http_path = %path,
        http_status = %status,
        duration_ms = %duration.as_millis(),
        "Request completed"
    );
}

/// Log task execution start
pub fn log_task_start(
    request_id: &str,
    task_trn: &str,
    connection_trn: &str,
) -> std::time::Instant {
    let start = std::time::Instant::now();
    tracing::info!(
        request_id = %request_id,
        task_trn = %task_trn,
        connection_trn = %connection_trn,
        "Task execution started"
    );
    start
}

/// Log task execution completion
pub fn log_task_end(
    request_id: &str,
    task_trn: &str,
    status: u16,
    start: std::time::Instant,
    retry_count: u32,
) {
    let duration = start.elapsed();
    tracing::info!(
        request_id = %request_id,
        task_trn = %task_trn,
        http_status = %status,
        duration_ms = %duration.as_millis(),
        retry_count = %retry_count,
        "Task execution completed"
    );
}

/// Log retry attempt
pub fn log_retry_attempt(
    request_id: &str,
    task_trn: &str,
    attempt: u32,
    max_retries: u32,
    delay_ms: u64,
    reason: &str,
) {
    tracing::warn!(
        request_id = %request_id,
        task_trn = %task_trn,
        retry_attempt = %attempt,
        max_retries = %max_retries,
        delay_ms = %delay_ms,
        reason = %reason,
        "Retrying request"
    );
}

/// Log error with context (with sanitization)
pub fn log_error(request_id: &str, error: &anyhow::Error, context: Option<&str>) {
    use crate::observability::sanitization::sanitize_error_message;

    let sanitized_error = sanitize_error_message(&error.to_string());

    match context {
        Some(ctx) => tracing::error!(
            request_id = %request_id,
            error = %sanitized_error,
            context = %ctx,
            "Operation failed"
        ),
        None => tracing::error!(
            request_id = %request_id,
            error = %sanitized_error,
            "Operation failed"
        ),
    }
}
