//! Connection configuration models
//! 
//! This module contains data structures for managing API connection configurations,
//! including authentication parameters and network settings.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

use super::common::{HttpParameter, NetworkConfig, TimeoutConfig, HttpPolicy, RetryPolicy};

/// Authorization type for connections
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub enum AuthorizationType {
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
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ApiKeyAuthParameters {
    pub api_key_name: String,
    #[cfg_attr(feature = "openapi", schema(example = "***redacted***"))]
    pub api_key_value: String, // Will be encrypted when stored
}

impl std::fmt::Debug for ApiKeyAuthParameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKeyAuthParameters")
            .field("api_key_name", &self.api_key_name)
            .field("api_key_value", &"[REDACTED]")
            .finish()
    }
}

/// Basic authentication parameters
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct BasicAuthParameters {
    pub username: String,
    #[cfg_attr(feature = "openapi", schema(example = "***redacted***"))]
    pub password: String, // Will be encrypted when stored
}

impl std::fmt::Debug for BasicAuthParameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BasicAuthParameters")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

/// OAuth2 parameters
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct OAuth2Parameters {
    pub client_id: String,
    #[cfg_attr(feature = "openapi", schema(example = "***redacted***"))]
    pub client_secret: String, // Will be encrypted when stored
    pub token_url: String,
    pub scope: Option<String>,
    pub redirect_uri: Option<String>,
    pub use_pkce: Option<bool>,
}

impl std::fmt::Debug for OAuth2Parameters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuth2Parameters")
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("token_url", &self.token_url)
            .field("scope", &self.scope)
            .field("redirect_uri", &self.redirect_uri)
            .field("use_pkce", &self.use_pkce)
            .finish()
    }
}

/// Authentication parameters container
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(ToSchema))]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_ref: Option<String>,
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
            retry_policy: None,
            auth_ref: None,
            created_at: now,
            updated_at: now,
            version: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_debug_sanitization() {
        let api_key = ApiKeyAuthParameters {
            api_key_name: "X-API-Key".to_string(),
            api_key_value: "secret-api-key-123".to_string(),
        };
        let debug_output = format!("{:?}", api_key);
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("secret-api-key-123"));
    }

    #[test]
    fn test_basic_auth_debug_sanitization() {
        let basic_auth = BasicAuthParameters {
            username: "testuser".to_string(),
            password: "secret-password".to_string(),
        };
        let debug_output = format!("{:?}", basic_auth);
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("secret-password"));
        assert!(debug_output.contains("testuser")); // username should not be redacted
    }

    #[test]
    fn test_oauth2_debug_sanitization() {
        let oauth2 = OAuth2Parameters {
            client_id: "my-client-id".to_string(),
            client_secret: "secret-client-secret".to_string(),
            token_url: "https://oauth.example.com/token".to_string(),
            scope: Some("read write".to_string()),
            redirect_uri: None,
            use_pkce: Some(true),
        };
        let debug_output = format!("{:?}", oauth2);
        assert!(debug_output.contains("[REDACTED]"));
        assert!(!debug_output.contains("secret-client-secret"));
        assert!(debug_output.contains("my-client-id")); // client_id should not be redacted
    }
}
