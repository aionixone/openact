//! Timeout management for HTTP requests

use crate::error::{ConnectorError, ConnectorResult};
use crate::http::connection::TimeoutConfig;
use reqwest::ClientBuilder;
use std::time::Duration;
use tokio::time::timeout;

/// Timeout manager that handles different types of timeouts properly
#[derive(Debug, Clone)]
pub struct TimeoutManager {
    config: TimeoutConfig,
}

impl TimeoutManager {
    /// Create a new timeout manager with the given configuration
    pub fn new(config: TimeoutConfig) -> Self {
        Self { config }
    }

    /// Apply connection-level timeouts to a reqwest ClientBuilder
    /// This sets timeouts that apply to the underlying HTTP client
    pub fn apply_to_client_builder(&self, mut builder: ClientBuilder) -> ClientBuilder {
        // Set connection timeout - time to establish a connection
        builder = builder.connect_timeout(Duration::from_millis(self.config.connect_ms));
        
        // Do NOT set request timeout here - that should be per-request
        // The client should not have a total timeout as it's shared across requests
        
        builder
    }

    /// Get the appropriate timeout for a specific request
    /// This determines which timeout to use based on the configuration
    pub fn get_request_timeout(&self) -> Duration {
        // Priority: use read_ms if it's set and reasonable, otherwise total_ms
        if self.config.read_ms > 0 && self.config.read_ms <= self.config.total_ms {
            Duration::from_millis(self.config.read_ms)
        } else {
            Duration::from_millis(self.config.total_ms)
        }
    }

    /// Execute a request with proper timeout handling
    /// This applies request-level timeout with better error reporting
    pub async fn execute_with_timeout<F, T>(
        &self,
        operation: F,
    ) -> ConnectorResult<T>
    where
        F: std::future::Future<Output = Result<T, reqwest::Error>>,
    {
        let request_timeout = self.get_request_timeout();
        
        match timeout(request_timeout, operation).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(reqwest_error)) => {
                // Convert reqwest error to our error type
                if reqwest_error.is_timeout() {
                    Err(ConnectorError::Timeout(format!(
                        "Request timed out after {}ms (read timeout)",
                        request_timeout.as_millis()
                    )))
                } else if reqwest_error.is_connect() {
                    Err(ConnectorError::Connection(format!(
                        "Connection failed: {}",
                        reqwest_error
                    )))
                } else {
                    Err(ConnectorError::Http(reqwest_error))
                }
            }
            Err(_) => {
                // Tokio timeout elapsed
                Err(ConnectorError::Timeout(format!(
                    "Request timed out after {}ms (total timeout)",
                    request_timeout.as_millis()
                )))
            }
        }
    }

    /// Get timeout configuration for debugging/logging
    pub fn get_config(&self) -> &TimeoutConfig {
        &self.config
    }

    /// Validate timeout configuration for reasonableness
    pub fn validate(&self) -> ConnectorResult<()> {
        if self.config.connect_ms == 0 {
            return Err(ConnectorError::InvalidConfig(
                "Connect timeout must be greater than 0".to_string(),
            ));
        }

        if self.config.total_ms == 0 {
            return Err(ConnectorError::InvalidConfig(
                "Total timeout must be greater than 0".to_string(),
            ));
        }

        if self.config.connect_ms > self.config.total_ms {
            return Err(ConnectorError::InvalidConfig(
                "Connect timeout cannot be greater than total timeout".to_string(),
            ));
        }

        if self.config.read_ms > 0 && self.config.read_ms > self.config.total_ms {
            return Err(ConnectorError::InvalidConfig(
                "Read timeout cannot be greater than total timeout".to_string(),
            ));
        }

        // Warn about unreasonably long timeouts (more than 10 minutes)
        const MAX_REASONABLE_TIMEOUT: u64 = 10 * 60 * 1000; // 10 minutes
        if self.config.total_ms > MAX_REASONABLE_TIMEOUT {
            // This is a warning, not an error
            eprintln!(
                "Warning: Total timeout is very long ({}ms). Consider reducing it.",
                self.config.total_ms
            );
        }

        Ok(())
    }

    /// Create a timeout manager with sensible defaults based on use case
    pub fn for_api_calls() -> Self {
        Self::new(TimeoutConfig {
            connect_ms: 5_000,   // 5 seconds to connect
            read_ms: 30_000,     // 30 seconds to read response
            total_ms: 60_000,    // 1 minute total
        })
    }

    /// Create a timeout manager for quick health checks
    pub fn for_health_check() -> Self {
        Self::new(TimeoutConfig {
            connect_ms: 2_000,   // 2 seconds to connect
            read_ms: 3_000,      // 3 seconds to read
            total_ms: 5_000,     // 5 seconds total
        })
    }

    /// Create a timeout manager for long-running operations
    pub fn for_long_operations() -> Self {
        Self::new(TimeoutConfig {
            connect_ms: 10_000,  // 10 seconds to connect
            read_ms: 300_000,    // 5 minutes to read
            total_ms: 600_000,   // 10 minutes total
        })
    }
}

/// Add timeout error variant to our error types
impl ConnectorError {
    /// Create a timeout error with context
    pub fn timeout(message: String) -> Self {
        ConnectorError::Timeout(message)
    }
}

// We need to add the Timeout variant to ConnectorError
// This should be done in the error.rs file

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration as TokioDuration};

    #[tokio::test]
    async fn test_timeout_manager_validation() {
        // Valid configuration
        let valid_config = TimeoutConfig {
            connect_ms: 5000,
            read_ms: 10000,
            total_ms: 15000,
        };
        let manager = TimeoutManager::new(valid_config);
        assert!(manager.validate().is_ok());

        // Invalid: connect > total
        let invalid_config = TimeoutConfig {
            connect_ms: 20000,
            read_ms: 10000,
            total_ms: 15000,
        };
        let manager = TimeoutManager::new(invalid_config);
        assert!(manager.validate().is_err());

        // Invalid: read > total
        let invalid_config = TimeoutConfig {
            connect_ms: 5000,
            read_ms: 20000,
            total_ms: 15000,
        };
        let manager = TimeoutManager::new(invalid_config);
        assert!(manager.validate().is_err());
    }

    #[tokio::test]
    async fn test_timeout_execution() {
        let config = TimeoutConfig {
            connect_ms: 1000,
            read_ms: 100,   // Very short timeout
            total_ms: 200,
        };
        let manager = TimeoutManager::new(config);

        // Test successful operation within timeout
        let quick_operation = async {
            sleep(TokioDuration::from_millis(50)).await;
            Ok::<String, reqwest::Error>("success".to_string())
        };

        let result = manager.execute_with_timeout(quick_operation).await;
        assert!(result.is_ok());

        // Test operation that times out
        let slow_operation = async {
            sleep(TokioDuration::from_millis(300)).await;
            Ok::<String, reqwest::Error>("too slow".to_string())
        };

        let result = manager.execute_with_timeout(slow_operation).await;
        assert!(result.is_err());
        
        // Check that it's specifically a timeout error
        match result {
            Err(ConnectorError::Timeout(msg)) => {
                assert!(msg.contains("timed out"));
            }
            _ => panic!("Expected timeout error"),
        }
    }

    #[test]
    fn test_request_timeout_selection() {
        // Test read_ms priority when valid
        let config = TimeoutConfig {
            connect_ms: 5000,
            read_ms: 10000,
            total_ms: 15000,
        };
        let manager = TimeoutManager::new(config);
        assert_eq!(manager.get_request_timeout(), Duration::from_millis(10000));

        // Test total_ms fallback when read_ms is 0
        let config = TimeoutConfig {
            connect_ms: 5000,
            read_ms: 0,
            total_ms: 15000,
        };
        let manager = TimeoutManager::new(config);
        assert_eq!(manager.get_request_timeout(), Duration::from_millis(15000));

        // Test total_ms fallback when read_ms > total_ms
        let config = TimeoutConfig {
            connect_ms: 5000,
            read_ms: 20000,
            total_ms: 15000,
        };
        let manager = TimeoutManager::new(config);
        assert_eq!(manager.get_request_timeout(), Duration::from_millis(15000));
    }

    #[test]
    fn test_preset_configurations() {
        let api_manager = TimeoutManager::for_api_calls();
        assert_eq!(api_manager.config.connect_ms, 5000);
        assert_eq!(api_manager.config.read_ms, 30000);
        assert_eq!(api_manager.config.total_ms, 60000);

        let health_manager = TimeoutManager::for_health_check();
        assert_eq!(health_manager.config.connect_ms, 2000);
        assert_eq!(health_manager.config.read_ms, 3000);
        assert_eq!(health_manager.config.total_ms, 5000);

        let long_manager = TimeoutManager::for_long_operations();
        assert_eq!(long_manager.config.connect_ms, 10000);
        assert_eq!(long_manager.config.read_ms, 300000);
        assert_eq!(long_manager.config.total_ms, 600000);
    }
}
