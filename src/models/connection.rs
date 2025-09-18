//! Connection configuration models
//! 
//! This module contains data structures for managing API connection configurations,
//! including authentication parameters and network settings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::common::{HttpParameter, NetworkConfig, TimeoutConfig, HttpPolicy};

/// Authorization type for connections
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationType {
    ApiKey,
    Basic,
    OAuth2ClientCredentials,
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

/// OAuth2 parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Parameters {
    pub client_id: String,
    pub client_secret: String, // Will be encrypted when stored
    pub token_url: String,
    pub scope: Option<String>,
    pub redirect_uri: Option<String>,
    pub use_pkce: Option<bool>,
}

/// Authentication parameters container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_auth_parameters: Option<ApiKeyAuthParameters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basic_auth_parameters: Option<BasicAuthParameters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_parameters: Option<OAuth2Parameters>,
}

/// Invocation HTTP parameters (connection-level defaults)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InvocationHttpParameters {
    #[serde(default)]
    pub header_parameters: Vec<HttpParameter>,
    #[serde(default)]
    pub query_string_parameters: Vec<HttpParameter>,
    #[serde(default)]
    pub body_parameters: Vec<HttpParameter>,
}

/// Connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub trn: String,
    pub name: String,
    pub authorization_type: AuthorizationType,
    pub auth_parameters: AuthParameters,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocation_http_parameters: Option<InvocationHttpParameters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_config: Option<NetworkConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_config: Option<TimeoutConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_policy: Option<HttpPolicy>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

impl ConnectionConfig {
    /// Create a new connection with default values
    pub fn new(trn: String, name: String, authorization_type: AuthorizationType) -> Self {
        let now = Utc::now();
        Self {
            trn,
            name,
            authorization_type,
            auth_parameters: AuthParameters {
                api_key_auth_parameters: None,
                basic_auth_parameters: None,
                oauth_parameters: None,
            },
            invocation_http_parameters: None,
            network_config: None,
            timeout_config: None,
            http_policy: None,
            created_at: now,
            updated_at: now,
            version: 1,
        }
    }
}
