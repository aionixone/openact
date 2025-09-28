//! PostgreSQL connector factory implementation

use openact_registry::{
    error::{RegistryError, RegistryResult},
    factory::{Action, ActionFactory, AsAny, Connection, ConnectionFactory},
    ConnectorRegistry, ConnectorRegistrar,
};
use async_trait::async_trait;
use crate::postgresql::{PostgresConnection, PostgresExecutor, actions::PostgresAction};
use openact_core::{types::ConnectorMetadata, ActionRecord, ConnectionRecord, ConnectorKind, Trn};
use serde_json::Value as JsonValue;
use std::{collections::HashMap, sync::Arc};

/// PostgreSQL connector factory for creating connections and actions
#[derive(Debug, Default)]
pub struct PostgresFactory;

impl PostgresFactory {
    /// Create a new PostgreSQL factory
    pub fn new() -> Self {
        Self
    }

    /// Returns a registrar function for the PostgreSQL factory.
    pub fn registrar() -> ConnectorRegistrar {
        |registry: &mut ConnectorRegistry| {
            let factory = Arc::new(PostgresFactory::new());
            registry.register_connection_factory(factory.clone());
            registry.register_action_factory(factory);
            tracing::debug!("Registered PostgreSQL connector via registrar");
        }
    }
}

#[async_trait]
impl ConnectionFactory for PostgresFactory {
    fn connector_kind(&self) -> ConnectorKind {
        ConnectorKind::new("postgres")
    }

    fn metadata(&self) -> ConnectorMetadata {
        ConnectorMetadata {
            kind: ConnectorKind::new("postgres"),
            display_name: "PostgreSQL".to_string(),
            description: "PostgreSQL database connector for executing SQL queries and commands"
                .to_string(),
            category: "database".to_string(),
            supported_operations: vec![
                "SELECT".to_string(),
                "INSERT".to_string(),
                "UPDATE".to_string(),
                "DELETE".to_string(),
                "CREATE".to_string(),
                "DROP".to_string(),
                "ALTER".to_string(),
            ],
            supports_auth: true,
            example_config: Some(serde_json::json!({
                "host": "localhost",
                "port": 5432,
                "database": "mydb",
                "user": "postgres",
                "password": "password",
                "ssl_mode": "prefer",
                "max_connections": 10,
                "connect_timeout_seconds": 30
            })),
            version: "1.0.0".to_string(),
        }
    }

    async fn create_connection(
        &self,
        record: &ConnectionRecord,
    ) -> RegistryResult<Box<dyn Connection>> {
        // Parse connection config into PostgresConnection
        let postgres_connection: PostgresConnection = serde_json::from_value(record.config_json.clone())
            .map_err(|e| {
                RegistryError::ConnectionCreationFailed(format!(
                    "Invalid PostgreSQL connection config: {}",
                    e
                ))
            })?;

        let connection = PostgresConnectionWrapper {
            trn: record.trn.clone(),
            connector_kind: record.connector.clone(),
            inner: postgres_connection,
        };

        Ok(Box::new(connection))
    }
}

#[async_trait]
impl ActionFactory for PostgresFactory {
    fn connector_kind(&self) -> ConnectorKind {
        ConnectorKind::new("postgres")
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
        // Downcast connection to PostgresConnectionWrapper
        let postgres_connection = connection
            .as_ref()
            .as_any()
            .downcast_ref::<PostgresConnectionWrapper>()
            .ok_or_else(|| {
                RegistryError::ActionCreationFailed("Connection is not PostgreSQL connection".to_string())
            })?;

        // Parse action config into PostgresAction
        let postgres_action: PostgresAction = serde_json::from_value(action_record.config_json.clone())
            .map_err(|e| {
                RegistryError::ActionCreationFailed(format!("Invalid PostgreSQL action config: {}", e))
            })?;

        let action = PostgresActionWrapper {
            trn: action_record.trn.clone(),
            connector_kind: action_record.connector.clone(),
            connection: postgres_connection.inner.clone(),
            action: postgres_action,
        };

        Ok(Box::new(action))
    }
}

/// Wrapper for PostgresConnection to implement the Connection trait
struct PostgresConnectionWrapper {
    trn: Trn,
    connector_kind: ConnectorKind,
    inner: PostgresConnection,
}

#[async_trait]
impl Connection for PostgresConnectionWrapper {
    fn trn(&self) -> &Trn {
        &self.trn
    }

    fn connector_kind(&self) -> &ConnectorKind {
        &self.connector_kind
    }

    async fn health_check(&self) -> RegistryResult<bool> {
        // Try to create a connection pool and verify connectivity
        match self.inner.create_pool().await {
            Ok(pool) => {
                // Try to execute a simple query
                match sqlx::query("SELECT 1").fetch_one(&pool).await {
                    Ok(_) => Ok(true),
                    Err(_) => Ok(false),
                }
            }
            Err(_) => Ok(false),
        }
    }

    fn metadata(&self) -> HashMap<String, JsonValue> {
        let mut metadata = HashMap::new();
        metadata.insert(
            "host".to_string(),
            JsonValue::String(self.inner.host.clone()),
        );
        metadata.insert(
            "port".to_string(),
            JsonValue::Number(serde_json::Number::from(self.inner.port)),
        );
        metadata.insert(
            "database".to_string(),
            JsonValue::String(self.inner.database.clone()),
        );
        metadata.insert(
            "user".to_string(),
            JsonValue::String(self.inner.user.clone()),
        );
        if let Some(ref ssl_mode) = self.inner.ssl_mode {
            metadata.insert(
                "ssl_mode".to_string(),
                JsonValue::String(ssl_mode.clone()),
            );
        }
        metadata
    }
}

impl AsAny for PostgresConnectionWrapper {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Wrapper for PostgresAction to implement the Action trait
struct PostgresActionWrapper {
    trn: Trn,
    connector_kind: ConnectorKind,
    connection: PostgresConnection,
    action: PostgresAction,
}

#[async_trait]
impl Action for PostgresActionWrapper {
    fn trn(&self) -> &Trn {
        &self.trn
    }

    fn connector_kind(&self) -> &ConnectorKind {
        &self.connector_kind
    }

    async fn execute(&self, input: JsonValue) -> RegistryResult<JsonValue> {
        // Create executor from connection
        let executor = PostgresExecutor::from_connection(&self.connection)
            .await
            .map_err(|e| RegistryError::ExecutionFailed(format!("Failed to create PostgreSQL executor: {}", e)))?;

        // Execute the PostgreSQL action
        let result = executor
            .execute(&self.action, input)
            .await
            .map_err(|e| RegistryError::ExecutionFailed(format!("PostgreSQL execution failed: {}", e)))?;

        Ok(result)
    }

    fn metadata(&self) -> HashMap<String, JsonValue> {
        let mut metadata = HashMap::new();
        metadata.insert(
            "statement".to_string(),
            JsonValue::String(self.action.statement.clone()),
        );
        if !self.action.parameters.is_empty() {
            metadata.insert(
                "parameters".to_string(),
                serde_json::to_value(&self.action.parameters).unwrap_or(JsonValue::Null),
            );
        }
        metadata
    }

    async fn validate_input(&self, input: &JsonValue) -> RegistryResult<()> {
        // Basic validation - ensure input matches expected parameters
        match input {
            JsonValue::Null => {
                if !self.action.parameters.is_empty() {
                    return Err(RegistryError::InvalidInput(
                        format!("PostgreSQL action expects {} parameters but received null", self.action.parameters.len())
                    ));
                }
            }
            JsonValue::Array(values) => {
                if !self.action.parameters.is_empty() && values.len() != self.action.parameters.len() {
                    return Err(RegistryError::InvalidInput(
                        format!("PostgreSQL action expects {} parameters but received {}", self.action.parameters.len(), values.len())
                    ));
                }
            }
            JsonValue::Object(map) => {
                // Check if it's an "args" wrapper
                if let Some(JsonValue::Array(values)) = map.get("args") {
                    if !self.action.parameters.is_empty() && values.len() != self.action.parameters.len() {
                        return Err(RegistryError::InvalidInput(
                            format!("PostgreSQL action expects {} parameters but received {}", self.action.parameters.len(), values.len())
                        ));
                    }
                } else {
                    // Named parameters - check all required parameters are present
                    for param in &self.action.parameters {
                        if !map.contains_key(&param.name) {
                            return Err(RegistryError::InvalidInput(
                                format!("Missing required parameter: {}", param.name)
                            ));
                        }
                    }
                }
            }
            _ => {
                return Err(RegistryError::InvalidInput(
                    "PostgreSQL action input should be null, array, or object".to_string(),
                ));
            }
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
    async fn test_postgres_factory_creation() {
        let factory = PostgresFactory::new();
        assert_eq!(
            ConnectionFactory::connector_kind(&factory),
            ConnectorKind::new("postgres")
        );
    }

    #[tokio::test]
    async fn test_postgres_connection_creation() {
        let factory = PostgresFactory::new();

        let connection_record = ConnectionRecord {
            trn: Trn::new("trn:openact:test:connection/postgresql/test".to_string()),
            connector: ConnectorKind::new("postgresql"),
            name: "test".to_string(),
            config_json: json!({
                "host": "localhost",
                "port": 5432,
                "database": "test",
                "user": "postgres",
                "password": "password"
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        };

        let connection = factory.create_connection(&connection_record).await.unwrap();

        assert_eq!(connection.trn(), &connection_record.trn);
        assert_eq!(connection.connector_kind(), &ConnectorKind::new("postgresql"));

        let metadata = connection.metadata();
        assert_eq!(metadata["host"], json!("localhost"));
        assert_eq!(metadata["port"], json!(5432));
        assert_eq!(metadata["database"], json!("test"));
    }
}
