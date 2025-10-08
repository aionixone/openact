use serde::{Deserialize, Serialize};

/// Authorization type for HTTP connections
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AuthorizationType {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "api_key")]
    ApiKey,
    #[serde(rename = "basic")]
    Basic,
    #[serde(rename = "oauth2_client_credentials")]
    OAuth2ClientCredentials,
    #[serde(rename = "oauth2_authorization_code")]
    OAuth2AuthorizationCode,
}

/// API Key authentication parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyAuthParameters {
    pub api_key_name: String,
    pub api_key_value: String, // Will be encrypted when stored
}

/// Basic authentication parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicAuthParameters {
    pub username: String,
    pub password: String, // Will be encrypted when stored
}

/// OAuth2 authentication parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Parameters {
    pub client_id: String,
    pub client_secret: String, // Will be encrypted when stored
    pub token_url: Option<String>,
    pub auth_url: Option<String>,
    pub scope: Option<String>,
    pub redirect_uri: Option<String>,
    pub use_pkce: Option<bool>,
}

/// Authentication parameters union
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthParameters {
    pub api_key_auth_parameters: Option<ApiKeyAuthParameters>,
    pub basic_auth_parameters: Option<BasicAuthParameters>,
    pub oauth_parameters: Option<OAuth2Parameters>,
}

/// HTTP parameter key-value pair
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpParameter {
    pub key: String,
    pub value: String,
}

/// Multi-value parameter support
pub type MultiValue = Vec<String>;

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
            connect_ms: 10_000, // 10 seconds
            read_ms: 30_000,    // 30 seconds
            total_ms: 60_000,   // 60 seconds
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
        Self { proxy_url: None, tls: Some(TlsConfig::default()) }
    }
}

/// HTTP policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            ],
            reserved_headers: vec!["authorization".to_string(), "user-agent".to_string()],
            multi_value_append_headers: vec!["accept".to_string(), "accept-encoding".to_string()],
            drop_forbidden_headers: true,
            normalize_header_names: true,
            max_header_value_length: 8192,
            max_total_headers: 100,
            allowed_content_types: vec![
                "application/json".to_string(),
                "application/xml".to_string(),
                "text/plain".to_string(),
                "text/html".to_string(),
                "application/x-www-form-urlencoded".to_string(),
                "multipart/form-data".to_string(),
            ],
        }
    }
}

/// Response policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponsePolicy {
    pub max_response_size: usize,
    pub allowed_status_codes: Vec<u16>,
    pub extract_json_path: Option<String>,
    pub transform_response: Option<String>,
}

impl Default for ResponsePolicy {
    fn default() -> Self {
        Self {
            max_response_size: 10 * 1024 * 1024, // 10MB
            allowed_status_codes: vec![200, 201, 202, 204],
            extract_json_path: None,
            transform_response: None,
        }
    }
}

/// Retry policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub retry_on_status_codes: Vec<u16>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_ms: 1000,
            max_delay_ms: 30000,
            backoff_multiplier: 2.0,
            retry_on_status_codes: vec![429, 500, 502, 503, 504],
        }
    }
}

/// HTTP invocation parameters for default values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvocationHttpParameters {
    #[serde(default)]
    pub header_parameters: Vec<HttpParameter>,
    #[serde(default)]
    pub query_string_parameters: Vec<HttpParameter>,
    #[serde(default)]
    pub body_parameters: Vec<HttpParameter>,
}

/// Complete HTTP connection configuration (maps to connection.config_json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpConnection {
    #[serde(default = "HttpConnection::current_version")]
    pub config_version: u16,
    /// Base URL for the API (new field for cleaner config)
    pub base_url: String,

    /// Authorization configuration
    #[serde(default = "default_authorization_type")]
    pub authorization: AuthorizationType,

    /// Authentication parameters (optional for None authorization type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_parameters: Option<AuthParameters>,

    /// Default HTTP parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocation_http_parameters: Option<InvocationHttpParameters>,

    /// Network configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_config: Option<NetworkConfig>,

    /// Timeout configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_config: Option<TimeoutConfig>,

    /// HTTP policy configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_policy: Option<HttpPolicy>,

    /// Retry policy configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,

    /// Reference to auth_connections table for OAuth tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_ref: Option<String>,
}

impl HttpConnection {
    pub const fn current_version() -> u16 {
        1
    }

    /// Create a new HTTP connection with defaults
    pub fn new(base_url: String, authorization: AuthorizationType) -> Self {
        let auth_params = if matches!(authorization, AuthorizationType::None) {
            None
        } else {
            Some(AuthParameters {
                api_key_auth_parameters: None,
                basic_auth_parameters: None,
                oauth_parameters: None,
            })
        };

        Self {
            config_version: Self::current_version(),
            base_url,
            authorization,
            auth_parameters: auth_params,
            invocation_http_parameters: None,
            network_config: None,
            timeout_config: None,
            http_policy: None,
            retry_policy: None,
            auth_ref: None,
        }
    }
}

impl Default for HttpConnection {
    fn default() -> Self {
        Self::new("http://localhost".to_string(), AuthorizationType::None)
    }
}

fn default_authorization_type() -> AuthorizationType {
    AuthorizationType::None
}
