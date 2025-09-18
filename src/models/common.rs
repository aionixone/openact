//! Common types and utilities used across models

use serde::{Deserialize, Serialize};

/// Multi-value parameter support
pub type MultiValue = Vec<String>;

/// HTTP parameter key-value pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpParameter {
    pub key: String,
    pub value: String,
}

/// Timeout configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutConfig {
    pub connect_ms: u64,
    pub read_ms: u64,
    pub total_ms: u64,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            connect_ms: 10_000,  // 10 seconds
            read_ms: 30_000,     // 30 seconds
            total_ms: 60_000,    // 60 seconds
        }
    }
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct HttpPolicy {
    pub denied_headers: Vec<String>,
    pub reserved_headers: Vec<String>,
    pub multi_value_append_headers: Vec<String>,
    pub drop_forbidden_headers: bool,
}

impl Default for HttpPolicy {
    fn default() -> Self {
        Self {
            denied_headers: vec![
                "host".to_string(),
                "content-length".to_string(),
                "transfer-encoding".to_string(),
                "expect".to_string(),
            ],
            reserved_headers: vec!["authorization".to_string()],
            multi_value_append_headers: vec![
                "accept".to_string(),
                "cookie".to_string(),
                "set-cookie".to_string(),
            ],
            drop_forbidden_headers: true,
        }
    }
}

/// Response policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsePolicy {
    pub allow_binary: bool,
    pub max_body_bytes: usize,
}

impl Default for ResponsePolicy {
    fn default() -> Self {
        Self {
            allow_binary: false,
            max_body_bytes: 8 * 1024 * 1024, // 8MB
        }
    }
}
