use crate::error::{ConfigError, ConfigResult};
use crate::schema::{ActionConfig, ConfigManifest, ConnectionConfig};
use chrono::Utc;
use openact_core::{ActionRecord, ConnectionRecord, ConnectorKind, Trn};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Supported file formats for configuration
#[derive(Debug, Clone, PartialEq)]
pub enum FileFormat {
    Yaml,
    Json,
}

impl FileFormat {
    /// Detect file format from extension
    pub fn from_path<P: AsRef<Path>>(path: P) -> ConfigResult<Self> {
        let path = path.as_ref();
        match path.extension().and_then(|s| s.to_str()) {
            Some("yaml") | Some("yml") => Ok(FileFormat::Yaml),
            Some("json") => Ok(FileFormat::Json),
            Some(ext) => Err(ConfigError::UnsupportedFormat(ext.to_string())),
            None => Err(ConfigError::UnsupportedFormat("no extension".to_string())),
        }
    }
}

/// Configuration loader that can parse YAML/JSON files into database records
pub struct ConfigLoader {
    tenant: String,
}

impl ConfigLoader {
    /// Create a new config loader for a specific tenant
    pub fn new(tenant: impl Into<String>) -> Self {
        Self {
            tenant: tenant.into(),
        }
    }

    /// Load configuration from a file
    pub async fn load_from_file<P: AsRef<Path>>(&self, path: P) -> ConfigResult<ConfigManifest> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)?;
        let format = FileFormat::from_path(path)?;

        self.parse_content(&content, format).await
    }

    /// Parse configuration content directly
    pub async fn parse_content(
        &self,
        content: &str,
        format: FileFormat,
    ) -> ConfigResult<ConfigManifest> {
        // First, parse into a generic JSON value so we can detect flat vs legacy formats
        let root_json: JsonValue = match format {
            FileFormat::Yaml => serde_yaml::from_str(content)?,
            FileFormat::Json => serde_json::from_str(content)?,
        };

        // Reject legacy format with `connectors` to enforce the new flat format
        if root_json.get("connectors").is_some() {
            return Err(ConfigError::Validation(
                "Legacy format with 'connectors' is no longer supported. Use flat {connections, actions}.".to_string()
            ));
        }

        // Otherwise, try to normalize the flat {connections, actions} structure into legacy manifest
        let manifest = self.normalize_flat_manifest(root_json)?;
        self.validate_manifest(&manifest)?;
        Ok(manifest)
    }

    /// Convert manifest to database records
    pub async fn manifest_to_records(
        &self,
        manifest: &ConfigManifest,
    ) -> ConfigResult<(Vec<ConnectionRecord>, Vec<ActionRecord>)> {
        let mut connections = Vec::new();
        let mut actions = Vec::new();
        let now = Utc::now();

        for (connector_type, connector_config) in &manifest.connectors {
            let connector_kind = ConnectorKind::new(connector_type.clone());

            // Process connections
            for (connection_name, connection_config) in &connector_config.connections {
                let trn = self.build_connection_trn(connector_type, connection_name, 1);

                let record = ConnectionRecord {
                    trn,
                    connector: connector_kind.clone(),
                    name: connection_name.clone(),
                    config_json: self.build_connection_config_json(connection_config)?,
                    created_at: now,
                    updated_at: now,
                    version: 1,
                };
                connections.push(record);
            }

            // Process actions
            for (action_name, action_config) in &connector_config.actions {
                // Validate that referenced connection exists
                if !connector_config
                    .connections
                    .contains_key(&action_config.connection)
                {
                    return Err(ConfigError::Validation(format!(
                        "Action '{}' references non-existent connection '{}' in connector '{}'",
                        action_name, action_config.connection, connector_type
                    )));
                }

                let action_trn = self.build_action_trn(connector_type, action_name, 1);
                let connection_trn =
                    self.build_connection_trn(connector_type, &action_config.connection, 1);

                let record = ActionRecord {
                    trn: action_trn,
                    connector: connector_kind.clone(),
                    name: action_name.clone(),
                    connection_trn,
                    config_json: self.build_action_config_json(action_config)?,
                    mcp_enabled: action_config.mcp_enabled.unwrap_or(false),
                    mcp_overrides: None, // Will be populated from metadata if present
                    created_at: now,
                    updated_at: now,
                    version: 1,
                };
                actions.push(record);
            }
        }

        Ok((connections, actions))
    }

    /// Build TRN for a connection
    fn build_connection_trn(&self, connector_type: &str, name: &str, version: i64) -> Trn {
        Trn::new(format!(
            "trn:openact:{}:connection/{}/{}@v{}",
            self.tenant, connector_type, name, version
        ))
    }

    /// Build TRN for an action
    fn build_action_trn(&self, connector_type: &str, name: &str, version: i64) -> Trn {
        Trn::new(format!(
            "trn:openact:{}:action/{}/{}@v{}",
            self.tenant, connector_type, name, version
        ))
    }

    /// Build config JSON for connection from the flattened config
    fn build_connection_config_json(&self, config: &ConnectionConfig) -> ConfigResult<JsonValue> {
        let mut json = config.config.clone();

        // Add metadata if present
        if let Some(metadata) = &config.metadata {
            if let JsonValue::Object(ref mut map) = json {
                map.insert(
                    "_metadata".to_string(),
                    JsonValue::Object(
                        metadata
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect(),
                    ),
                );
            }
        }

        Ok(json)
    }

    /// Build config JSON for action from the flattened config
    fn build_action_config_json(&self, config: &ActionConfig) -> ConfigResult<JsonValue> {
        let mut json = config.config.clone();

        // Add metadata if present
        if let Some(metadata) = &config.metadata {
            if let JsonValue::Object(ref mut map) = json {
                map.insert(
                    "_metadata".to_string(),
                    JsonValue::Object(
                        metadata
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect(),
                    ),
                );
            }
        }

        Ok(json)
    }

    /// Validate the loaded manifest
    fn validate_manifest(&self, manifest: &ConfigManifest) -> ConfigResult<()> {
        // Basic validation
        if manifest.connectors.is_empty() {
            return Err(ConfigError::Validation("No connectors defined".to_string()));
        }

        // Validate connector names
        for connector_type in manifest.connectors.keys() {
            if connector_type.is_empty() {
                return Err(ConfigError::Validation("Empty connector type".to_string()));
            }
        }

        Ok(())
    }

    /// Normalize flat format { version?, metadata?, connections, actions } into legacy manifest
    fn normalize_flat_manifest(&self, root: JsonValue) -> ConfigResult<ConfigManifest> {
        use crate::schema::{ActionConfig, ConnectionConfig, ConnectorConfig};
        let mut connectors: HashMap<String, ConnectorConfig> = HashMap::new();

        let version = root
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("1.0")
            .to_string();
        let metadata = root.get("metadata").cloned().and_then(|m| match m {
            JsonValue::Object(map) => Some(map.into_iter().collect()),
            _ => None,
        });

        // Helper function to get or create connector entry (avoid closure borrowing issues)

        // Index connections by name → kind
        let mut connection_kind_by_name: HashMap<String, String> = HashMap::new();

        // Normalize connections
        if let Some(JsonValue::Object(conns)) = root.get("connections") {
            for (conn_name, conn_val) in conns {
                let conn_obj = conn_val.as_object().ok_or_else(|| {
                    ConfigError::Validation(format!("Connection '{}' must be an object", conn_name))
                })?;

                let kind = conn_obj
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ConfigError::Validation(format!(
                            "Connection '{}' missing required field 'kind'",
                            conn_name
                        ))
                    })?;
                // Canonicalize connector kind
                let kind_canon = openact_core::ConnectorKind::new(kind.to_string()).canonical();
                connection_kind_by_name.insert(conn_name.clone(), kind_canon.as_str().to_string());

                // Build connection config by excluding only outer keys; keep all others (connector-agnostic)
                let mut config_map = serde_json::Map::new();
                for (k, v) in conn_obj {
                    if k == "kind" || k == "description" { continue; }
                    config_map.insert(k.clone(), v.clone());
                }

                let connection = ConnectionConfig {
                    description: conn_obj
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    config: JsonValue::Object(config_map),
                    metadata: None,
                };

                let connector = connectors
                    .entry(kind_canon.as_str().to_string())
                    .or_insert_with(ConnectorConfig::default);
                connector.connections.insert(conn_name.clone(), connection);
            }
        } else {
            return Err(ConfigError::Validation(
                "Flat format requires 'connections' object".to_string(),
            ));
        }

        // Normalize actions
        if let Some(JsonValue::Object(acts)) = root.get("actions") {
            for (act_name, act_val) in acts {
                let act_obj = act_val.as_object().ok_or_else(|| {
                    ConfigError::Validation(format!("Action '{}' must be an object", act_name))
                })?;

                let connection_name = act_obj
                    .get("connection")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ConfigError::Validation(format!(
                            "Action '{}' missing required field 'connection'",
                            act_name
                        ))
                    })?;

                // Determine action kind: explicit or inherit from connection
                let action_kind = act_obj.get("kind").and_then(|v| v.as_str()).map(|s| s.to_string())
                    .or_else(|| connection_kind_by_name.get(connection_name).cloned())
                    .ok_or_else(|| ConfigError::Validation(format!(
                        "Action '{}' cannot determine kind (no explicit kind and connection not found)", act_name
                    )))?;
                let action_kind_canon = openact_core::ConnectorKind::new(action_kind.clone()).canonical();

                // Build action.config (prefer 'config' object; support 'statement' sugar)
                let mut config_json = if let Some(cfg) = act_obj.get("config") {
                    cfg.clone()
                } else if let Some(stmt) = act_obj.get("statement") {
                    let mut m = serde_json::Map::new();
                    m.insert("statement".to_string(), stmt.clone());
                    JsonValue::Object(m)
                } else {
                    // Connector-agnostic: include all non-metadata fields into config_json
                    let mut config_map = serde_json::Map::new();
                    for (key, value) in act_obj {
                        if !matches!(key.as_str(), "connection" | "description" | "mcp_enabled" | "mcp") {
                            config_map.insert(key.clone(), value.clone());
                        }
                    }
                    JsonValue::Object(config_map)
                };

                if let Some(parameters) = act_obj.get("parameters") {
                    if let JsonValue::Object(ref mut map) = config_json {
                        map.insert("parameters".to_string(), parameters.clone());
                    }
                }

                // Build metadata: mcp_overrides and parameters if present
                let mut metadata_map: HashMap<String, JsonValue> = HashMap::new();

                if let Some(JsonValue::Object(mcp)) = act_obj.get("mcp") {
                    // enabled handled separately via mcp_enabled
                    let mut overrides = serde_json::Map::new();
                    if let Some(v) = mcp.get("tool_name") {
                        overrides.insert("tool_name".to_string(), v.clone());
                    }
                    if let Some(v) = mcp.get("description") {
                        overrides.insert("description".to_string(), v.clone());
                    }
                    if !overrides.is_empty() {
                        metadata_map
                            .insert("mcp_overrides".to_string(), JsonValue::Object(overrides));
                    }
                }

                if let Some(params) = act_obj.get("parameters") {
                    metadata_map.insert("parameters".to_string(), params.clone());
                }

                let action = ActionConfig {
                    connection: connection_name.to_string(),
                    description: act_obj
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    mcp_enabled: act_obj
                        .get("mcp")
                        .and_then(|v| v.get("enabled"))
                        .and_then(|v| v.as_bool()),
                    config: config_json,
                    metadata: if metadata_map.is_empty() {
                        None
                    } else {
                        Some(metadata_map)
                    },
                };

                let connector = connectors
                    .entry(action_kind_canon.as_str().to_string())
                    .or_insert_with(ConnectorConfig::default);
                connector.actions.insert(act_name.clone(), action);
            }
        }

        Ok(ConfigManifest {
            version,
            metadata,
            connectors,
        })
    }

    /// Export database records back to manifest format
    pub async fn records_to_manifest(
        &self,
        connections: Vec<ConnectionRecord>,
        actions: Vec<ActionRecord>,
    ) -> ConfigResult<ConfigManifest> {
        let mut connectors: HashMap<String, crate::schema::ConnectorConfig> = HashMap::new();

        // Group connections by connector type
        for conn in connections {
            let connector_type = conn.connector.as_str().to_string();
            let connector_config = connectors.entry(connector_type).or_default();

            let connection_config = ConnectionConfig {
                description: None, // TODO: Extract from metadata if present
                config: conn.config_json,
                metadata: None, // TODO: Extract _metadata if present
            };

            connector_config
                .connections
                .insert(conn.name, connection_config);
        }

        // Group actions by connector type
        for action in actions {
            let connector_type = action.connector.as_str().to_string();
            let connector_config = connectors.entry(connector_type).or_default();

            // Extract connection name from TRN
            let connection_name = if let Some(components) = action.connection_trn.parse_connection()
            {
                components.name
            } else {
                return Err(ConfigError::InvalidTrn(format!(
                    "Invalid connection TRN: {}",
                    action.connection_trn
                )));
            };

            let action_config = ActionConfig {
                connection: connection_name,
                description: None, // TODO: Extract from metadata if present
                mcp_enabled: Some(action.mcp_enabled),
                config: action.config_json,
                metadata: None, // TODO: Extract _metadata if present
            };

            connector_config.actions.insert(action.name, action_config);
        }

        Ok(ConfigManifest {
            version: "v1".to_string(),
            metadata: None,
            connectors,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ActionConfig, ConfigManifest, ConnectionConfig, ConnectorConfig};
    use serde_json::json;

    #[tokio::test]
    async fn test_flat_format_parsing() {
        let flat_yaml = r#"
version: "1.0"
metadata:
  author: "test"

connections:
  test-api:
    kind: http
    base_url: "https://httpbin.org"
    authorization: "api_key"
    auth_parameters:
      api_key_auth_parameters:
        api_key_name: "Authorization"
        api_key_value: "Bearer test-token"

actions:
  httpbin.get:
    connection: test-api
    description: "GET /get"
    parameters:
      - name: query
        type: object
        required: false
    config:
      method: "GET"
      path: "/get"
    mcp:
      enabled: true
      tool_name: "httpbin.get"
"#;

        let loader = ConfigLoader::new("default");
        let manifest = loader
            .parse_content(flat_yaml, FileFormat::Yaml)
            .await
            .unwrap();

        // Verify top-level structure
        assert_eq!(manifest.version, "1.0");
        assert!(manifest.metadata.is_some());
        assert_eq!(
            manifest.metadata.as_ref().unwrap().get("author").unwrap(),
            "test"
        );

        // Verify connections were normalized into connectors
        assert!(manifest.connectors.contains_key("http"));
        let http_connector = &manifest.connectors["http"];
        assert!(http_connector.connections.contains_key("test-api"));

        let connection = &http_connector.connections["test-api"];
        assert!(connection.config.get("base_url").is_some());
        assert_eq!(connection.config["base_url"], json!("https://httpbin.org"));

        // Verify actions were normalized
        assert!(http_connector.actions.contains_key("httpbin.get"));
        let action = &http_connector.actions["httpbin.get"];
        assert_eq!(action.connection, "test-api");
        assert_eq!(action.description.as_ref().unwrap(), "GET /get");
        assert_eq!(action.mcp_enabled, Some(true));

        // Verify parameters were stored in metadata
        assert!(action.metadata.is_some());
        let metadata = action.metadata.as_ref().unwrap();
        assert!(metadata.contains_key("parameters"));

        // Verify mcp_overrides were extracted
        assert!(metadata.contains_key("mcp_overrides"));
        let mcp_overrides = &metadata["mcp_overrides"];
        assert_eq!(mcp_overrides["tool_name"], json!("httpbin.get"));
    }

    #[tokio::test]
    async fn test_kind_inheritance() {
        let flat_yaml = r#"
version: "1.0"
connections:
  test-api:
    kind: http
    base_url: "https://example.com"

actions:
  test-action:
    connection: test-api
    config:
      method: "GET"
      path: "/test"
"#;

        let loader = ConfigLoader::new("default");
        let manifest = loader
            .parse_content(flat_yaml, FileFormat::Yaml)
            .await
            .unwrap();

        // Action should be placed in http connector due to kind inheritance
        assert!(manifest.connectors.contains_key("http"));
        let http_connector = &manifest.connectors["http"];
        assert!(http_connector.actions.contains_key("test-action"));
    }

    #[tokio::test]
    async fn test_statement_sugar() {
        let flat_yaml = r#"
version: "1.0"
connections:
  test-db:
    kind: postgres
    host: "localhost"

actions:
  test-query:
    connection: test-db
    statement: "SELECT * FROM users WHERE id = $1"
"#;

        let loader = ConfigLoader::new("default");
        let manifest = loader
            .parse_content(flat_yaml, FileFormat::Yaml)
            .await
            .unwrap();

        let postgres_connector = &manifest.connectors["postgres"];
        let action = &postgres_connector.actions["test-query"];

        // Statement should be normalized into config
        assert!(action.config.get("statement").is_some());
        assert_eq!(
            action.config["statement"],
            json!("SELECT * FROM users WHERE id = $1")
        );
    }

    #[tokio::test]
    async fn test_legacy_format_rejection() {
        let legacy_yaml = r#"
version: "1.0"
connectors:
  http:
    connections:
      test-api:
        config:
          base_url: "https://example.com"
"#;

        let loader = ConfigLoader::new("default");
        let result = loader.parse_content(legacy_yaml, FileFormat::Yaml).await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Legacy format"));
        assert!(error_msg.contains("no longer supported"));
    }

    #[tokio::test]
    async fn test_missing_required_fields() {
        let invalid_yaml = r#"
version: "1.0"
actions:
  test-action:
    connection: nonexistent
    config:
      method: "GET"
"#;

        let loader = ConfigLoader::new("default");
        let result = loader.parse_content(invalid_yaml, FileFormat::Yaml).await;

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        println!("Actual error: {}", error_msg);
        assert!(error_msg.contains("connections") || error_msg.contains("cannot determine kind"));
    }

    #[tokio::test]
    async fn test_manifest_to_records() {
        let loader = ConfigLoader::new("default");

        // Create a test manifest
        let mut connectors = std::collections::HashMap::new();
        let mut http_connector = ConnectorConfig::default();

        http_connector.connections.insert(
            "test-conn".to_string(),
            ConnectionConfig {
                description: Some("Test connection".to_string()),
                config: json!({
                    "base_url": "https://api.test.com",
                    "authorization": "api_key"
                }),
                metadata: None,
            },
        );

        http_connector.actions.insert(
            "test-action".to_string(),
            ActionConfig {
                connection: "test-conn".to_string(),
                description: Some("Test action".to_string()),
                mcp_enabled: Some(true),
                config: json!({
                    "method": "GET",
                    "path": "/test"
                }),
                metadata: None,
            },
        );

        connectors.insert("http".to_string(), http_connector);

        let manifest = ConfigManifest {
            version: "1.0".to_string(),
            metadata: None,
            connectors,
        };

        let (connections, actions) = loader.manifest_to_records(&manifest).await.unwrap();

        assert_eq!(connections.len(), 1);
        assert_eq!(actions.len(), 1);

        let connection = &connections[0];
        assert_eq!(connection.name, "test-conn");
        assert_eq!(connection.connector, ConnectorKind::new("http"));

        let action = &actions[0];
        assert_eq!(action.name, "test-action");
        assert_eq!(action.connector, ConnectorKind::new("http"));
        assert!(action.mcp_enabled);
    }
}
