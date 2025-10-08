use super::body_builder::RequestBodyType;
use super::connection::{
    HttpPolicy, MultiValue, NetworkConfig, ResponsePolicy, RetryPolicy, TimeoutConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// HTTP action configuration (maps to action.config_json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpAction {
    #[serde(default = "HttpAction::current_version")]
    pub config_version: u16,
    /// HTTP method
    pub method: String,

    /// API endpoint path (appended to connection's base_url)
    pub path: String,

    /// Override headers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, MultiValue>>,

    /// Override query parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_params: Option<HashMap<String, MultiValue>>,

    /// Request body (legacy JSON format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<serde_json::Value>,

    /// Typed request body with content type support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<RequestBodyType>,

    /// Override timeout configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_config: Option<TimeoutConfig>,

    /// Override network configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_config: Option<NetworkConfig>,

    /// Override HTTP policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_policy: Option<HttpPolicy>,

    /// Override response policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_policy: Option<ResponsePolicy>,

    /// Override retry policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
}

impl HttpAction {
    pub const fn current_version() -> u16 {
        1
    }

    /// Create a new HTTP action with required fields
    pub fn new(method: String, path: String) -> Self {
        Self {
            config_version: Self::current_version(),
            method,
            path,
            headers: None,
            query_params: None,
            request_body: None,
            body: None,
            timeout_config: None,
            network_config: None,
            http_policy: None,
            response_policy: None,
            retry_policy: None,
        }
    }
}
