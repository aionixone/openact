//! HTTP connector factory implementation

use openact_registry::{
    error::{RegistryError, RegistryResult},
    factory::{Action, ActionFactory, AsAny, Connection, ConnectionFactory},
    ConnectorRegistry, ConnectorRegistrar,
};
use async_trait::async_trait;
use crate::auth::{AuthConnection, AuthConnectionStore};
use crate::http::{HttpAction, HttpConnection, HttpExecutor};
use openact_core::{types::ConnectorMetadata, ActionRecord, ConnectionRecord, ConnectorKind, Trn};
use serde_json::Value as JsonValue;
use std::{collections::HashMap, sync::Arc};

/// HTTP connector factory for creating connections and actions
#[derive(Debug, Default)]
pub struct HttpFactory;

impl HttpFactory {
    /// Create a new HTTP factory
    pub fn new() -> Self {
        Self
    }

    /// Returns a registrar function for the HTTP factory.
    pub fn registrar() -> ConnectorRegistrar {
        |registry: &mut ConnectorRegistry| {
            let factory = Arc::new(HttpFactory::new());
            registry.register_connection_factory(factory.clone());
            registry.register_action_factory(factory);
            tracing::debug!("Registered HTTP connector via registrar");
        }
    }
}

#[async_trait]
impl ConnectionFactory for HttpFactory {
    fn connector_kind(&self) -> ConnectorKind {
        ConnectorKind::new("http")
    }

    fn metadata(&self) -> ConnectorMetadata {
        ConnectorMetadata {
            kind: ConnectorKind::new("http"),
            display_name: "HTTP".to_string(),
            description: "HTTP REST API connector for making web requests and API calls"
                .to_string(),
            category: "web".to_string(),
            supported_operations: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "PATCH".to_string(),
                "HEAD".to_string(),
                "OPTIONS".to_string(),
            ],
            supports_auth: true,
            example_config: Some(serde_json::json!({
                "base_url": "https://api.example.com",
                "authorization": "api_key",
                "auth_parameters": {
                    "api_key_auth_parameters": {
                        "api_key": "your-api-key",
                        "header_name": "X-API-Key"
                    }
                },
                "default_headers": {
                    "User-Agent": "OpenAct/1.0"
                },
                "timeout_config": {
                    "connect_timeout_ms": 5000,
                    "request_timeout_ms": 30000
                }
            })),
            version: "1.0.0".to_string(),
        }
    }

    async fn create_connection(
        &self,
        record: &ConnectionRecord,
    ) -> RegistryResult<Box<dyn Connection>> {
        // Parse connection config into HttpConnection
        let http_connection: HttpConnection = serde_json::from_value(record.config_json.clone())
            .map_err(|e| {
                RegistryError::ConnectionCreationFailed(format!(
                    "Invalid HTTP connection config: {}",
                    e
                ))
            })?;

        let connection = HttpConnectionWrapper {
            trn: record.trn.clone(),
            connector_kind: record.connector.clone(),
            inner: http_connection,
        };

        Ok(Box::new(connection))
    }
}

#[async_trait]
impl ActionFactory for HttpFactory {
    fn connector_kind(&self) -> ConnectorKind {
        ConnectorKind::new("http")
    }

    fn metadata(&self) -> ConnectorMetadata {
        // Reuse the same metadata as connection side
        <Self as ConnectionFactory>::metadata(self)
    }

    async fn create_action(
        &self,
        action_record: &ActionRecord,
        connection: Box<dyn Connection>,
    ) -> RegistryResult<Box<dyn Action>> {
        // Downcast connection to HttpConnectionWrapper
        let http_connection = connection
            .as_ref()
            .as_any()
            .downcast_ref::<HttpConnectionWrapper>()
            .ok_or_else(|| {
                RegistryError::ActionCreationFailed("Connection is not HTTP connection".to_string())
            })?;

        // Parse action config into HttpAction
        let http_action: HttpAction = serde_json::from_value(action_record.config_json.clone())
            .map_err(|e| {
                RegistryError::ActionCreationFailed(format!("Invalid HTTP action config: {}", e))
            })?;

        let action = HttpActionWrapper {
            trn: action_record.trn.clone(),
            connector_kind: action_record.connector.clone(),
            connection: http_connection.inner.clone(),
            action: http_action,
        };

        Ok(Box::new(action))
    }
}

/// Wrapper for HttpConnection to implement the Connection trait
struct HttpConnectionWrapper {
    trn: Trn,
    connector_kind: ConnectorKind,
    inner: HttpConnection,
}

#[async_trait]
impl Connection for HttpConnectionWrapper {
    fn trn(&self) -> &Trn {
        &self.trn
    }

    fn connector_kind(&self) -> &ConnectorKind {
        &self.connector_kind
    }

    async fn health_check(&self) -> RegistryResult<bool> {
        // For HTTP connections, we could ping the base URL
        // For now, just return true if the connection is configured
        Ok(!self.inner.base_url.is_empty())
    }

    fn metadata(&self) -> HashMap<String, JsonValue> {
        let mut metadata = HashMap::new();
        metadata.insert(
            "base_url".to_string(),
            JsonValue::String(self.inner.base_url.clone()),
        );
        if let Some(ref timeout_config) = self.inner.timeout_config {
            metadata.insert(
                "timeout_config".to_string(),
                serde_json::to_value(timeout_config).unwrap_or(JsonValue::Null),
            );
        }
        metadata
    }
}

impl AsAny for HttpConnectionWrapper {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Wrapper for HttpAction to implement the Action trait
struct HttpActionWrapper {
    trn: Trn,
    connector_kind: ConnectorKind,
    connection: HttpConnection,
    action: HttpAction,
}

#[async_trait]
impl Action for HttpActionWrapper {
    fn trn(&self) -> &Trn {
        &self.trn
    }

    fn connector_kind(&self) -> &ConnectorKind {
        &self.connector_kind
    }

    async fn execute(&self, input: JsonValue) -> RegistryResult<JsonValue> {
        // Create a simple auth store that doesn't store anything (for now)
        let auth_store = SimpleAuthStore;
        let executor = HttpExecutor::new_with_auth_store(auth_store).map_err(|e| {
            RegistryError::ExecutionFailed(format!("Failed to create HTTP executor: {}", e))
        })?;

        // Execute the HTTP action with input for merging
        let result = executor
            .execute(&self.connection, &self.action, Some(input))
            .await
            .map_err(|e| RegistryError::ExecutionFailed(format!("HTTP execution failed: {}", e)))?;

        // Convert HttpExecutionResult to JsonValue
        let output = serde_json::json!({
            "status_code": result.status_code,
            "headers": result.headers,
            "body": result.body,
            "execution_time_ms": result.execution_time_ms
        });

        Ok(output)
    }

    fn metadata(&self) -> HashMap<String, JsonValue> {
        let mut metadata = HashMap::new();
        metadata.insert(
            "method".to_string(),
            JsonValue::String(self.action.method.clone()),
        );
        metadata.insert(
            "path".to_string(),
            JsonValue::String(self.action.path.clone()),
        );
        metadata
    }

    async fn validate_input(&self, input: &JsonValue) -> RegistryResult<()> {
        // Basic validation - ensure input is an object for most HTTP operations
        if !input.is_object() && !input.is_null() {
            return Err(RegistryError::InvalidInput(
                "HTTP action input should be a JSON object or null".to_string(),
            ));
        }
        Ok(())
    }

    fn mcp_input_schema(&self, _record: &openact_core::ActionRecord) -> JsonValue {
        use serde_json::{json, Value};

        let mut properties = serde_json::Map::new();
        let mut required: Vec<String> = Vec::new();

        // 1) Path variables from /path/{var}
        for var in extract_path_variables(&self.action.path) {
            properties.insert(
                var.clone(),
                json!({"type": "string", "description": "Path parameter"}),
            );
            required.push(var);
        }

        // 2) Query parameters (object with per-key hints when available)
        if let Some(ref qp) = self.action.query_params {
            let mut qprops = serde_json::Map::new();
            for (k, v) in qp {
                // MultiValue = Vec<String> implies array<string>, otherwise default to string
                let schema = if v.len() > 1 {
                    json!({"type": "array", "items": {"type": "string"}})
                } else {
                    json!({"type": "string"})
                };
                qprops.insert(k.clone(), schema);
            }
            properties.insert(
                "query".to_string(),
                json!({
                    "type": "object",
                    "description": "Query parameters",
                    "properties": Value::Object(qprops)
                }),
            );
        } else {
            // Generic query object
            properties.insert(
                "query".to_string(),
                json!({"type": "object", "description": "Query parameters"}),
            );
        }

        // 3) Headers (object with per-key hints when available)
        if let Some(ref hs) = self.action.headers {
            let mut hprops = serde_json::Map::new();
            for (k, v) in hs {
                let schema = if v.len() > 1 {
                    json!({"type": "array", "items": {"type": "string"}})
                } else {
                    json!({"type": "string"})
                };
                hprops.insert(k.clone(), schema);
            }
            properties.insert(
                "headers".to_string(),
                json!({
                    "type": "object",
                    "description": "Additional headers",
                    "properties": Value::Object(hprops)
                }),
            );
        } else {
            properties.insert(
                "headers".to_string(),
                json!({"type": "object", "description": "Additional headers"}),
            );
        }

        // 4) Body (for POST/PUT/PATCH), try to infer from typed body
        let method_up = self.action.method.to_uppercase();
        if matches!(method_up.as_str(), "POST" | "PUT" | "PATCH") {
            let body_schema = if let Some(ref b) = self.action.body {
                infer_body_schema_from_typed(b)
            } else if let Some(ref legacy) = self.action.request_body {
                // Legacy: infer from JSON sample
                infer_objectish_schema(legacy).unwrap_or(json!({"type": "object"}))
            } else {
                json!({"type": "object", "description": "Request body"})
            };
            properties.insert("body".to_string(), body_schema);
        }

        json!({
            "type": "object",
            "properties": Value::Object(properties),
            "required": required,
        })
    }

    fn mcp_output_schema(&self, _record: &openact_core::ActionRecord) -> Option<JsonValue> {
        Some(serde_json::json!({
            "type": "object",
            "properties": { "data": { "type": ["object","array","string","number","boolean","null"] } },
        }))
    }

    fn mcp_wrap_output(&self, output: JsonValue) -> JsonValue {
        // Wrap the HTTP execution details as data for a stable shape
        serde_json::json!({ "data": output })
    }
}

// --------- helpers (HTTP schema inference) ---------
use crate::http::body_builder::RequestBodyType;

fn extract_path_variables(path: &str) -> Vec<String> {
    let mut res = Vec::new();
    let mut chars = path.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '{' {
            let mut name = String::new();
            while let Some(c) = chars.next() {
                if c == '}' { break; }
                name.push(c);
            }
            if !name.is_empty() { res.push(name); }
        }
    }
    res
}

fn infer_body_schema_from_typed(body: &RequestBodyType) -> JsonValue {
    use serde_json::json;
    match body {
        RequestBodyType::Json { data } => infer_objectish_schema(data).unwrap_or(json!({"type": "object"})),
        RequestBodyType::Form { .. } => json!({
            "type": "object",
            "additionalProperties": {"type": "string"}
        }),
        RequestBodyType::Multipart { .. } => json!({"type": "object"}),
        RequestBodyType::Raw { .. } => json!({"type": "string", "description": "base64-encoded"}),
        RequestBodyType::Text { .. } => json!({"type": "string"}),
    }
}

fn infer_objectish_schema(value: &JsonValue) -> Option<JsonValue> {
    use serde_json::json;
    match value {
        JsonValue::Object(map) => {
            let mut props = serde_json::Map::new();
            for (k, v) in map.iter() {
                let t = match v {
                    JsonValue::Null => json!({"type": ["null", "string"]}),
                    JsonValue::Bool(_) => json!({"type": "boolean"}),
                    JsonValue::Number(n) => {
                        if n.is_i64() || n.is_u64() { json!({"type": "integer"}) } else { json!({"type": "number"}) }
                    }
                    JsonValue::String(_) => json!({"type": "string"}),
                    JsonValue::Array(_) => json!({"type": "array"}),
                    JsonValue::Object(_) => json!({"type": "object"}),
                };
                props.insert(k.clone(), t);
            }
            Some(json!({"type": "object", "properties": props}))
        }
        JsonValue::Array(items) => {
            if let Some(first) = items.first() {
                let it = infer_objectish_schema(first).unwrap_or(json!({"type": "string"}));
                Some(json!({"type": "array", "items": it}))
            } else {
                Some(json!({"type": "array"}))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    #[tokio::test]
    async fn test_http_factory_creation() {
        let factory = HttpFactory::new();
        assert_eq!(
            ConnectionFactory::connector_kind(&factory),
            ConnectorKind::new("http")
        );
    }

    #[tokio::test]
    async fn test_http_connection_creation() {
        let factory = HttpFactory::new();

        let connection_record = ConnectionRecord {
            trn: Trn::new("trn:openact:test:connection/http/test".to_string()),
            connector: ConnectorKind::new("http"),
            name: "test".to_string(),
            config_json: json!({
                "base_url": "https://api.example.com",
                "authorization": "api_key",
                "auth_parameters": {
                    "api_key_auth_parameters": null,
                    "basic_auth_parameters": null,
                    "oauth_parameters": null
                }
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        };

        let connection = factory.create_connection(&connection_record).await.unwrap();

        assert_eq!(connection.trn(), &connection_record.trn);
        assert_eq!(connection.connector_kind(), &ConnectorKind::new("http"));

        let health = connection.health_check().await.unwrap();
        assert!(health);

        let metadata = connection.metadata();
        assert_eq!(metadata["base_url"], json!("https://api.example.com"));
    }
}

/// Simple auth store that doesn't persist anything (for basic HTTP without OAuth)
#[derive(Debug, Clone)]
struct SimpleAuthStore;

#[async_trait]
impl AuthConnectionStore for SimpleAuthStore {
    async fn get(&self, _auth_ref: &str) -> anyhow::Result<Option<AuthConnection>> {
        Ok(None)
    }

    async fn put(&self, _auth_ref: &str, _connection: &AuthConnection) -> anyhow::Result<()> {
        Ok(())
    }

    async fn delete(&self, _auth_ref: &str) -> anyhow::Result<bool> {
        Ok(false)
    }
}
