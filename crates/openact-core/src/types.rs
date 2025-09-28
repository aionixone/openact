use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Trn(pub String);

impl Trn {
    pub fn new(trn: impl Into<String>) -> Self {
        Self(trn.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse TRN components
    /// Format: trn:openact:{tenant}:{resource_type}/{name}@v{version}
    pub fn parse(&self) -> Option<TrnComponents> {
        let parts: Vec<&str> = self.0.split(':').collect();
        if parts.len() < 4 || parts[0] != "trn" || parts[1] != "openact" {
            return None;
        }

        let tenant = parts[2];
        let resource_part = parts[3];

        // Parse resource_type/name@version
        if let Some((resource_type_name, version_part)) = resource_part.split_once('@') {
            if let Some((resource_type, name)) = resource_type_name.split_once('/') {
                let version = version_part.strip_prefix('v')?.parse().ok()?;
                return Some(TrnComponents {
                    tenant: tenant.to_string(),
                    resource_type: resource_type.to_string(),
                    name: name.to_string(),
                    version,
                });
            }
        }
        None
    }

    /// Helper to parse connection TRN: trn:openact:{tenant}:connection/{connector}/{name}@v{version}
    pub fn parse_connection(&self) -> Option<ConnectionTrnComponents> {
        let components = self.parse()?;
        if components.resource_type.starts_with("connection/") {
            let connector = components.resource_type.strip_prefix("connection/")?;
            return Some(ConnectionTrnComponents {
                tenant: components.tenant,
                connector: connector.to_string(),
                name: components.name,
                version: components.version,
            });
        }
        None
    }

    /// Helper to parse action TRN: trn:openact:{tenant}:action/{connector}/{name}@v{version}
    pub fn parse_action(&self) -> Option<ActionTrnComponents> {
        let components = self.parse()?;
        if components.resource_type.starts_with("action/") {
            let connector = components.resource_type.strip_prefix("action/")?;
            return Some(ActionTrnComponents {
                tenant: components.tenant,
                connector: connector.to_string(),
                name: components.name,
                version: components.version,
            });
        }
        None
    }
}

impl fmt::Display for Trn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for Trn {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for Trn {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrnComponents {
    pub tenant: String,
    pub resource_type: String,
    pub name: String,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionTrnComponents {
    pub tenant: String,
    pub connector: String,
    pub name: String,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionTrnComponents {
    pub tenant: String,
    pub connector: String,
    pub name: String,
    pub version: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConnectorKind(pub String);

impl ConnectorKind {
    // Well-known connector types
    pub const HTTP: &'static str = "http";
    pub const POSTGRESQL: &'static str = "postgresql";
    pub const MYSQL: &'static str = "mysql";
    pub const REDIS: &'static str = "redis";
    pub const MONGODB: &'static str = "mongodb";
    pub const MCP: &'static str = "mcp";
    pub const GRPC: &'static str = "grpc";
    pub const SQLITE: &'static str = "sqlite";
    pub const CLICKHOUSE: &'static str = "clickhouse";
    pub const ELASTICSEARCH: &'static str = "elasticsearch";
    pub const KAFKA: &'static str = "kafka";
    pub const S3: &'static str = "s3";
    pub const VAULT: &'static str = "vault";

    pub fn new(kind: impl Into<String>) -> Self {
        Self(kind.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConnectorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ConnectorKind {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl AsRef<str> for ConnectorKind {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Connector metadata for REST API exposition and documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorMetadata {
    /// Connector type identifier (e.g., "http", "postgresql", "redis")
    pub kind: ConnectorKind,
    /// Human-readable display name
    pub display_name: String,
    /// Brief description of the connector's purpose
    pub description: String,
    /// Category for organization (e.g., "web", "database", "cache", "messaging")
    pub category: String,
    /// Supported operations/methods (e.g., ["GET", "POST"] for HTTP)
    pub supported_operations: Vec<String>,
    /// Whether this connector supports authentication
    pub supports_auth: bool,
    /// Example connection configuration (for documentation)
    pub example_config: Option<JsonValue>,
    /// Version of the connector implementation
    pub version: String,
}

/// Connection record storing connector-specific configuration
///
/// The `config_json` field contains connector-specific configuration (auth, network, etc.).
/// Schema validation is the responsibility of individual connector implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionRecord {
    pub trn: Trn,
    pub connector: ConnectorKind,
    pub name: String,
    /// Connector-specific configuration in JSON format
    /// Schema validation is handled by individual connector implementations
    pub config_json: JsonValue,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

/// Action record containing action-specific configuration
///
/// The `config_json` field contains action-specific configuration (endpoints, methods, etc.).
/// Schema validation is the responsibility of individual connector implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord {
    pub trn: Trn,
    pub connector: ConnectorKind,
    pub name: String,
    pub connection_trn: Trn,
    /// Action-specific configuration in JSON format (AUTHORITY SOURCE)
    /// Schema validation is handled by individual connector implementations
    pub config_json: JsonValue,
    /// Whether this action should be exposed as an MCP tool
    #[serde(default)]
    pub mcp_enabled: bool,
    /// Optional overrides for MCP manifest generation
    /// Used to customize tool name, description, etc. without changing execution config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_overrides: Option<McpOverrides>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

/// Optional overrides for customizing MCP manifest without changing execution config
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpOverrides {
    /// Custom tool name for MCP (defaults to action name)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Custom description for MCP clients
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Tags for organizing tools into categories
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether this tool requires client authorization
    #[serde(default)]
    pub requires_auth: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub run_id: String,
    pub paused_state: String,
    pub context_json: JsonValue,
    pub await_meta_json: Option<JsonValue>,
}

// ============= MCP Types =============

pub use mcp_types::*;

mod mcp_types {
    use super::*;
    use std::collections::HashMap;

    /// MCP Tool definition for a tool the MCP client can call
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct McpManifest {
        /// The name of the tool
        pub name: String,
        /// A human-readable description of the tool
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
        /// A JSON Schema object defining the expected parameters for the tool
        #[serde(rename = "inputSchema", skip_serializing_if = "Option::is_none")]
        pub input_schema: Option<McpToolsSchema>,
    }

    /// JSON Schema object for MCP tool parameters
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct McpToolsSchema {
        #[serde(rename = "type")]
        pub schema_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub properties: Option<HashMap<String, ParameterMcpManifest>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub required: Option<Vec<String>>,
    }

    /// MCP parameter manifest for individual tool parameters
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ParameterMcpManifest {
        #[serde(rename = "type")]
        pub param_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub items: Option<Box<ParameterMcpManifest>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub additional_properties: Option<JsonValue>,
    }

    /// Toolbox manifest for client SDKs (classic manifest format)
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolboxManifest {
        pub description: String,
        pub parameters: Vec<ParameterManifest>,
        #[serde(rename = "authRequired")]
        pub auth_required: Vec<String>,
    }

    /// Parameter manifest for toolbox (classic format)
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ParameterManifest {
        pub name: String,
        #[serde(rename = "type")]
        pub param_type: String,
        pub required: bool,
        pub description: String,
        #[serde(rename = "authSources")]
        pub auth_sources: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub items: Option<Box<ParameterManifest>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub additional_properties: Option<JsonValue>,
    }

    /// Toolset manifest containing multiple tools
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ToolsetManifest {
        pub server_version: String,
        pub tools: HashMap<String, ToolboxManifest>,
    }

    /// MCP toolset manifest containing multiple MCP tools
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct McpToolsetManifest {
        pub tools: Vec<McpManifest>,
    }

    /// Protocol version for MCP
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProtocolVersion {
        pub major: u32,
        pub minor: u32,
        pub patch: u32,
    }

    impl ProtocolVersion {
        /// Current MCP protocol version
        pub const CURRENT: ProtocolVersion = ProtocolVersion {
            major: 2024,
            minor: 11,
            patch: 5,
        };
    }

    impl Default for ProtocolVersion {
        fn default() -> Self {
            Self::CURRENT
        }
    }

    /// MCP server capabilities
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct McpServerCapabilities {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub logging: Option<JsonValue>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub prompts: Option<JsonValue>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub resources: Option<JsonValue>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub tools: Option<JsonValue>,
    }

    /// Complete MCP server manifest
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct McpServerManifest {
        pub protocol_version: ProtocolVersion,
        pub capabilities: McpServerCapabilities,
        pub server_info: McpServerInfo,
    }

    /// MCP server information
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct McpServerInfo {
        pub name: String,
        pub version: String,
    }
}

/// Authentication connection state, including OAuth tokens and metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthConnection {
    /// TRN identifier (trn:openact:{tenant}:auth/{provider}/{user_id})
    pub trn: String,
    /// Tenant ID
    pub tenant: String,
    /// Provider name (e.g., "github", "google")
    pub provider: String,
    /// User ID from the provider
    pub user_id: String,
    /// Access token
    pub access_token: String,
    /// Refresh token (optional)
    pub refresh_token: Option<String>,
    /// Token expiration time
    pub expires_at: Option<DateTime<Utc>>,
    /// Token type (usually "Bearer")
    pub token_type: String,
    /// Authorization scope
    pub scope: Option<String>,
    /// Additional metadata
    pub extra: JsonValue,
    /// Creation time
    pub created_at: DateTime<Utc>,
    /// Last update time
    pub updated_at: DateTime<Utc>,
    /// Version for optimistic locking
    pub version: i64,
}

impl AuthConnection {
    /// Create a new auth connection
    pub fn new(
        tenant: impl Into<String>,
        provider: impl Into<String>,
        user_id: impl Into<String>,
        access_token: impl Into<String>,
    ) -> Self {
        let tenant_str = tenant.into();
        let provider_str = provider.into();
        let user_id_str = user_id.into();
        let trn = format!("trn:openact:{tenant_str}:auth/{provider_str}/{user_id_str}");
        let now = Utc::now();

        Self {
            trn,
            tenant: tenant_str,
            provider: provider_str,
            user_id: user_id_str,
            access_token: access_token.into(),
            refresh_token: None,
            expires_at: None,
            token_type: "Bearer".to_string(),
            scope: None,
            extra: JsonValue::Null,
            created_at: now,
            updated_at: now,
            version: 1,
        }
    }

    /// Check if the token is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() >= expires_at
        } else {
            false
        }
    }

    /// Check if the token is about to expire (within given seconds)
    pub fn is_expiring_soon(&self, within_seconds: i64) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = Utc::now();
            let expiry_threshold = now + chrono::Duration::seconds(within_seconds);
            expires_at <= expiry_threshold
        } else {
            false
        }
    }

    /// Update the access token and expiration
    pub fn update_token(&mut self, access_token: String, expires_at: Option<DateTime<Utc>>) {
        self.access_token = access_token;
        self.expires_at = expires_at;
        self.updated_at = Utc::now();
    }

    /// Update the refresh token
    pub fn update_refresh_token(&mut self, refresh_token: Option<String>) {
        self.refresh_token = refresh_token;
        self.updated_at = Utc::now();
    }

    /// Get the authorization header value
    pub fn authorization_header_value(&self) -> String {
        format!("{} {}", self.token_type, self.access_token)
    }

    /// Update only the access token (alias for compatibility)
    pub fn update_access_token(&mut self, access_token: String) {
        self.access_token = access_token;
        self.updated_at = Utc::now();
    }
}

impl Default for AuthConnection {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            trn: "trn:openact:default:auth/unknown/unknown".to_string(),
            tenant: "default".to_string(),
            provider: "unknown".to_string(),
            user_id: "unknown".to_string(),
            access_token: String::new(),
            refresh_token: None,
            expires_at: None,
            token_type: "Bearer".to_string(),
            scope: None,
            extra: JsonValue::Null,
            created_at: now,
            updated_at: now,
            version: 1,
        }
    }
}
