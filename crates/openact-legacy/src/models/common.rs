//! Common types and utilities used across models

use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Multi-value parameter support
pub type MultiValue = Vec<String>;

/// HTTP parameter key-value pair
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct HttpParameter {
    pub key: String,
    pub value: String,
}

/// Timeout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TimeoutConfig {
    pub connect_ms: u64,
    pub read_ms: u64,
    pub total_ms: u64,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            connect_ms: 10_000, // 10 seconds
            read_ms: 30_000,    // 30 seconds
            total_ms: 60_000,   // 60 seconds
        }
    }
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TlsConfig {
    pub verify_peer: bool,
    pub ca_pem: Option<Vec<u8>>,
    pub client_cert_pem: Option<Vec<u8>>,
    pub client_key_pem: Option<Vec<u8>>,
    pub server_name: Option<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            verify_peer: true,
            ca_pem: None,
            client_cert_pem: None,
            client_key_pem: None,
            server_name: None,
        }
    }
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct NetworkConfig {
    pub proxy_url: Option<String>,
    pub tls: Option<TlsConfig>,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            proxy_url: None,
            tls: Some(TlsConfig::default()),
        }
    }
}

/// HTTP policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct HttpPolicy {
    pub denied_headers: Vec<String>,
    pub reserved_headers: Vec<String>,
    pub multi_value_append_headers: Vec<String>,
    pub drop_forbidden_headers: bool,
    pub normalize_header_names: bool,
    pub max_header_value_length: usize,
    pub max_total_headers: usize,
    pub allowed_content_types: Vec<String>,
}

impl Default for HttpPolicy {
    fn default() -> Self {
        Self {
            denied_headers: vec![
                "host".to_string(),
                "content-length".to_string(),
                "transfer-encoding".to_string(),
                "expect".to_string(),
                "connection".to_string(),
                "upgrade".to_string(),
                "proxy-authorization".to_string(),
            ],
            reserved_headers: vec!["authorization".to_string()],
            multi_value_append_headers: vec![
                "accept".to_string(),
                "accept-encoding".to_string(),
                "accept-language".to_string(),
                "cookie".to_string(),
                "set-cookie".to_string(),
                "cache-control".to_string(),
            ],
            drop_forbidden_headers: true,
            normalize_header_names: true,
            max_header_value_length: 8192, // 8KB per header value
            max_total_headers: 64,
            allowed_content_types: vec![], // Empty = allow all content types
        }
    }
}

/// Response policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ResponsePolicy {
    pub allow_binary: bool,
    pub max_body_bytes: usize,
}

impl Default for ResponsePolicy {
    fn default() -> Self {
        Self {
            allow_binary: true, // Allow binary responses by default for compatibility
            max_body_bytes: 8 * 1024 * 1024, // 8MB
        }
    }
}

/// Retry policy configuration for HTTP requests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct RetryPolicy {
    /// Maximum number of retries (excluding initial attempt)
    pub max_retries: u32,
    /// Base delay duration in milliseconds
    pub base_delay_ms: u64,
    /// Maximum delay duration in milliseconds
    pub max_delay_ms: u64,
    /// Backoff multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// HTTP status codes that should trigger a retry
    pub retry_status_codes: Vec<u16>,
    /// Whether to respect Retry-After headers
    pub respect_retry_after: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 0, // Default: no retries to maintain current behavior
            base_delay_ms: 100,
            max_delay_ms: 30_000, // 30 seconds
            backoff_multiplier: 2.0,
            retry_status_codes: vec![429, 500, 502, 503, 504], // Common retry-able status codes
            respect_retry_after: true,
        }
    }
}

impl RetryPolicy {
    /// Calculate delay for the nth retry attempt
    pub fn delay_for_attempt(&self, attempt: u32) -> std::time::Duration {
        if attempt == 0 {
            return std::time::Duration::ZERO;
        }

        let delay_ms =
            (self.base_delay_ms as f64 * self.backoff_multiplier.powi(attempt as i32 - 1)) as u64;
        let delay_ms = delay_ms.min(self.max_delay_ms);
        std::time::Duration::from_millis(delay_ms)
    }

    /// Check if a status code should trigger a retry
    pub fn should_retry_status(&self, status_code: u16) -> bool {
        self.retry_status_codes.contains(&status_code)
    }

    /// Create a more aggressive retry policy for testing
    pub fn aggressive() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 50,
            max_delay_ms: 5_000,
            backoff_multiplier: 1.5,
            retry_status_codes: vec![408, 429, 500, 502, 503, 504],
            respect_retry_after: true,
        }
    }

    /// Create a conservative retry policy
    pub fn conservative() -> Self {
        Self {
            max_retries: 1,
            base_delay_ms: 1000,
            max_delay_ms: 10_000,
            backoff_multiplier: 2.0,
            retry_status_codes: vec![429, 503, 504],
            respect_retry_after: true,
        }
    }
}
