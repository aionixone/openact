use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Top-level configuration manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigManifest {
    /// Version of the config format
    pub version: String,
    /// Metadata about the configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, JsonValue>>,
    /// Connector configurations organized by connector type
    pub connectors: HashMap<String, ConnectorConfig>,
}

/// Configuration for a specific connector type (e.g., "http", "postgresql")
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    /// Connection definitions for this connector
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub connections: HashMap<String, ConnectionConfig>,
    /// Action definitions for this connector
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub actions: HashMap<String, ActionConfig>,
}

/// Configuration for a single connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Connector-specific configuration (e.g., URL, auth, timeouts)
    #[serde(flatten)]
    pub config: JsonValue,
    /// Metadata tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, JsonValue>>,
}

/// Configuration for a single action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionConfig {
    /// Reference to connection name within the same connector
    pub connection: String,
    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether to expose this action as an MCP tool
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_enabled: Option<bool>,
    /// Action-specific configuration (e.g., method, path, query params)
    #[serde(flatten)]
    pub config: JsonValue,
    /// Metadata tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, JsonValue>>,
}

impl Default for ConfigManifest {
    fn default() -> Self {
        Self {
            version: "v1".to_string(),
            metadata: None,
            connectors: HashMap::new(),
        }
    }
}

impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            connections: HashMap::new(),
            actions: HashMap::new(),
        }
    }
}
