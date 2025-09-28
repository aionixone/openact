use openact_core::{ActionRecord, ConnectionRecord, Trn, ConnectorKind};
use openact_config::{ConfigManifest, ConfigLoader};
use crate::error::{RuntimeError, RuntimeResult};

/// Convert a ConfigManifest to connection and action records
/// This bridges the gap between file/JSON config and the execution runtime
pub async fn records_from_manifest(
    manifest: ConfigManifest,
) -> RuntimeResult<(Vec<ConnectionRecord>, Vec<ActionRecord>)> {
    // Use ConfigLoader to convert manifest to records
    let loader = ConfigLoader::new("default");
    let (connection_records, action_records) = loader.manifest_to_records(&manifest).await
        .map_err(|e| RuntimeError::config(format!("Failed to convert manifest: {}", e)))?;

    tracing::debug!(
        connections = connection_records.len(),
        actions = action_records.len(),
        "Converted manifest to records"
    );

    Ok((connection_records, action_records))
}

/// Convert inline JSON config to records
/// This handles the "execute-inline" use case where config is provided as JSON
pub fn records_from_inline_config(
    connections: Option<Vec<serde_json::Value>>,
    actions: Option<Vec<serde_json::Value>>,
) -> RuntimeResult<(Vec<ConnectionRecord>, Vec<ActionRecord>)> {
    let mut connection_records = Vec::new();
    let mut action_records = Vec::new();
    
    let now = chrono::Utc::now();

    // Process connections
    if let Some(conn_configs) = connections {
        for config in conn_configs {
            // Extract required fields
            let trn = config.get("trn")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RuntimeError::config("Connection missing 'trn' field"))?
                .to_string();
            
            let connector = config.get("connector")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RuntimeError::config("Connection missing 'connector' field"))?
                .to_string();

            let name = config.get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RuntimeError::config("Connection missing 'name' field"))?
                .to_string();

            let version = config.get("version")
                .and_then(|v| v.as_i64())
                .unwrap_or(1);

            // Extract config_json (everything except trn, connector, name, version)
            let mut config_json = config.clone();
            if let Some(obj) = config_json.as_object_mut() {
                obj.remove("trn");
                obj.remove("connector");
                obj.remove("name");
                obj.remove("version");
            }

            let record = ConnectionRecord {
                trn: Trn::new(trn),
                connector: ConnectorKind::new(connector),
                name,
                config_json,
                created_at: now,
                updated_at: now,
                version,
            };
            connection_records.push(record);
        }
    }

    // Process actions
    if let Some(action_configs) = actions {
        for config in action_configs {
            // Extract required fields
            let trn = config.get("trn")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RuntimeError::config("Action missing 'trn' field"))?
                .to_string();
            
            let connector = config.get("connector")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RuntimeError::config("Action missing 'connector' field"))?
                .to_string();

            let name = config.get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RuntimeError::config("Action missing 'name' field"))?
                .to_string();

            let connection_trn = config.get("connection_trn")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RuntimeError::config("Action missing 'connection_trn' field"))?
                .to_string();

            let mcp_enabled = config.get("mcp_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            let version = config.get("version")
                .and_then(|v| v.as_i64())
                .unwrap_or(1);

            // Extract config_json (everything except meta fields)
            let mut config_json = config.clone();
            if let Some(obj) = config_json.as_object_mut() {
                obj.remove("trn");
                obj.remove("connector");
                obj.remove("name");
                obj.remove("connection_trn");
                obj.remove("mcp_enabled");
                obj.remove("version");
            }

            let record = ActionRecord {
                trn: Trn::new(trn),
                connector: ConnectorKind::new(connector),
                name,
                connection_trn: Trn::new(connection_trn),
                config_json,
                mcp_enabled,
                mcp_overrides: None, // TODO: Parse from config if needed
                created_at: now,
                updated_at: now,
                version,
            };
            action_records.push(record);
        }
    }

    tracing::debug!(
        connections = connection_records.len(),
        actions = action_records.len(),
        "Converted inline config to records"
    );

    Ok((connection_records, action_records))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_records_from_inline_config() {
        let connections = Some(vec![
            json!({
                "trn": "test:conn:api",
                "connector": "http",
                "name": "api",
                "version": 1,
                "base_url": "https://api.example.com"
            })
        ]);

        let actions = Some(vec![
            json!({
                "trn": "test:action:get-user",
                "connector": "http",
                "name": "get_user",
                "connection_trn": "test:conn:api",
                "version": 1,
                "method": "GET",
                "path": "/users/{id}"
            })
        ]);

        let (conn_records, action_records) = records_from_inline_config(connections, actions).unwrap();

        assert_eq!(conn_records.len(), 1);
        assert_eq!(action_records.len(), 1);

        let conn = &conn_records[0];
        assert_eq!(conn.trn.as_str(), "test:conn:api");
        assert_eq!(conn.connector.as_str(), "http");
        assert_eq!(conn.name, "api");
        assert_eq!(conn.config_json.get("base_url").unwrap(), "https://api.example.com");

        let action = &action_records[0];
        assert_eq!(action.trn.as_str(), "test:action:get-user");
        assert_eq!(action.connector.as_str(), "http");
        assert_eq!(action.name, "get_user");
        assert_eq!(action.connection_trn.as_str(), "test:conn:api");
        assert_eq!(action.config_json.get("method").unwrap(), "GET");
    }

    #[test]
    fn test_records_from_inline_config_missing_fields() {
        let connections = Some(vec![
            json!({
                "connector": "http",
                "name": "api"
                // Missing trn
            })
        ]);

        let result = records_from_inline_config(connections, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing 'trn' field"));
    }
}