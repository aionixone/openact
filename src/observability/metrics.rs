//! Metrics collection and export
//! 
//! Provides application metrics using the metrics crate with optional Prometheus export

use metrics::{counter, gauge, histogram, Counter, Gauge, Histogram};
use std::time::Duration;
use anyhow::Result;

/// Metrics registry for OpenAct
pub struct OpenActMetrics {
    // Request metrics
    pub http_requests_total: Counter,
    pub http_request_duration: Histogram,
    pub task_executions_total: Counter,
    pub task_execution_duration: Histogram,
    
    // Retry metrics
    pub retries_total: Counter,
    pub retry_delay_seconds: Histogram,
    
    // Connection metrics
    pub active_connections: Gauge,
    pub connection_pool_size: Gauge,
    pub connection_pool_hits: Counter,
    pub connection_pool_misses: Counter,
    
    // Storage metrics
    pub database_operations_total: Counter,
    pub database_operation_duration: Histogram,
    pub cache_operations_total: Counter,
    pub cache_hit_ratio: Gauge,
    
    // Error metrics
    pub errors_total: Counter,
    pub http_errors_by_status: Counter,
}

impl Default for OpenActMetrics {
    fn default() -> Self {
        Self {
            // Request metrics
            http_requests_total: counter!("openact_http_requests_total"),
            http_request_duration: histogram!("openact_http_request_duration_seconds"),
            task_executions_total: counter!("openact_task_executions_total"),
            task_execution_duration: histogram!("openact_task_execution_duration_seconds"),
            
            // Retry metrics
            retries_total: counter!("openact_retries_total"),
            retry_delay_seconds: histogram!("openact_retry_delay_seconds"),
            
            // Connection metrics
            active_connections: gauge!("openact_active_connections"),
            connection_pool_size: gauge!("openact_connection_pool_size"),
            connection_pool_hits: counter!("openact_connection_pool_hits_total"),
            connection_pool_misses: counter!("openact_connection_pool_misses_total"),
            
            // Storage metrics
            database_operations_total: counter!("openact_database_operations_total"),
            database_operation_duration: histogram!("openact_database_operation_duration_seconds"),
            cache_operations_total: counter!("openact_cache_operations_total"),
            cache_hit_ratio: gauge!("openact_cache_hit_ratio"),
            
            // Error metrics
            errors_total: counter!("openact_errors_total"),
            http_errors_by_status: counter!("openact_http_errors_by_status_total"),
        }
    }
}

use std::sync::LazyLock;

/// Global metrics instance
static METRICS: LazyLock<OpenActMetrics> = LazyLock::new(|| {
    // Initialize metrics recorder
    if std::env::var("OPENACT_METRICS_ENABLED").unwrap_or_else(|_| "true".to_string()) == "true" {
        let recorder = metrics::NoopRecorder;
        let _ = metrics::set_global_recorder(recorder);
    }
    
    OpenActMetrics::default()
});

/// Initialize metrics system
pub fn init() -> Result<()> {
    // Initialize by accessing the static
    let _ = &*METRICS;
    Ok(())
}

/// Get global metrics instance
pub fn get() -> &'static OpenActMetrics {
    &*METRICS
}

/// Record HTTP request metrics
pub fn record_http_request(_method: &str, _path: &str, status: u16, duration: Duration) {
    // Simplified metrics without labels for now
    // In a real implementation, we'd use a proper metrics backend
    counter!("openact_http_requests_total").increment(1);
    histogram!("openact_http_request_duration_seconds").record(duration.as_secs_f64());
    
    // Record errors separately
    if status >= 400 {
        counter!("openact_http_errors_by_status_total").increment(1);
    }
}

/// Record task execution metrics
pub fn record_task_execution(
    _task_trn: &str,
    _connection_trn: &str,
    _status: u16,
    duration: Duration,
    retry_count: u32,
) {
    counter!("openact_task_executions_total").increment(1);
    histogram!("openact_task_execution_duration_seconds").record(duration.as_secs_f64());
    
    // Record retries if any
    if retry_count > 0 {
        counter!("openact_retries_total").increment(retry_count as u64);
    }
}

/// Record retry attempt
pub fn record_retry_attempt(_task_trn: &str, _attempt: u32, delay: Duration, _reason: &str) {
    counter!("openact_retries_total").increment(1);
    histogram!("openact_retry_delay_seconds").record(delay.as_secs_f64());
}

/// Update connection pool metrics  
pub fn update_connection_pool_metrics(size: u64, hits: u64, misses: u64) {
    gauge!("openact_connection_pool_size").set(size as f64);
    
    // Record incremental hits/misses (simplified)
    counter!("openact_connection_pool_hits_total").increment(hits);
    counter!("openact_connection_pool_misses_total").increment(misses);
}

/// Record database operation
pub fn record_database_operation(_operation: &str, duration: Duration, _success: bool) {
    counter!("openact_database_operations_total").increment(1);
    histogram!("openact_database_operation_duration_seconds").record(duration.as_secs_f64());
}

/// Update cache metrics
pub fn update_cache_metrics(hit_ratio: f64, operations: u64) {
    gauge!("openact_cache_hit_ratio").set(hit_ratio);
    counter!("openact_cache_operations_total").increment(operations);
}

/// Record general error
pub fn record_error(_error_type: &str, _component: &str) {
    counter!("openact_errors_total").increment(1);
}

/// Get current metrics snapshot for debugging
pub fn get_metrics_snapshot() -> serde_json::Value {
    use serde_json::json;
    
    // Since we're using a NoopRecorder by default, we'll return static info
    // In a real implementation with Prometheus, we'd query the actual values
    json!({
        "metrics_enabled": std::env::var("OPENACT_METRICS_ENABLED").unwrap_or_else(|_| "true".to_string()),
        "recorder_type": "noop",
        "note": "Use Prometheus metrics feature for actual metric values"
    })
}
