//! Advanced retry management with jitter, error classification, and total timeout

use crate::error::ConnectorError;
use crate::http::connection::RetryPolicy;
use rand::Rng;
use std::time::{Duration, Instant};

/// Classification of errors for retry decision making
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorClassification {
    /// Errors that should be retried (network issues, temporary server errors)
    Retryable,
    /// Errors that should not be retried (auth errors, client errors)
    NonRetryable,
    /// Rate limiting - should be retried with longer delays
    RateLimited,
}

/// Enhanced retry policy with better error handling and jitter
#[derive(Debug, Clone)]
pub struct RetryManager {
    policy: RetryPolicy,
    /// Maximum total time to spend on retries (including delays)
    max_total_duration: Duration,
    /// Whether to add jitter to delay calculations
    use_jitter: bool,
}

/// Retry attempt information
#[derive(Debug, Clone)]
pub struct RetryAttempt {
    pub attempt_number: u32,
    pub delay_before_attempt: Duration,
    pub total_elapsed: Duration,
    pub error_classification: ErrorClassification,
}

/// Result of retry decision making
#[derive(Debug, Clone)]
pub enum RetryDecision {
    /// Retry the operation after the specified delay
    Retry {
        delay: Duration,
        attempt_info: RetryAttempt,
    },
    /// Stop retrying and return the error
    Stop {
        reason: String,
        final_attempt: u32,
        total_elapsed: Duration,
    },
}

impl RetryManager {
    /// Create a new retry manager with the given policy
    pub fn new(policy: RetryPolicy) -> Self {
        Self {
            policy,
            max_total_duration: Duration::from_secs(300), // 5 minutes default
            use_jitter: true,
        }
    }

    /// Create retry manager with custom total timeout
    pub fn with_total_timeout(mut self, timeout: Duration) -> Self {
        self.max_total_duration = timeout;
        self
    }

    /// Enable or disable jitter in delay calculations
    pub fn with_jitter(mut self, use_jitter: bool) -> Self {
        self.use_jitter = use_jitter;
        self
    }

    /// Classify an error to determine if it should be retried
    pub fn classify_error(&self, error: &ConnectorError) -> ErrorClassification {
        match error {
            // Network and timeout errors are retryable
            ConnectorError::Timeout(_) => ErrorClassification::Retryable,
            ConnectorError::Connection(_) => ErrorClassification::Retryable,
            
            // HTTP errors depend on status code
            ConnectorError::Http(reqwest_error) => {
                if let Some(status) = reqwest_error.status() {
                    self.classify_status_code(status.as_u16())
                } else if reqwest_error.is_timeout() || reqwest_error.is_connect() {
                    ErrorClassification::Retryable
                } else {
                    ErrorClassification::NonRetryable
                }
            }
            
            // Authentication errors are not retryable
            ConnectorError::Authentication(_) => ErrorClassification::NonRetryable,
            
            // Configuration and validation errors are not retryable
            ConnectorError::InvalidConfig(_) => ErrorClassification::NonRetryable,
            ConnectorError::Validation(_) => ErrorClassification::NonRetryable,
            
            // Database errors might be retryable (depends on specific error)
            // Note: Database errors are handled in other connectors
            #[cfg(feature = "postgresql")]
            ConnectorError::Database(_) => ErrorClassification::Retryable,
            
            // Default to non-retryable for safety
            _ => ErrorClassification::NonRetryable,
        }
    }

    /// Classify a status code for retry decisions
    pub fn classify_status_code(&self, status_code: u16) -> ErrorClassification {
        match status_code {
            // 1xx, 2xx, 3xx - not errors, shouldn't reach here
            100..=399 => ErrorClassification::NonRetryable,
            
            // 4xx client errors - generally not retryable
            400 => ErrorClassification::NonRetryable, // Bad Request
            401 => ErrorClassification::NonRetryable, // Unauthorized
            403 => ErrorClassification::NonRetryable, // Forbidden
            404 => ErrorClassification::NonRetryable, // Not Found
            405 => ErrorClassification::NonRetryable, // Method Not Allowed
            406 => ErrorClassification::NonRetryable, // Not Acceptable
            407 => ErrorClassification::NonRetryable, // Proxy Authentication Required
            409 => ErrorClassification::NonRetryable, // Conflict
            410 => ErrorClassification::NonRetryable, // Gone
            411 => ErrorClassification::NonRetryable, // Length Required
            412 => ErrorClassification::NonRetryable, // Precondition Failed
            413 => ErrorClassification::NonRetryable, // Payload Too Large
            414 => ErrorClassification::NonRetryable, // URI Too Long
            415 => ErrorClassification::NonRetryable, // Unsupported Media Type
            416 => ErrorClassification::NonRetryable, // Range Not Satisfiable
            417 => ErrorClassification::NonRetryable, // Expectation Failed
            422 => ErrorClassification::NonRetryable, // Unprocessable Entity
            423 => ErrorClassification::NonRetryable, // Locked
            424 => ErrorClassification::NonRetryable, // Failed Dependency
            426 => ErrorClassification::NonRetryable, // Upgrade Required
            428 => ErrorClassification::NonRetryable, // Precondition Required
            431 => ErrorClassification::NonRetryable, // Request Header Fields Too Large
            451 => ErrorClassification::NonRetryable, // Unavailable For Legal Reasons
            
            // Special 4xx cases that might be retryable
            408 => ErrorClassification::Retryable, // Request Timeout
            429 => ErrorClassification::RateLimited, // Too Many Requests
            
            // 5xx server errors - generally retryable
            500 => ErrorClassification::Retryable, // Internal Server Error
            501 => ErrorClassification::NonRetryable, // Not Implemented
            502 => ErrorClassification::Retryable, // Bad Gateway
            503 => ErrorClassification::Retryable, // Service Unavailable
            504 => ErrorClassification::Retryable, // Gateway Timeout
            505 => ErrorClassification::NonRetryable, // HTTP Version Not Supported
            506 => ErrorClassification::NonRetryable, // Variant Also Negotiates
            507 => ErrorClassification::Retryable, // Insufficient Storage
            508 => ErrorClassification::NonRetryable, // Loop Detected
            510 => ErrorClassification::NonRetryable, // Not Extended
            511 => ErrorClassification::Retryable, // Network Authentication Required
            
            // Other 4xx/5xx - conservative approach
            400..=499 => ErrorClassification::NonRetryable,
            500..=599 => ErrorClassification::Retryable,
            
            // Unexpected status codes
            _ => ErrorClassification::NonRetryable,
        }
    }

    /// Decide whether to retry an operation
    pub fn should_retry(
        &self,
        error: &ConnectorError,
        attempt_number: u32,
        start_time: Instant,
    ) -> RetryDecision {
        let elapsed = start_time.elapsed();
        let classification = self.classify_error(error);

        // Check if we've exceeded maximum attempts
        if attempt_number >= self.policy.max_retries {
            return RetryDecision::Stop {
                reason: format!("Maximum retry attempts ({}) exceeded", self.policy.max_retries),
                final_attempt: attempt_number,
                total_elapsed: elapsed,
            };
        }

        // Check if we've exceeded total timeout
        if elapsed >= self.max_total_duration {
            return RetryDecision::Stop {
                reason: format!(
                    "Maximum total retry duration ({}ms) exceeded",
                    self.max_total_duration.as_millis()
                ),
                final_attempt: attempt_number,
                total_elapsed: elapsed,
            };
        }

        // Check error classification
        match classification {
            ErrorClassification::NonRetryable => {
                return RetryDecision::Stop {
                    reason: "Error is not retryable".to_string(),
                    final_attempt: attempt_number,
                    total_elapsed: elapsed,
                };
            }
            ErrorClassification::Retryable | ErrorClassification::RateLimited => {
                // Calculate delay
                let delay = self.calculate_delay(attempt_number, &classification);
                
                // Check if delay would exceed total timeout
                if elapsed + delay > self.max_total_duration {
                    return RetryDecision::Stop {
                        reason: format!(
                            "Next retry delay ({}ms) would exceed total timeout",
                            delay.as_millis()
                        ),
                        final_attempt: attempt_number,
                        total_elapsed: elapsed,
                    };
                }

                return RetryDecision::Retry {
                    delay,
                    attempt_info: RetryAttempt {
                        attempt_number: attempt_number + 1,
                        delay_before_attempt: delay,
                        total_elapsed: elapsed,
                        error_classification: classification,
                    },
                };
            }
        }
    }

    /// Calculate the delay before the next retry attempt
    pub(crate) fn calculate_delay(&self, attempt_number: u32, classification: &ErrorClassification) -> Duration {
        // Base delay calculation with exponential backoff
        let base_delay = self.policy.initial_delay_ms as f64
            * self.policy.backoff_multiplier.powi(attempt_number as i32);
        
        // Apply classification-specific adjustments
        let adjusted_delay = match classification {
            ErrorClassification::RateLimited => {
                // For rate limiting, use longer delays
                base_delay * 2.0
            }
            _ => base_delay,
        };

        // Cap at maximum delay
        let capped_delay = adjusted_delay.min(self.policy.max_delay_ms as f64);

        // Add jitter if enabled
        let final_delay = if self.use_jitter {
            self.add_jitter(capped_delay)
        } else {
            capped_delay
        };

        Duration::from_millis(final_delay as u64)
    }

    /// Add jitter to delay to prevent thundering herd
    fn add_jitter(&self, delay_ms: f64) -> f64 {
        let mut rng = rand::thread_rng();
        
        // Use full jitter: random between 0 and delay
        // This provides maximum distribution of retry attempts
        rng.gen::<f64>() * delay_ms
    }

    /// Get the current retry policy
    pub fn get_policy(&self) -> &RetryPolicy {
        &self.policy
    }

    /// Get the maximum total duration
    pub fn get_max_total_duration(&self) -> Duration {
        self.max_total_duration
    }

    /// Create a retry manager optimized for API calls
    pub fn for_api_calls() -> Self {
        let policy = RetryPolicy {
            max_retries: 3,
            initial_delay_ms: 1000,  // 1 second
            max_delay_ms: 16000,     // 16 seconds
            backoff_multiplier: 2.0,
            retry_on_status_codes: vec![408, 429, 500, 502, 503, 504, 511],
        };
        
        Self::new(policy)
            .with_total_timeout(Duration::from_secs(60)) // 1 minute total
            .with_jitter(true)
    }

    /// Create a retry manager for background jobs with more attempts
    pub fn for_background_jobs() -> Self {
        let policy = RetryPolicy {
            max_retries: 5,
            initial_delay_ms: 2000,  // 2 seconds
            max_delay_ms: 60000,     // 1 minute
            backoff_multiplier: 2.0,
            retry_on_status_codes: vec![408, 429, 500, 502, 503, 504, 511],
        };
        
        Self::new(policy)
            .with_total_timeout(Duration::from_secs(600)) // 10 minutes total
            .with_jitter(true)
    }

    /// Create a retry manager for health checks with minimal retries
    pub fn for_health_checks() -> Self {
        let policy = RetryPolicy {
            max_retries: 1,          // Only one retry
            initial_delay_ms: 500,   // 0.5 seconds
            max_delay_ms: 1000,      // 1 second
            backoff_multiplier: 1.0, // No exponential backoff
            retry_on_status_codes: vec![500, 502, 503, 504],
        };
        
        Self::new(policy)
            .with_total_timeout(Duration::from_secs(10)) // 10 seconds total
            .with_jitter(false) // No jitter for health checks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_error_classification() {
        let manager = RetryManager::for_api_calls();

        // Test status code classification
        assert_eq!(
            manager.classify_status_code(200),
            ErrorClassification::NonRetryable
        );
        assert_eq!(
            manager.classify_status_code(400),
            ErrorClassification::NonRetryable
        );
        assert_eq!(
            manager.classify_status_code(401),
            ErrorClassification::NonRetryable
        );
        assert_eq!(
            manager.classify_status_code(408),
            ErrorClassification::Retryable
        );
        assert_eq!(
            manager.classify_status_code(429),
            ErrorClassification::RateLimited
        );
        assert_eq!(
            manager.classify_status_code(500),
            ErrorClassification::Retryable
        );
        assert_eq!(
            manager.classify_status_code(502),
            ErrorClassification::Retryable
        );

        // Test error classification
        assert_eq!(
            manager.classify_error(&ConnectorError::Timeout("timeout".to_string())),
            ErrorClassification::Retryable
        );
        assert_eq!(
            manager.classify_error(&ConnectorError::Authentication("auth".to_string())),
            ErrorClassification::NonRetryable
        );
        assert_eq!(
            manager.classify_error(&ConnectorError::InvalidConfig("config".to_string())),
            ErrorClassification::NonRetryable
        );
    }

    #[test]
    fn test_retry_decision_max_attempts() {
        let manager = RetryManager::for_api_calls();
        let error = ConnectorError::Timeout("timeout".to_string());
        let start_time = Instant::now();

        // Should retry for first few attempts
        for attempt in 0..3 {
            let decision = manager.should_retry(&error, attempt, start_time);
            match decision {
                RetryDecision::Retry { .. } => {
                    // Expected for first 3 attempts
                }
                RetryDecision::Stop { .. } => {
                    panic!("Should not stop at attempt {}", attempt);
                }
            }
        }

        // Should stop after max attempts
        let decision = manager.should_retry(&error, 3, start_time);
        match decision {
            RetryDecision::Stop { reason, .. } => {
                assert!(reason.contains("Maximum retry attempts"));
            }
            RetryDecision::Retry { .. } => {
                panic!("Should stop after max attempts");
            }
        }
    }

    #[test]
    fn test_retry_decision_non_retryable() {
        let manager = RetryManager::for_api_calls();
        let error = ConnectorError::Authentication("unauthorized".to_string());
        let start_time = Instant::now();

        let decision = manager.should_retry(&error, 0, start_time);
        match decision {
            RetryDecision::Stop { reason, .. } => {
                assert!(reason.contains("not retryable"));
            }
            RetryDecision::Retry { .. } => {
                panic!("Should not retry non-retryable error");
            }
        }
    }

    #[test]
    fn test_delay_calculation() {
        let manager = RetryManager::for_api_calls().with_jitter(false);

        // Test exponential backoff
        let delay0 = manager.calculate_delay(0, &ErrorClassification::Retryable);
        let delay1 = manager.calculate_delay(1, &ErrorClassification::Retryable);
        let delay2 = manager.calculate_delay(2, &ErrorClassification::Retryable);

        assert_eq!(delay0, Duration::from_millis(1000)); // 1s * 2^0 = 1s
        assert_eq!(delay1, Duration::from_millis(2000)); // 1s * 2^1 = 2s
        assert_eq!(delay2, Duration::from_millis(4000)); // 1s * 2^2 = 4s

        // Test rate limit adjustment
        let rate_delay = manager.calculate_delay(0, &ErrorClassification::RateLimited);
        assert_eq!(rate_delay, Duration::from_millis(2000)); // 1s * 2 = 2s
    }

    #[test]
    fn test_jitter() {
        let manager = RetryManager::for_api_calls().with_jitter(true);
        
        // Run multiple times to ensure jitter is working
        let mut delays = Vec::new();
        for _ in 0..10 {
            let delay = manager.calculate_delay(0, &ErrorClassification::Retryable);
            delays.push(delay.as_millis());
        }

        // All delays should be different due to jitter
        let unique_delays: std::collections::HashSet<_> = delays.iter().collect();
        assert!(unique_delays.len() > 1, "Jitter should produce different delays");

        // All delays should be between 0 and initial_delay_ms
        for delay in delays {
            assert!(delay <= 1000, "Jittered delay should not exceed base delay");
        }
    }

    #[test]
    fn test_preset_configurations() {
        let api_manager = RetryManager::for_api_calls();
        assert_eq!(api_manager.policy.max_retries, 3);
        assert_eq!(api_manager.max_total_duration, Duration::from_secs(60));

        let bg_manager = RetryManager::for_background_jobs();
        assert_eq!(bg_manager.policy.max_retries, 5);
        assert_eq!(bg_manager.max_total_duration, Duration::from_secs(600));

        let health_manager = RetryManager::for_health_checks();
        assert_eq!(health_manager.policy.max_retries, 1);
        assert_eq!(health_manager.max_total_duration, Duration::from_secs(10));
        assert!(!health_manager.use_jitter);
    }
}
