use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use stepflow_dsl::WorkflowDSL;

/// openact DSL version
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Version {
    #[serde(rename = "1.0")]
    V1_0,
}

impl Default for Version {
    fn default() -> Self {
        Version::V1_0
    }
}

/// Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    /// Provider name (e.g., "google", "slack", "notion")
    pub name: String,
    /// Provider type (e.g., "oauth2", "api_key", "jwt")
    pub provider_type: String,
    /// Provider display name
    #[serde(default)]
    pub display_name: Option<String>,
    /// Provider description
    #[serde(default)]
    pub description: Option<String>,
    /// Provider configuration parameters
    #[serde(default)]
    pub config: HashMap<String, Value>,
    /// Supported authentication flows
    pub flows: HashMap<String, WorkflowDSL>,
    /// Provider-specific policy configuration
    #[serde(default)]
    pub policy: Option<ProviderPolicy>,
}

/// Provider policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPolicy {
    /// Default timeout (seconds)
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    /// Retry configuration
    #[serde(default)]
    pub retry: Option<RetryPolicy>,
    /// Rate limit configuration
    #[serde(default)]
    pub rate_limit: Option<RateLimitPolicy>,
    /// Security configuration
    #[serde(default)]
    pub security: Option<SecurityPolicy>,
}

fn default_timeout() -> u64 {
    30
}

/// Retry policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryPolicy {
    /// Maximum retry attempts
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
    /// Initial delay (milliseconds)
    #[serde(default = "default_initial_delay")]
    pub initial_delay_ms: u64,
    /// Maximum delay (milliseconds)
    #[serde(default = "default_max_delay")]
    pub max_delay_ms: u64,
    /// Backoff multiplier
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
    /// Retryable error types
    #[serde(default)]
    pub retryable_errors: Vec<String>,
}

fn default_max_attempts() -> u32 {
    3
}

fn default_initial_delay() -> u64 {
    1000
}

fn default_max_delay() -> u64 {
    30000
}

fn default_backoff_multiplier() -> f64 {
    2.0
}

/// Rate limit policy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitPolicy {
    /// Requests per second limit
    #[serde(default)]
    pub requests_per_second: Option<u32>,
    /// Requests per minute limit
    #[serde(default)]
    pub requests_per_minute: Option<u32>,
    /// Burst size limit
    #[serde(default)]
    pub burst_size: Option<u32>,
}

/// Security policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityPolicy {
    /// Allowed redirect domain whitelist
    #[serde(default)]
    pub allowed_redirect_domains: Vec<String>,
    /// Enable PKCE
    #[serde(default)]
    pub require_pkce: Option<bool>,
    /// Enable state parameter validation
    #[serde(default = "default_require_state")]
    pub require_state: bool,
    /// Log redaction paths
    #[serde(default)]
    pub redact_paths: Vec<String>,
    /// TLS configuration
    #[serde(default)]
    pub tls: Option<TlsPolicy>,
}

fn default_require_state() -> bool {
    true
}

/// TLS policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TlsPolicy {
    /// Verify certificates
    #[serde(default = "default_verify_certs")]
    pub verify_certificates: bool,
    /// Client certificate path
    #[serde(default)]
    pub client_cert_path: Option<String>,
    /// Client private key path
    #[serde(default)]
    pub client_key_path: Option<String>,
    /// CA certificate path
    #[serde(default)]
    pub ca_cert_path: Option<String>,
}

fn default_verify_certs() -> bool {
    true
}

/// Global configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalConfig {
    /// Default connection store configuration
    #[serde(default)]
    pub connection_store: Option<ConnectionStoreConfig>,
    /// Default run store configuration
    #[serde(default)]
    pub run_store: Option<RunStoreConfig>,
    /// Default secrets store configuration
    #[serde(default)]
    pub secrets_store: Option<SecretsStoreConfig>,
    /// Global policy configuration
    #[serde(default)]
    pub policy: Option<GlobalPolicy>,
    /// Callback server configuration
    #[serde(default)]
    pub callback_server: Option<CallbackServerConfig>,
}

/// Connection store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStoreConfig {
    /// Store type ("memory", "redis", "sqlite")
    #[serde(rename = "type")]
    pub store_type: String,
    /// Default TTL (seconds)
    #[serde(default)]
    pub default_ttl_seconds: Option<u64>,
    /// Store-specific configuration
    #[serde(default)]
    pub config: HashMap<String, Value>,
}

/// Run store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStoreConfig {
    /// Store type ("memory", "redis", "sqlite")
    #[serde(rename = "type")]
    pub store_type: String,
    /// Store-specific configuration
    #[serde(default)]
    pub config: HashMap<String, Value>,
}

/// Secrets store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretsStoreConfig {
    /// Store type ("memory", "vault", "env")
    #[serde(rename = "type")]
    pub store_type: String,
    /// Store-specific configuration
    #[serde(default)]
    pub config: HashMap<String, Value>,
}

/// Global policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalPolicy {
    /// Default timeout (seconds)
    #[serde(default = "default_timeout")]
    pub default_timeout_seconds: u64,
    /// Default retry policy
    #[serde(default)]
    pub default_retry: Option<RetryPolicy>,
    /// Global security policy
    #[serde(default)]
    pub security: Option<SecurityPolicy>,
    /// Logging configuration
    #[serde(default)]
    pub logging: Option<LoggingPolicy>,
    /// Metrics configuration
    #[serde(default)]
    pub metrics: Option<MetricsPolicy>,
}

/// Callback server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallbackServerConfig {
    /// Bind address
    #[serde(default = "default_callback_addr")]
    pub bind_address: String,
    /// Callback path
    #[serde(default = "default_callback_path")]
    pub callback_path: String,
    /// Timeout (seconds)
    #[serde(default = "default_callback_timeout")]
    pub timeout_seconds: u64,
    /// Enable health check endpoint
    #[serde(default)]
    pub enable_health_check: bool,
}

fn default_callback_addr() -> String {
    "127.0.0.1:8080".to_string()
}

fn default_callback_path() -> String {
    "/oauth/callback".to_string()
}

fn default_callback_timeout() -> u64 {
    300
}

/// Logging policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingPolicy {
    /// Log level
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Enable structured logging
    #[serde(default)]
    pub structured: bool,
    /// Log format
    #[serde(default)]
    pub format: Option<String>,
    /// Redact fields
    #[serde(default)]
    pub redact_fields: Vec<String>,
}

fn default_log_level() -> String {
    "info".to_string()
}

/// Metrics policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsPolicy {
    /// Enable metrics collection
    #[serde(default)]
    pub enabled: bool,
    /// Metrics prefix
    #[serde(default)]
    pub prefix: Option<String>,
    /// Metrics labels
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

/// openact DSL top-level structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenactDsl {
    /// DSL version
    #[serde(default)]
    pub version: Version,
    /// Configuration metadata
    #[serde(default)]
    pub metadata: Option<Metadata>,
    /// Provider definition
    pub provider: Provider,
    /// Global configuration
    #[serde(default)]
    pub global: Option<GlobalConfig>,
}

/// Configuration metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    /// Configuration name
    pub name: String,
    /// Configuration description
    #[serde(default)]
    pub description: Option<String>,
    /// Configuration version
    #[serde(default)]
    pub config_version: Option<String>,
    /// Author information
    #[serde(default)]
    pub author: Option<String>,
    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creation time
    #[serde(default)]
    pub created_at: Option<String>,
    /// Update time
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl OpenactDsl {
    /// Parse openact DSL from a YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml)
            .map_err(|e| anyhow!("Failed to parse openact DSL: {}", e))
    }

    /// Parse openact DSL from a JSON string
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| anyhow!("Failed to parse openact DSL: {}", e))
    }

    /// Convert to a YAML string
    pub fn to_yaml(&self) -> Result<String> {
        serde_yaml::to_string(self)
            .map_err(|e| anyhow!("Failed to serialize openact DSL to YAML: {}", e))
    }

    /// Convert to a JSON string
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| anyhow!("Failed to serialize openact DSL to JSON: {}", e))
    }

    /// Validate the DSL configuration
    pub fn validate(&self) -> Result<()> {
        // Validate provider name
        if self.provider.name.is_empty() {
            return Err(anyhow!("Provider name cannot be empty"));
        }

        // Validate provider type
        if self.provider.provider_type.is_empty() {
            return Err(anyhow!("Provider type cannot be empty"));
        }

        // Validate flow definitions
        if self.provider.flows.is_empty() {
            return Err(anyhow!("Provider must define at least one flow"));
        }

        // Validate each flow
        for (flow_name, flow_dsl) in &self.provider.flows {
            if flow_name.is_empty() {
                return Err(anyhow!("Flow name cannot be empty"));
            }
            
            // Validate flow DSL structure
            if flow_dsl.start_at.is_empty() {
                return Err(anyhow!("Flow '{}' must have a startAt state", flow_name));
            }
            
            if flow_dsl.states.is_empty() {
                return Err(anyhow!("Flow '{}' must define at least one state", flow_name));
            }
        }

        // Validate security configuration
        if let Some(policy) = &self.provider.policy {
            if let Some(security) = &policy.security {
                // Validate redirect domain format
                for domain in &security.allowed_redirect_domains {
                    if domain.is_empty() {
                        return Err(anyhow!("Redirect domain cannot be empty"));
                    }
                }
            }
        }

        Ok(())
    }

    /// Get a specific authentication flow
    pub fn get_flow(&self, flow_name: &str) -> Option<&WorkflowDSL> {
        self.provider.flows.get(flow_name)
    }

    /// List all available flow names
    pub fn list_flows(&self) -> Vec<&String> {
        self.provider.flows.keys().collect()
    }

    /// Get provider configuration parameter
    pub fn get_provider_config(&self, key: &str) -> Option<&Value> {
        self.provider.config.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_openact_dsl_creation() {
        let provider = Provider {
            name: "test_provider".to_string(),
            provider_type: "oauth2".to_string(),
            display_name: Some("Test Provider".to_string()),
            description: Some("A test provider".to_string()),
            config: HashMap::new(),
            flows: HashMap::new(),
            policy: None,
        };

        let dsl = OpenactDsl {
            version: Version::V1_0,
            metadata: None,
            provider,
            global: None,
        };

        assert_eq!(dsl.version, Version::V1_0);
        assert_eq!(dsl.provider.name, "test_provider");
        assert_eq!(dsl.provider.provider_type, "oauth2");
    }

    #[test]
    fn test_openact_dsl_validation() {
        // Test empty provider name
        let mut provider = Provider {
            name: "".to_string(),
            provider_type: "oauth2".to_string(),
            display_name: None,
            description: None,
            config: HashMap::new(),
            flows: HashMap::new(),
            policy: None,
        };

        let dsl = OpenactDsl {
            version: Version::V1_0,
            metadata: None,
            provider: provider.clone(),
            global: None,
        };

        assert!(dsl.validate().is_err());

        // Test valid configuration
        provider.name = "valid_provider".to_string();
        provider.flows.insert("obtain".to_string(), WorkflowDSL {
            comment: Some("Test flow".to_string()),
            version: Some("1.0".to_string()),
            start_at: "Start".to_string(),
            global_config: None,
            error_handling: None,
            states: {
                let mut states = HashMap::new();
                states.insert("Start".to_string(), stepflow_dsl::State::Succeed(
                    stepflow_dsl::SucceedState {
                        base: stepflow_dsl::BaseState {
                            comment: None,
                            retry: None,
                            catch: None,
                            next: None,
                            end: Some(true),
                        },
                        assign: None,
                        output: None,
                        parameters: None,
                    }
                ));
                states
            },
        });

        let valid_dsl = OpenactDsl {
            version: Version::V1_0,
            metadata: None,
            provider,
            global: None,
        };

        assert!(valid_dsl.validate().is_ok());
    }

    #[test]
    fn test_json_serialization() {
        let provider = Provider {
            name: "github".to_string(),
            provider_type: "oauth2".to_string(),
            display_name: Some("GitHub".to_string()),
            description: Some("GitHub OAuth2 Provider".to_string()),
            config: {
                let mut config = HashMap::new();
                config.insert("authorize_url".to_string(), json!("https://github.com/login/oauth/authorize"));
                config.insert("token_url".to_string(), json!("https://github.com/login/oauth/access_token"));
                config
            },
            flows: HashMap::new(),
            policy: Some(ProviderPolicy {
                timeout_seconds: 30,
                retry: Some(RetryPolicy {
                    max_attempts: 3,
                    initial_delay_ms: 1000,
                    max_delay_ms: 30000,
                    backoff_multiplier: 2.0,
                    retryable_errors: vec!["network_error".to_string(), "timeout".to_string()],
                }),
                rate_limit: None,
                security: Some(SecurityPolicy {
                    allowed_redirect_domains: vec!["localhost".to_string(), "example.com".to_string()],
                    require_pkce: Some(true),
                    require_state: true,
                    redact_paths: vec!["$.client_secret".to_string(), "$.access_token".to_string()],
                    tls: None,
                }),
            }),
        };

        let dsl = OpenactDsl {
            version: Version::V1_0,
            metadata: Some(Metadata {
                name: "GitHub OAuth2".to_string(),
                description: Some("GitHub OAuth2 authentication configuration".to_string()),
                config_version: Some("1.0.0".to_string()),
                author: Some("openact Team".to_string()),
                tags: vec!["oauth2".to_string(), "github".to_string()],
                created_at: Some("2024-01-01T00:00:00Z".to_string()),
                updated_at: Some("2024-01-01T00:00:00Z".to_string()),
            }),
            provider,
            global: Some(GlobalConfig {
                connection_store: Some(ConnectionStoreConfig {
                    store_type: "memory".to_string(),
                    default_ttl_seconds: Some(3600),
                    config: HashMap::new(),
                }),
                run_store: None,
                secrets_store: None,
                policy: None,
                callback_server: Some(CallbackServerConfig {
                    bind_address: "127.0.0.1:8080".to_string(),
                    callback_path: "/oauth/callback".to_string(),
                    timeout_seconds: 300,
                    enable_health_check: true,
                }),
            }),
        };

        // Test JSON serialization and deserialization
        let json_str = dsl.to_json().unwrap();
        let parsed_dsl = OpenactDsl::from_json(&json_str).unwrap();
        
        assert_eq!(parsed_dsl.provider.name, "github");
        assert_eq!(parsed_dsl.provider.provider_type, "oauth2");
        assert!(parsed_dsl.global.is_some());
        assert!(parsed_dsl.metadata.is_some());
    }
}
