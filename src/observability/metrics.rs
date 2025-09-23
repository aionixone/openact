//! Metrics collection and export
//! 
//! Provides application metrics using the metrics crate with optional Prometheus export
//! 
//! When the `metrics` feature is enabled, metrics are exported via Prometheus.
//! Otherwise, a no-op implementation is used for zero performance overhead.

use std::time::Duration;
use anyhow::Result;

// Conditional imports based on metrics feature
#[cfg(feature = "metrics")]
use metrics::{counter, gauge, histogram, Counter, Gauge, Histogram};

#[cfg(not(feature = "metrics"))]
use crate::observability::noop_metrics::{counter, gauge, histogram, Counter, Gauge, Histogram};

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
    // Initialize metrics recorder based on feature and environment
    #[cfg(feature = "metrics")]
    {
        if std::env::var("OPENACT_METRICS_ENABLED").unwrap_or_else(|_| "true".to_string()) == "true" {
            init_prometheus_recorder();
        }
    }
    
    OpenActMetrics::default()
});

#[cfg(feature = "metrics")]
fn init_prometheus_recorder() {
    use metrics_exporter_prometheus::PrometheusBuilder;
    use std::net::SocketAddr;
    
    let listen_addr = std::env::var("OPENACT_METRICS_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:9090".to_string());
    
    let socket_addr: SocketAddr = listen_addr.parse().unwrap_or_else(|_| {
        tracing::warn!("Invalid metrics address '{}', using default 0.0.0.0:9090", listen_addr);
        "0.0.0.0:9090".parse().unwrap()
    });
        
    match PrometheusBuilder::new()
        .with_http_listener(socket_addr)
        .build()
    {
        Ok((recorder, future)) => {
            if let Err(e) = metrics::set_global_recorder(recorder) {
                tracing::error!("Failed to set Prometheus metrics recorder: {}", e);
            } else {
                tracing::info!("Prometheus metrics enabled on {}", socket_addr);
                
                // Start the HTTP server in a background task
                tokio::spawn(async move {
                    future.await
                });
            }
        }
        Err(e) => {
            tracing::error!("Failed to create Prometheus metrics recorder: {}", e);
        }
    }
}

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
    
    let metrics_feature_enabled = cfg!(feature = "metrics");
    let metrics_env_enabled = std::env::var("OPENACT_METRICS_ENABLED")
        .unwrap_or_else(|_| "true".to_string()) == "true";
    
    let recorder_type = if metrics_feature_enabled && metrics_env_enabled {
        "prometheus"
    } else if metrics_feature_enabled {
        "prometheus_disabled_by_env"
    } else {
        "noop"
    };
    
    let listen_addr = if metrics_feature_enabled && metrics_env_enabled {
        Some(std::env::var("OPENACT_METRICS_ADDR").unwrap_or_else(|_| "0.0.0.0:9090".to_string()))
    } else {
        None
    };
    
    json!({
        "metrics_feature_enabled": metrics_feature_enabled,
        "metrics_env_enabled": metrics_env_enabled,
        "recorder_type": recorder_type,
        "listen_addr": listen_addr,
        "note": if metrics_feature_enabled {
            "Prometheus metrics available. Set OPENACT_METRICS_ENABLED=true and check /metrics endpoint."
        } else {
            "Compile with --features metrics to enable Prometheus export."
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[cfg(not(feature = "metrics"))]
    #[test]
    fn test_metrics_initialization() {
        // Test that metrics can be initialized without panicking (noop mode)
        let result = init();
        assert!(result.is_ok());
    }

    #[cfg(feature = "metrics")]
    #[tokio::test]
    async fn test_metrics_initialization() {
        // Test that metrics can be initialized without panicking (Prometheus mode)
        // Disable actual Prometheus for this test
        unsafe {
            std::env::set_var("OPENACT_METRICS_ENABLED", "false");
        }
        
        let result = init();
        assert!(result.is_ok());
        
        // Clean up
        unsafe {
            std::env::remove_var("OPENACT_METRICS_ENABLED");
        }
    }

    #[test]
    fn test_metrics_snapshot_without_feature() {
        let snapshot = get_metrics_snapshot();
        
        // When metrics feature is disabled, should use noop
        #[cfg(not(feature = "metrics"))]
        {
            assert_eq!(snapshot["metrics_feature_enabled"], false);
            assert_eq!(snapshot["recorder_type"], "noop");
        }
        
        // When metrics feature is enabled, depends on environment
        #[cfg(feature = "metrics")]
        {
            assert_eq!(snapshot["metrics_feature_enabled"], true);
            // recorder_type depends on OPENACT_METRICS_ENABLED
        }
    }

    #[test]
    fn test_record_functions() {
        // These should not panic in either mode
        record_http_request("GET", "/test", 200, Duration::from_millis(100));
        record_task_execution("test_trn", "conn_trn", 200, Duration::from_millis(50), 0);
        record_retry_attempt("test_trn", 1, Duration::from_millis(1000), "timeout");
        update_connection_pool_metrics(10, 5, 2);
        record_database_operation("SELECT", Duration::from_millis(10), true);
        update_cache_metrics(0.85, 100);
        record_error("validation_error", "api");
    }

    #[test]  
    fn test_metrics_struct_creation() {
        // Test that metrics struct can be created
        let metrics = OpenActMetrics::default();
        
        // Verify that all fields are present (compile-time check)
        let _ = &metrics.http_requests_total;
        let _ = &metrics.http_request_duration;
        let _ = &metrics.task_executions_total;
        let _ = &metrics.task_execution_duration;
        let _ = &metrics.retries_total;
        let _ = &metrics.retry_delay_seconds;
        let _ = &metrics.active_connections;
        let _ = &metrics.connection_pool_size;
        let _ = &metrics.connection_pool_hits;
        let _ = &metrics.connection_pool_misses;
        let _ = &metrics.database_operations_total;
        let _ = &metrics.database_operation_duration;
        let _ = &metrics.cache_operations_total;
        let _ = &metrics.cache_hit_ratio;
        let _ = &metrics.errors_total;
        let _ = &metrics.http_errors_by_status;
    }

    #[cfg(feature = "metrics")]
    #[test]
    fn test_prometheus_config() {
        // Test that Prometheus configuration works
        unsafe {
            std::env::set_var("OPENACT_METRICS_ADDR", "127.0.0.1:9091");
            std::env::set_var("OPENACT_METRICS_ENABLED", "true");
        }
        
        let snapshot = get_metrics_snapshot();
        assert_eq!(snapshot["metrics_feature_enabled"], true);
        assert_eq!(snapshot["metrics_env_enabled"], true);
        assert_eq!(snapshot["recorder_type"], "prometheus");
        assert_eq!(snapshot["listen_addr"], "127.0.0.1:9091");
        
        // Clean up
        unsafe {
            std::env::remove_var("OPENACT_METRICS_ADDR");
            std::env::remove_var("OPENACT_METRICS_ENABLED");
        }
    }

    #[cfg(not(feature = "metrics"))]
    #[test]
    fn test_noop_metrics_zero_overhead() {
        use crate::observability::noop_metrics::*;
        
        // Test that noop implementations exist and can be called
        let counter = counter("test_counter");
        counter.increment(5);
        counter.inc();
        
        let gauge = gauge("test_gauge");
        gauge.set(42.0);
        gauge.increment(1.0);
        gauge.decrement(0.5);
        
        let histogram = histogram("test_histogram");
        histogram.record(3.14);
        histogram.record_duration(Duration::from_millis(100));
        
        // All operations should be no-ops and not panic
    }
}
