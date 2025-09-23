//! Distributed tracing configuration and utilities
//! 
//! Provides tracing spans for tracking requests across service boundaries

use tracing::{Span, Level};
use uuid::Uuid;

/// Create a new root span for HTTP requests
pub fn create_request_span(method: &str, path: &str, request_id: Option<String>) -> Span {
    let request_id = request_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    
    tracing::span!(
        Level::INFO,
        "http_request",
        request_id = %request_id,
        http.method = %method,
        http.path = %path,
        otel.kind = "server"
    )
}

/// Create a span for task execution
pub fn create_task_execution_span(
    task_trn: &str,
    connection_trn: &str,
    request_id: &str,
) -> Span {
    tracing::span!(
        Level::INFO,
        "task_execution",
        request_id = %request_id,
        task_trn = %task_trn,
        connection_trn = %connection_trn,
        otel.kind = "internal"
    )
}

/// Create a span for HTTP client requests
pub fn create_http_client_span(
    method: &str,
    url: &str,
    request_id: &str,
    task_trn: &str,
) -> Span {
    tracing::span!(
        Level::INFO,
        "http_client_request",
        request_id = %request_id,
        task_trn = %task_trn,
        http.method = %method,
        http.url = %url,
        otel.kind = "client"
    )
}

/// Create a span for database operations
pub fn create_database_span(
    operation: &str,
    table: &str,
    request_id: &str,
) -> Span {
    tracing::span!(
        Level::DEBUG,
        "database_operation",
        request_id = %request_id,
        db.operation = %operation,
        db.table = %table,
        otel.kind = "client"
    )
}

/// Create a span for cache operations
pub fn create_cache_span(
    operation: &str,
    key: &str,
    request_id: &str,
) -> Span {
    tracing::span!(
        Level::DEBUG,
        "cache_operation",
        request_id = %request_id,
        cache.operation = %operation,
        cache.key = %key,
        otel.kind = "internal"
    )
}

/// Create a span for retry attempts
pub fn create_retry_span(
    attempt: u32,
    max_retries: u32,
    request_id: &str,
    task_trn: &str,
) -> Span {
    tracing::span!(
        Level::WARN,
        "retry_attempt",
        request_id = %request_id,
        task_trn = %task_trn,
        retry.attempt = %attempt,
        retry.max_retries = %max_retries,
        otel.kind = "internal"
    )
}

/// Add standard fields to current span
pub fn enrich_span_with_response(status: u16, duration_ms: u64) {
    let span = Span::current();
    span.record("http.status_code", status);
    span.record("duration_ms", duration_ms);
    
    if status >= 400 {
        span.record("error", true);
    }
}

/// Add error information to current span
pub fn enrich_span_with_error(error: &anyhow::Error) {
    let span = Span::current();
    span.record("error", true);
    span.record("error.message", &tracing::field::display(error));
    span.record("error.type", std::any::type_name::<anyhow::Error>());
}

/// Extract trace context from headers (for distributed tracing)
pub fn extract_trace_context(headers: &std::collections::HashMap<String, String>) -> Option<String> {
    // Look for common tracing headers
    headers.get("x-trace-id")
        .or_else(|| headers.get("traceparent"))
        .or_else(|| headers.get("x-request-id"))
        .cloned()
}

/// Inject trace context into headers (for distributed tracing)
pub fn inject_trace_context(
    headers: &mut std::collections::HashMap<String, String>,
    request_id: &str,
) {
    headers.insert("x-request-id".to_string(), request_id.to_string());
    headers.insert("x-trace-id".to_string(), request_id.to_string());
}

/// Macro for creating instrumented functions
#[macro_export]
macro_rules! instrument_function {
    ($level:ident, $name:expr, $($field:ident = $value:expr),*) => {
        tracing::instrument!(
            level = $level,
            name = $name,
            $(field($field = $value),)*
            skip_all
        )
    };
}
