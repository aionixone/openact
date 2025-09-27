//! HTTP connector factory implementation

use crate::{
    error::{RegistryError, RegistryResult},
    factory::{Action, ActionFactory, AsAny, Connection, ConnectionFactory},
};
use async_trait::async_trait;
use openact_connectors::auth::{AuthConnection, AuthConnectionStore};
use openact_connectors::http::{HttpAction, HttpConnection, HttpExecutor};
use openact_core::{types::ConnectorMetadata, ActionRecord, ConnectionRecord, ConnectorKind, Trn};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// HTTP connector factory for creating connections and actions
#[derive(Debug, Default)]
pub struct HttpFactory;

impl HttpFactory {
    /// Create a new HTTP factory
    pub fn new() -> Self {
        Self
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
