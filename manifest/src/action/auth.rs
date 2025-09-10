// AuthFlow integration for Action execution
// Provides authentication context injection for API calls

use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::collections::HashMap;
use crate::utils::error::{OpenApiToolError, Result};

/// Authentication configuration parsed from x-auth extension
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Authentication type (oauth2, api_key, bearer, basic, none)
    pub auth_type: String,
    /// Provider name (github, google, slack, etc.)
    pub provider: String,
    /// Required scopes for OAuth2
    pub scopes: Vec<String>,
    /// Additional authentication parameters
    pub parameters: HashMap<String, Value>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            auth_type: "none".to_string(),
            provider: "default".to_string(),
            scopes: vec![],
            parameters: HashMap::new(),
        }
    }
}

impl AuthConfig {
    /// Parse authentication configuration from x-auth extension
    pub fn from_extension(extension_value: &Value) -> Result<Self> {
        if let Some(obj) = extension_value.as_object() {
            let auth_type = obj.get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("none")
                .to_string();
            
            let provider = obj.get("provider")
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();
            
            let scopes = obj.get("scopes")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|s| s.to_string())
                    .collect())
                .unwrap_or_default();
            
            let mut parameters = HashMap::new();
            for (key, value) in obj {
                if !["type", "provider", "scopes"].contains(&key.as_str()) {
                    parameters.insert(key.clone(), value.clone());
                }
            }
            
            Ok(Self {
                auth_type,
                provider,
                scopes,
                parameters,
            })
        } else {
            Ok(Self::default())
        }
    }
}

/// Authentication context containing tokens and headers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    /// Access token
    pub access_token: String,
    /// Token type (Bearer, Basic, etc.)
    pub token_type: String,
    /// Additional headers to inject
    pub headers: HashMap<String, String>,
    /// Provider information
    pub provider: String,
    /// Token expiration time (if available)
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl AuthContext {
    /// Create a new authentication context
    pub fn new(access_token: String, token_type: String, provider: String) -> Self {
        Self {
            access_token,
            token_type,
            headers: HashMap::new(),
            provider,
            expires_at: None,
        }
    }
    
    /// Add a custom header
    pub fn with_header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }
    
    /// Set token expiration
    pub fn with_expires_at(mut self, expires_at: chrono::DateTime<chrono::Utc>) -> Self {
        self.expires_at = Some(expires_at);
        self
    }
    
    /// Get the authorization header value
    pub fn get_auth_header(&self) -> String {
        format!("{} {}", self.token_type, self.access_token)
    }
    
    /// Check if the token is expired
    pub fn is_expired(&self) -> bool {
        self.expires_at.map(|exp| chrono::Utc::now() > exp).unwrap_or(false)
    }
}

/// Authentication adapter for integrating with AuthFlow
pub struct AuthAdapter {
    /// Tenant identifier
    #[allow(dead_code)]
    tenant: String,
    /// Mock connection store (will be replaced with real AuthFlow integration)
    mock_connections: HashMap<String, AuthContext>,
}

impl AuthAdapter {
    /// Create a new authentication adapter
    pub fn new(tenant: String) -> Self {
        let mut mock_connections = HashMap::new();
        
        // Add some mock connections for testing
        mock_connections.insert(
            "github".to_string(),
            AuthContext::new(
                "ghp_mock_token_12345".to_string(),
                "Bearer".to_string(),
                "github".to_string(),
            ).with_expires_at(chrono::Utc::now() + chrono::Duration::hours(1)),
        );
        
        mock_connections.insert(
            "google".to_string(),
            AuthContext::new(
                "ya29_mock_token_67890".to_string(),
                "Bearer".to_string(),
                "google".to_string(),
            ).with_expires_at(chrono::Utc::now() + chrono::Duration::hours(1)),
        );
        
        Self {
            tenant,
            mock_connections,
        }
    }
    
    /// Get authentication context for a provider
    pub async fn get_auth_context(&self, provider: &str) -> Result<AuthContext> {
        // TODO: Replace with real AuthFlow connection store query
        // For now, use mock data
        self.mock_connections
            .get(provider)
            .cloned()
            .ok_or_else(|| OpenApiToolError::ValidationError(
                format!("No authentication found for provider: {}", provider)
            ))
    }
    
    /// Get authentication context for an action
    pub async fn get_auth_for_action(&self, auth_config: &AuthConfig) -> Result<AuthContext> {
        match auth_config.auth_type.as_str() {
            "oauth2" | "bearer" => {
                let mut context = self.get_auth_context(&auth_config.provider).await?;
                
                // Add any additional headers from parameters
                for (key, value) in &auth_config.parameters {
                    if let Some(header_value) = value.as_str() {
                        context = context.with_header(key.clone(), header_value.to_string());
                    }
                }
                
                Ok(context)
            }
            "api_key" => {
                // Handle API key authentication
                let api_key = auth_config.parameters
                    .get("api_key")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| OpenApiToolError::ValidationError(
                        "API key not found in auth parameters".to_string()
                    ))?;
                
                let header_name = auth_config.parameters
                    .get("header_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("X-API-Key");
                
                Ok(AuthContext::new(
                    api_key.to_string(),
                    "ApiKey".to_string(),
                    auth_config.provider.clone(),
                ).with_header(header_name.to_string(), api_key.to_string()))
            }
            "basic" => {
                // Handle basic authentication
                let username = auth_config.parameters
                    .get("username")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| OpenApiToolError::ValidationError(
                        "Username not found in auth parameters".to_string()
                    ))?;
                
                let password = auth_config.parameters
                    .get("password")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| OpenApiToolError::ValidationError(
                        "Password not found in auth parameters".to_string()
                    ))?;
                
                // For now, use a simple base64 encoding (in production, use proper base64 crate)
                let credentials = format!("{}:{}", username, password);
                
                Ok(AuthContext::new(
                    credentials,
                    "Basic".to_string(),
                    auth_config.provider.clone(),
                ))
            }
            "none" => {
                // No authentication required
                Ok(AuthContext::new(
                    "".to_string(),
                    "None".to_string(),
                    "none".to_string(),
                ))
            }
            _ => Err(OpenApiToolError::ValidationError(
                format!("Unsupported authentication type: {}", auth_config.auth_type)
            )),
        }
    }
    
    /// Refresh authentication context if needed
    pub async fn refresh_auth_context(&self, context: &AuthContext) -> Result<AuthContext> {
        if context.is_expired() {
            // TODO: Implement token refresh logic with AuthFlow
            // For now, just return a new mock token
            Ok(AuthContext::new(
                format!("{}_refreshed", context.access_token),
                context.token_type.clone(),
                context.provider.clone(),
            ).with_expires_at(chrono::Utc::now() + chrono::Duration::hours(1)))
        } else {
            Ok(context.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_auth_config_from_extension() {
        let extension = json!({
            "type": "oauth2",
            "provider": "github",
            "scopes": ["user:email", "repo:read"]
        });
        
        let config = AuthConfig::from_extension(&extension).unwrap();
        assert_eq!(config.auth_type, "oauth2");
        assert_eq!(config.provider, "github");
        assert_eq!(config.scopes, vec!["user:email", "repo:read"]);
    }

    #[test]
    fn test_auth_config_default() {
        let config = AuthConfig::default();
        assert_eq!(config.auth_type, "none");
        assert_eq!(config.provider, "default");
        assert!(config.scopes.is_empty());
    }

    #[test]
    fn test_auth_context() {
        let context = AuthContext::new(
            "test_token".to_string(),
            "Bearer".to_string(),
            "github".to_string(),
        );
        
        assert_eq!(context.get_auth_header(), "Bearer test_token");
        assert!(!context.is_expired());
    }

    #[tokio::test]
    async fn test_auth_adapter() {
        let adapter = AuthAdapter::new("test_tenant".to_string());
        
        let context = adapter.get_auth_context("github").await.unwrap();
        assert_eq!(context.provider, "github");
        assert_eq!(context.token_type, "Bearer");
        
        let result = adapter.get_auth_context("unknown").await;
        assert!(result.is_err());
    }
}
