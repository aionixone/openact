//! Factory traits for creating connections and actions

use crate::error::RegistryResult;
use async_trait::async_trait;
use openact_core::{types::ConnectorMetadata, ActionRecord, ConnectionRecord, ConnectorKind};
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Trait to enable downcasting for trait objects
pub trait AsAny {
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Factory for creating connection instances
#[async_trait]
pub trait ConnectionFactory: Send + Sync {
    /// The connector type this factory handles
    fn connector_kind(&self) -> ConnectorKind;

    /// Get metadata for this connector (description, category, etc.)
    fn metadata(&self) -> ConnectorMetadata;

    /// Create a connection instance from a connection record
    async fn create_connection(
        &self,
        record: &ConnectionRecord,
    ) -> RegistryResult<Box<dyn Connection>>;
}

/// Factory for creating action instances
#[async_trait]
pub trait ActionFactory: Send + Sync {
    /// The connector type this factory handles
    fn connector_kind(&self) -> ConnectorKind;

    /// Get metadata for this connector (description, category, etc.)
    fn metadata(&self) -> ConnectorMetadata;

    /// Create an action instance from an action record and connection
    async fn create_action(
        &self,
        action_record: &ActionRecord,
        connection: Box<dyn Connection>,
    ) -> RegistryResult<Box<dyn Action>>;
}

/// Runtime connection instance
#[async_trait]
pub trait Connection: Send + Sync + AsAny {
    /// Get the connection's TRN
    fn trn(&self) -> &openact_core::Trn;

    /// Get the connector kind
    fn connector_kind(&self) -> &ConnectorKind;

    /// Test if the connection is healthy
    async fn health_check(&self) -> RegistryResult<bool>;

    /// Get connection metadata (for debugging/monitoring)
    fn metadata(&self) -> HashMap<String, JsonValue>;
}

/// Runtime action instance
#[async_trait]
pub trait Action: Send + Sync {
    /// Get the action's TRN
    fn trn(&self) -> &openact_core::Trn;

    /// Get the connector kind
    fn connector_kind(&self) -> &ConnectorKind;

    /// Execute the action with given input
    async fn execute(&self, input: JsonValue) -> RegistryResult<JsonValue>;

    /// Get action metadata (for debugging/monitoring)
    fn metadata(&self) -> HashMap<String, JsonValue>;

    /// Validate input against action schema (optional)
    async fn validate_input(&self, input: &JsonValue) -> RegistryResult<()> {
        // Default implementation: accept any input
        let _ = input;
        Ok(())
    }

    /// Provide a JSON Schema (as serde_json::Value) describing expected MCP input for this action.
    /// Default: a permissive object.
    fn mcp_input_schema(&self, _record: &ActionRecord) -> JsonValue {
        serde_json::json!({ "type": "object" })
    }

    /// Provide a JSON Schema describing structured MCP output for this action, if any.
    /// Default: none (client may treat as arbitrary JSON).
    fn mcp_output_schema(&self, _record: &ActionRecord) -> Option<JsonValue> {
        None
    }

    /// Give the action a chance to wrap/normalize its execution output for MCP consumers.
    /// Default: pass-through.
    fn mcp_wrap_output(&self, output: JsonValue) -> JsonValue {
        output
    }
}
