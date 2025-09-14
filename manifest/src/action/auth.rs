// AuthFlow integration for Action execution
// Provides authentication context injection for API calls

use crate::utils::error::{OpenApiToolError, Result};
use authflow::store::{create_connection_store, ConnectionStore, StoreBackend, StoreConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Authentication configuration parsed from x-auth extension (spec compliant)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthConfig {
    pub connection_trn: String,
    #[serde(default)]
    pub scheme: Option<String>,
    pub injection: InjectionConfig,
    #[serde(default)]
    pub expiry: Option<ExpiryConfig>,
    #[serde(default)]
    pub refresh: Option<RefreshConfig>,
    #[serde(default)]
    pub failure: Option<FailureConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InjectionConfig {
    #[serde(default = "default_jsonada_type")]
    pub r#type: String,
    pub mapping: String,
}
fn default_jsonada_type() -> String {
    "jsonada".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpirySource {
    Field,
    Header,
    None,
}
impl Default for ExpirySource {
    fn default() -> Self {
        ExpirySource::Field
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpiryConfig {
    #[serde(default)]
    pub source: ExpirySource,
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub header: Option<String>,
    #[serde(default = "default_clock_skew_ms")]
    pub clock_skew_ms: u64,
    #[serde(default)]
    pub min_ttl_ms: u64,
}
fn default_clock_skew_ms() -> u64 {
    30_000
}
impl Default for ExpiryConfig {
    fn default() -> Self {
        Self {
            source: ExpirySource::Field,
            field: None,
            header: None,
            clock_skew_ms: 30_000,
            min_ttl_ms: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshWhen {
    Proactive,
    #[serde(alias = "on_401")]
    On401,
    #[serde(alias = "proactive_or_401")]
    ProactiveOr401,
}
impl Default for RefreshWhen {
    fn default() -> Self {
        RefreshWhen::ProactiveOr401
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefreshConfig {
    #[serde(default)]
    pub when: RefreshWhen,
    #[serde(default = "default_refresh_retries")]
    pub max_retries: u32,
    #[serde(default)]
    pub cooldown_ms: u64,
}
fn default_refresh_retries() -> u32 {
    1
}
impl Default for RefreshConfig {
    fn default() -> Self {
        Self {
            when: RefreshWhen::ProactiveOr401,
            max_retries: 1,
            cooldown_ms: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailureConfig {
    #[serde(default = "default_reauth_error_code")]
    pub reauth_error_code: String,
    #[serde(default = "default_bubble_provider_message")]
    pub bubble_provider_message: bool,
}
fn default_reauth_error_code() -> String {
    "E_AUTH".to_string()
}
fn default_bubble_provider_message() -> bool {
    true
}
impl Default for FailureConfig {
    fn default() -> Self {
        Self {
            reauth_error_code: default_reauth_error_code(),
            bubble_provider_message: true,
        }
    }
}

impl AuthConfig {
    /// Parse authentication configuration from x-auth extension (serde-based)
    pub fn from_extension(extension_value: &Value) -> Result<Self> {
        let cfg: AuthConfig = serde_json::from_value(extension_value.clone())
            .map_err(|e| OpenApiToolError::parse(format!("x-auth parse error: {}", e)))?;
        Ok(cfg)
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
        self.expires_at
            .map(|exp| chrono::Utc::now() > exp)
            .unwrap_or(false)
    }
}

/// Authentication adapter for integrating with AuthFlow
pub struct AuthAdapter {
    /// Tenant identifier
    #[allow(dead_code)]
    tenant: String,
    /// Mock connection store (will be replaced with real AuthFlow integration)
    #[allow(dead_code)]
    mock_connections: HashMap<String, AuthContext>,
    /// Real authflow connection store (optional)
    store: Option<Arc<dyn ConnectionStore>>,
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
            )
            .with_expires_at(chrono::Utc::now() + chrono::Duration::hours(1)),
        );

        mock_connections.insert(
            "google".to_string(),
            AuthContext::new(
                "ya29_mock_token_67890".to_string(),
                "Bearer".to_string(),
                "google".to_string(),
            )
            .with_expires_at(chrono::Utc::now() + chrono::Duration::hours(1)),
        );

        Self {
            tenant,
            mock_connections,
            store: None,
        }
    }

    /// Get authentication context by TRN (stub implementation)
    pub async fn get_auth_context_by_trn(&self, connection_trn: &str) -> Result<AuthContext> {
        if let Some(store) = &self.store {
            // Try read real connection from store
            if let Some(conn) = store
                .get(connection_trn)
                .await
                .map_err(|e| OpenApiToolError::database(e.to_string()))?
            {
                let mut ctx = AuthContext::new(
                    conn.access_token.clone(),
                    conn.token_type.clone(),
                    conn.trn.provider.clone(),
                );
                if let Some(exp) = conn.expires_at {
                    ctx = ctx.with_expires_at(exp);
                }
                return Ok(ctx);
            }
        }
        // Fallback: mock
        let provider = if connection_trn.contains("github") {
            "github"
        } else {
            "default"
        };
        let token = if provider == "github" {
            "ghp_mock_token_12345"
        } else {
            "mock_token"
        };
        Ok(AuthContext::new(
            token.to_string(),
            "Bearer".to_string(),
            provider.to_string(),
        )
        .with_expires_at(chrono::Utc::now() + chrono::Duration::hours(1)))
    }

    /// Get authentication context for an action (by TRN)
    pub async fn get_auth_for_action(&self, auth_config: &AuthConfig) -> Result<AuthContext> {
        self.get_auth_context_by_trn(&auth_config.connection_trn)
            .await
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
            )
            .with_expires_at(chrono::Utc::now() + chrono::Duration::hours(1)))
        } else {
            Ok(context.clone())
        }
    }

    /// Initialize real authflow store (memory for now; later can be sqlite)
    pub async fn init_store_memory(&mut self) -> Result<()> {
        let mut cfg = StoreConfig::default();
        cfg.backend = StoreBackend::Memory;
        let store = create_connection_store(cfg)
            .await
            .map_err(|e| OpenApiToolError::database(e.to_string()))?;
        self.store = Some(store);
        Ok(())
    }

    /// Initialize sqlite store with given database url
    pub async fn init_store_sqlite(
        &mut self,
        database_url: String,
        enable_encryption: bool,
    ) -> Result<()> {
        let sqlite_cfg = authflow::store::sqlite_connection_store::SqliteConfig {
            database_url,
            enable_encryption,
            ..Default::default()
        };
        let mut cfg = StoreConfig::default();
        cfg.backend = StoreBackend::Sqlite;
        cfg.sqlite = Some(sqlite_cfg);
        let store = create_connection_store(cfg)
            .await
            .map_err(|e| OpenApiToolError::database(e.to_string()))?;
        self.store = Some(store);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_auth_config_from_extension() {
        let extension = serde_json::json!({
            "connection_trn": "trn:authflow:tenant:connection/github",
            "injection": { "type": "jsonada", "mapping": "{% $access_token %}" }
        });
        let config = AuthConfig::from_extension(&extension).unwrap();
        assert_eq!(
            config.connection_trn,
            "trn:authflow:tenant:connection/github"
        );
        assert_eq!(config.injection.r#type, "jsonada");
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
        let context = adapter
            .get_auth_context_by_trn("trn:authflow:tenant:connection/github")
            .await
            .unwrap();
        assert_eq!(context.provider, "github");
        assert_eq!(context.token_type, "Bearer");
    }
}
