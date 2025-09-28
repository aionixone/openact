//! PostgreSQL connector factory implementation

use crate::{
    error::{RegistryError, RegistryResult},
    factory::{Action, ActionFactory, AsAny, Connection, ConnectionFactory},
};
use async_trait::async_trait;
use openact_connectors::postgresql::{
    actions::PostgresAction, PostgresConnection, PostgresExecutor,
};
use openact_connectors::ConnectorError;
use openact_core::{types::ConnectorMetadata, ActionRecord, ConnectionRecord, ConnectorKind, Trn};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;

/// PostgreSQL factory producing connection and action instances backed by sqlx.
#[derive(Debug, Default)]
pub struct PostgresFactory;

impl PostgresFactory {
    pub fn new() -> Self {
        Self
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
            description: "PostgreSQL relational database connector".to_string(),
            category: "database".to_string(),
            supported_operations: vec!["query".to_string(), "execute".to_string()],
            supports_auth: true,
            example_config: Some(serde_json::json!({
                "host": "127.0.0.1",
                "port": 5432,
                "database": "openact",
                "user": "openact",
                "password": "${PG_PASSWORD}",
                "query_params": {
                    "application_name": "openact",
                    "sslmode": "prefer"
                }
            })),
            version: "1.0.0".to_string(),
        }
    }

    async fn create_connection(
        &self,
        record: &ConnectionRecord,
    ) -> RegistryResult<Box<dyn Connection>> {
        let config: PostgresConnection = serde_json::from_value(record.config_json.clone())
            .map_err(|err| {
                RegistryError::ConnectionCreationFailed(format!(
                    "Invalid PostgreSQL connection config: {}",
                    err
                ))
            })?;

        let executor = PostgresExecutor::from_connection(&config)
            .await
            .map_err(|err| RegistryError::ConnectionCreationFailed(err.to_string()))?;

        let connection = PostgresConnectionWrapper {
            trn: record.trn.clone(),
            connector_kind: record.connector.clone(),
            config,
            executor: Arc::new(executor),
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
        <Self as ConnectionFactory>::metadata(self)
    }

    async fn create_action(
        &self,
        action_record: &ActionRecord,
        connection: Box<dyn Connection>,
    ) -> RegistryResult<Box<dyn Action>> {
        let pg_connection = connection
            .as_ref()
            .as_any()
            .downcast_ref::<PostgresConnectionWrapper>()
            .ok_or_else(|| {
                RegistryError::ActionCreationFailed(
                    "Connection is not a PostgreSQL connection".to_string(),
                )
            })?;

        let action = PostgresAction::from_json(action_record.config_json.clone())
            .map_err(|err| RegistryError::ActionCreationFailed(err.to_string()))?;

        let wrapper = PostgresActionWrapper {
            trn: action_record.trn.clone(),
            connector_kind: action_record.connector.clone(),
            action,
            executor: pg_connection.executor.clone(),
        };

        Ok(Box::new(wrapper))
    }
}

struct PostgresConnectionWrapper {
    trn: Trn,
    connector_kind: ConnectorKind,
    config: PostgresConnection,
    executor: Arc<PostgresExecutor>,
}

impl AsAny for PostgresConnectionWrapper {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
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
        self.executor
            .health_check()
            .await
            .map(|_| true)
            .map_err(|err| RegistryError::ConnectionCreationFailed(err.to_string()))
    }

    fn metadata(&self) -> HashMap<String, JsonValue> {
        let mut map = HashMap::new();
        map.insert(
            "host".to_string(),
            JsonValue::String(self.config.host.clone()),
        );
        map.insert(
            "database".to_string(),
            JsonValue::String(self.config.database.clone()),
        );
        map.insert(
            "port".to_string(),
            JsonValue::Number(serde_json::Number::from(self.config.port as u64)),
        );
        map
    }
}

struct PostgresActionWrapper {
    trn: Trn,
    connector_kind: ConnectorKind,
    action: PostgresAction,
    executor: Arc<PostgresExecutor>,
}

impl AsAny for PostgresActionWrapper {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[async_trait]
impl Action for PostgresActionWrapper {
    fn trn(&self) -> &Trn {
        &self.trn
    }

    fn connector_kind(&self) -> &ConnectorKind {
        &self.connector_kind
    }

    fn metadata(&self) -> HashMap<String, JsonValue> {
        let mut meta = HashMap::new();
        meta.insert("statement".to_string(), JsonValue::String(self.action.statement.clone()));
        if !self.action.parameters.is_empty() {
            meta.insert(
                "parameters".to_string(),
                JsonValue::Array(
                    self.action
                        .parameters
                        .iter()
                        .map(|p| {
                            let mut map = serde_json::Map::new();
                            map.insert("name".to_string(), JsonValue::String(p.name.clone()));
                            if let Some(ref ty) = p.param_type {
                                map.insert("type".to_string(), JsonValue::String(ty.clone()));
                            }
                            JsonValue::Object(map)
                        })
                        .collect(),
                ),
            );
        }
        meta
    }

    async fn execute(&self, input: JsonValue) -> RegistryResult<JsonValue> {
        self.executor
            .execute(&self.action, input)
            .await
            .map_err(|err| match err {
                ConnectorError::Validation(msg) => RegistryError::InvalidInput(msg),
                other => RegistryError::ExecutionFailed(other.to_string()),
            })
    }
}
