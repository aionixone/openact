use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use openact_core::{types::ConnectorMetadata, ActionRecord, ConnectionRecord, ConnectorKind, Trn};
use openact_registry::{
    error::{RegistryError, RegistryResult},
    factory::{Action, ActionFactory, AsAny, Connection, ConnectionFactory},
    ConnectorRegistrar, ConnectorRegistry,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Number, Value as JsonValue};
use uuid::Uuid;

use super::config::GenericAsyncActionConfig;

/// Minimal connection configuration for the generic async connector.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenericAsyncConnection {
    /// Optional display name for observability.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Free-form metadata preserved for downstream usage.
    #[serde(default)]
    pub metadata: HashMap<String, JsonValue>,
}

/// Minimal action configuration for the generic async connector.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GenericAsyncAction {
    pub description: Option<String>,
    #[serde(default)]
    pub tags: HashMap<String, String>,
    #[serde(flatten)]
    pub execution: GenericAsyncActionConfig,
}

/// Factory responsible for registering the generic async connector.
#[derive(Debug, Default)]
pub struct GenericAsyncFactory;

impl GenericAsyncFactory {
    pub fn new() -> Self {
        Self
    }

    pub fn registrar() -> ConnectorRegistrar {
        |registry: &mut ConnectorRegistry| {
            let factory = Arc::new(GenericAsyncFactory::new());
            registry.register_connection_factory(factory.clone());
            registry.register_action_factory(factory);
            tracing::debug!("Registered generic_async connector via registrar");
        }
    }
}

#[async_trait]
impl ConnectionFactory for GenericAsyncFactory {
    fn connector_kind(&self) -> ConnectorKind {
        ConnectorKind::new("generic_async")
    }

    fn metadata(&self) -> ConnectorMetadata {
        ConnectorMetadata {
            kind: ConnectorKind::new("generic_async"),
            display_name: "Generic Async".to_string(),
            description: "Generic async connector for long-running task orchestration".to_string(),
            category: "orchestration".to_string(),
            supported_operations: vec!["async".to_string()],
            supports_auth: false,
            example_config: Some(json!({
                "display_name": "Async Runner"
            })),
            version: "0.1.0".to_string(),
        }
    }

    async fn create_connection(
        &self,
        record: &ConnectionRecord,
    ) -> RegistryResult<Arc<dyn Connection>> {
        let config: GenericAsyncConnection = serde_json::from_value(record.config_json.clone())
            .map_err(|err| {
                RegistryError::ConnectionCreationFailed(format!(
                    "invalid generic_async connection config: {err}"
                ))
            })?;

        let wrapper = GenericAsyncConnectionWrapper {
            trn: record.trn.clone(),
            connector_kind: record.connector.clone(),
            inner: config,
        };

        Ok(Arc::new(wrapper))
    }
}

#[async_trait]
impl ActionFactory for GenericAsyncFactory {
    fn connector_kind(&self) -> ConnectorKind {
        ConnectorKind::new("generic_async")
    }

    fn metadata(&self) -> ConnectorMetadata {
        <Self as ConnectionFactory>::metadata(self)
    }

    async fn create_action(
        &self,
        action_record: &ActionRecord,
        connection: Arc<dyn Connection>,
    ) -> RegistryResult<Box<dyn Action>> {
        let connection = connection
            .as_ref()
            .as_any()
            .downcast_ref::<GenericAsyncConnectionWrapper>()
            .ok_or_else(|| {
                RegistryError::ActionCreationFailed(
                    "connection is not generic_async connection".to_string(),
                )
            })?;

        let action: GenericAsyncAction = serde_json::from_value(action_record.config_json.clone())
            .map_err(|err| {
                RegistryError::ActionCreationFailed(format!(
                    "invalid generic_async action config: {err}"
                ))
            })?;

        let wrapper = GenericAsyncActionWrapper {
            trn: action_record.trn.clone(),
            connector_kind: action_record.connector.clone(),
            connection: connection.inner.clone(),
            action,
        };

        Ok(Box::new(wrapper))
    }
}

struct GenericAsyncConnectionWrapper {
    trn: Trn,
    connector_kind: ConnectorKind,
    inner: GenericAsyncConnection,
}

#[async_trait]
impl Connection for GenericAsyncConnectionWrapper {
    fn trn(&self) -> &Trn {
        &self.trn
    }

    fn connector_kind(&self) -> &ConnectorKind {
        &self.connector_kind
    }

    async fn health_check(&self) -> RegistryResult<bool> {
        Ok(true)
    }

    fn metadata(&self) -> HashMap<String, JsonValue> {
        self.inner.metadata.clone()
    }
}

impl AsAny for GenericAsyncConnectionWrapper {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

struct GenericAsyncActionWrapper {
    trn: Trn,
    connector_kind: ConnectorKind,
    connection: GenericAsyncConnection,
    action: GenericAsyncAction,
}

#[async_trait]
impl Action for GenericAsyncActionWrapper {
    fn trn(&self) -> &Trn {
        &self.trn
    }

    fn connector_kind(&self) -> &ConnectorKind {
        &self.connector_kind
    }

    async fn execute(&self, input: JsonValue) -> RegistryResult<JsonValue> {
        let external_run_id = Uuid::new_v4().to_string();
        let config = self.action.execution.clone();
        let mode = config.resolved_mode();
        let status = mode.status_str();
        let phase = mode.phase_str();
        let heartbeat_timeout = config.heartbeat_timeout_seconds;
        let status_ttl = config.status_ttl_seconds;
        let config_json = serde_json::to_value(&config).unwrap_or(JsonValue::Null);

        let handle = json!({
            "backendId": "generic_async",
            "externalRunId": external_run_id,
            "mode": mode.as_str(),
            "heartbeatTimeoutSeconds": heartbeat_timeout,
            "statusTtlSeconds": status_ttl,
            "config": config_json,
            "tags": self.action.tags.clone(),
            "connection": self.connection.metadata.clone(),
            "input": input,
        });

        let mut payload = serde_json::Map::new();
        payload.insert("status".to_string(), JsonValue::String(status.to_string()));
        payload.insert("mode".to_string(), JsonValue::String(mode.as_str().to_string()));
        payload.insert("phase".to_string(), JsonValue::String(phase.to_string()));
        if let Some(timeout) = heartbeat_timeout {
            payload
                .insert("heartbeatTimeout".to_string(), JsonValue::Number(Number::from(timeout)));
        }
        if let Some(ttl) = status_ttl {
            payload.insert("statusTtl".to_string(), JsonValue::Number(Number::from(ttl)));
        }
        payload.insert("handle".to_string(), handle);

        Ok(JsonValue::Object(payload))
    }

    fn metadata(&self) -> HashMap<String, JsonValue> {
        let mut metadata = HashMap::new();
        if let Some(name) = self.connection.display_name.clone() {
            metadata.insert("connection_display_name".to_string(), JsonValue::String(name));
        }
        if let Some(desc) = self.action.description.clone() {
            metadata.insert("description".to_string(), JsonValue::String(desc));
        }
        metadata
    }

    async fn validate_input(&self, _input: &JsonValue) -> RegistryResult<()> {
        Ok(())
    }
}

impl AsAny for GenericAsyncActionWrapper {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
