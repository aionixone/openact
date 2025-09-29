//! Main registry implementation for managing connectors and executing actions

use crate::{
    error::{RegistryError, RegistryResult},
    factory::{Action, ActionFactory, AsAny, Connection, ConnectionFactory},
};
use openact_core::{
    store::{ActionRepository, ConnectionStore},
    types::ConnectorMetadata,
    ActionRecord, ConnectionRecord, ConnectorKind, Trn,
};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Execution context containing runtime information
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Unique execution ID for tracing
    pub execution_id: String,
    /// Additional context data
    pub metadata: HashMap<String, JsonValue>,
}

impl ExecutionContext {
    /// Create a new execution context with generated ID
    pub fn new() -> Self {
        Self { execution_id: uuid::Uuid::new_v4().to_string(), metadata: HashMap::new() }
    }

    /// Create context with custom execution ID
    pub fn with_id(execution_id: String) -> Self {
        Self { execution_id, metadata: HashMap::new() }
    }

    /// Add metadata to the context
    pub fn with_metadata(mut self, key: String, value: JsonValue) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of action execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// The output data from the action
    pub output: JsonValue,
    /// Execution metadata (timing, logs, etc.)
    pub metadata: HashMap<String, JsonValue>,
    /// Execution context
    pub context: ExecutionContext,
}

impl ExecutionResult {
    /// Create a simple success result
    pub fn success(output: JsonValue, context: ExecutionContext) -> Self {
        Self { output, metadata: HashMap::new(), context }
    }

    /// Add metadata to the result
    pub fn with_metadata(mut self, key: String, value: JsonValue) -> Self {
        self.metadata.insert(key, value);
        self
    }
}

/// Main connector registry for managing and executing actions
pub struct ConnectorRegistry {
    /// Registered connection factories by connector type
    connection_factories: HashMap<ConnectorKind, Arc<dyn ConnectionFactory>>,
    /// Registered action factories by connector type  
    action_factories: HashMap<ConnectorKind, Arc<dyn ActionFactory>>,
    /// Connection store for fetching connection records
    connection_store: Arc<dyn ConnectionStore>,
    /// Action repository for fetching action records
    action_repository: Arc<dyn ActionRepository>,
    /// Connection cache to avoid repeated creation
    connection_cache: Arc<RwLock<HashMap<Trn, Arc<dyn Connection>>>>,
}

impl ConnectorRegistry {
    /// Create a new registry with store backends
    pub fn new<C, A>(connection_store: C, action_repository: A) -> Self
    where
        C: ConnectionStore + 'static,
        A: ActionRepository + 'static,
    {
        Self {
            connection_factories: HashMap::new(),
            action_factories: HashMap::new(),
            connection_store: Arc::new(connection_store),
            action_repository: Arc::new(action_repository),
            connection_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a connection factory for a specific connector type
    pub fn register_connection_factory(&mut self, factory: Arc<dyn ConnectionFactory>) {
        let connector_kind = factory.connector_kind();
        self.connection_factories.insert(connector_kind, factory);
    }

    /// Register an action factory for a specific connector type
    pub fn register_action_factory(&mut self, factory: Arc<dyn ActionFactory>) {
        let connector_kind = factory.connector_kind();
        self.action_factories.insert(connector_kind, factory);
    }

    /// Get list of registered connector types
    pub fn registered_connectors(&self) -> Vec<ConnectorKind> {
        let mut connectors: std::collections::HashSet<ConnectorKind> =
            std::collections::HashSet::new();
        connectors.extend(self.connection_factories.keys().cloned());
        connectors.extend(self.action_factories.keys().cloned());
        connectors.into_iter().collect()
    }

    /// Get metadata for all registered connectors (union of connection/action factories, de-duplicated by kind)
    pub fn connector_metadata(&self) -> Vec<ConnectorMetadata> {
        use std::collections::HashMap as StdHashMap;
        let mut meta_map: StdHashMap<String, ConnectorMetadata> = StdHashMap::new();

        for factory in self.connection_factories.values() {
            let meta = factory.metadata();
            meta_map.entry(meta.kind.as_str().to_string()).or_insert(meta);
        }

        for factory in self.action_factories.values() {
            let meta = factory.metadata();
            meta_map.entry(meta.kind.as_str().to_string()).or_insert(meta);
        }

        meta_map.into_values().collect()
    }

    /// Get metadata for a specific connector
    pub fn connector_metadata_by_kind(&self, kind: &ConnectorKind) -> Option<ConnectorMetadata> {
        if let Some(factory) = self.connection_factories.get(kind) {
            return Some(factory.metadata());
        }
        if let Some(factory) = self.action_factories.get(kind) {
            return Some(factory.metadata());
        }
        None
    }

    /// Execute an action by TRN with input data
    pub async fn execute(
        &self,
        action_trn: &Trn,
        input: JsonValue,
        context: Option<ExecutionContext>,
    ) -> RegistryResult<ExecutionResult> {
        let context = context.unwrap_or_default();

        // Fetch action record
        let action_record = self
            .action_repository
            .get(action_trn)
            .await?
            .ok_or_else(|| RegistryError::ActionNotFound(action_trn.clone()))?;

        // Get connection (from cache or create new)
        let connection = self.get_or_create_connection(&action_record.connection_trn).await?;

        // Create action instance
        let action = self.create_action(&action_record, connection).await?;

        // Validate input if supported
        action.validate_input(&input).await?;

        // Execute action
        let start_time = std::time::Instant::now();
        let raw_output = action.execute(input).await?;
        // Allow action to present an MCP-friendly wrapped output
        let output = action.mcp_wrap_output(raw_output);
        let duration = start_time.elapsed();

        // Build result with metadata
        let mut result = ExecutionResult::success(output, context);
        result = result.with_metadata(
            "duration_ms".to_string(),
            JsonValue::Number(serde_json::Number::from(duration.as_millis() as u64)),
        );
        result = result
            .with_metadata("action_trn".to_string(), JsonValue::String(action_trn.to_string()));
        result = result.with_metadata(
            "connector".to_string(),
            JsonValue::String(action_record.connector.to_string()),
        );

        Ok(result)
    }

    /// Get or create a connection instance
    async fn get_or_create_connection(
        &self,
        connection_trn: &Trn,
    ) -> RegistryResult<Arc<dyn Connection>> {
        // Check cache first
        {
            let cache = self.connection_cache.read().await;
            if let Some(connection) = cache.get(connection_trn) {
                return Ok(connection.clone());
            }
        }

        // Fetch connection record
        let connection_record = self
            .connection_store
            .get(connection_trn)
            .await?
            .ok_or_else(|| RegistryError::ConnectionNotFound(connection_trn.clone()))?;

        // Create connection instance
        let connection = self.create_connection(&connection_record).await?;
        let connection_arc: Arc<dyn Connection> = Arc::from(connection);

        // Cache the connection
        {
            let mut cache = self.connection_cache.write().await;
            cache.insert(connection_trn.clone(), connection_arc.clone());
        }

        Ok(connection_arc)
    }

    /// Create a connection instance from record
    async fn create_connection(
        &self,
        record: &ConnectionRecord,
    ) -> RegistryResult<Box<dyn Connection>> {
        let factory = self
            .connection_factories
            .get(&record.connector)
            .ok_or_else(|| RegistryError::ConnectorNotRegistered(record.connector.clone()))?;

        factory.create_connection(record).await
    }

    /// Create an action instance from record and connection
    async fn create_action(
        &self,
        action_record: &ActionRecord,
        connection: Arc<dyn Connection>,
    ) -> RegistryResult<Box<dyn Action>> {
        let factory = self.action_factories.get(&action_record.connector).ok_or_else(|| {
            RegistryError::ConnectorNotRegistered(action_record.connector.clone())
        })?;

        // Convert Arc<dyn Connection> to Box<dyn Connection>
        // This requires cloning the connection data, but factories expect owned connections
        // TODO: Consider redesigning factories to accept Arc<dyn Connection> directly
        let connection_box = Box::new(ConnectionWrapper(connection));

        factory.create_action(action_record, connection_box).await
    }

    /// Health check for a specific connection
    pub async fn health_check_connection(&self, connection_trn: &Trn) -> RegistryResult<bool> {
        let connection = self.get_or_create_connection(connection_trn).await?;
        connection.health_check().await
    }

    /// Clear connection cache (useful for testing or after configuration changes)
    pub async fn clear_connection_cache(&self) {
        let mut cache = self.connection_cache.write().await;
        cache.clear();
    }

    /// Get statistics about the registry
    pub async fn stats(&self) -> HashMap<String, JsonValue> {
        let mut stats = HashMap::new();
        stats.insert(
            "registered_connectors".to_string(),
            JsonValue::Number(serde_json::Number::from(self.registered_connectors().len())),
        );

        let cache = self.connection_cache.read().await;
        stats.insert(
            "cached_connections".to_string(),
            JsonValue::Number(serde_json::Number::from(cache.len())),
        );

        stats
    }

    /// Instantiate an action instance for the given ActionRecord (helper for schema/metadata needs).
    pub async fn instantiate_action_for_record(
        &self,
        action_record: &ActionRecord,
    ) -> RegistryResult<Box<dyn Action>> {
        // Fetch connection record backing this action
        let connection_record =
            self.connection_store.get(&action_record.connection_trn).await?.ok_or_else(|| {
                RegistryError::ConnectionNotFound(action_record.connection_trn.clone())
            })?;

        // Create connection instance via its factory
        let conn_factory =
            self.connection_factories.get(&connection_record.connector).ok_or_else(|| {
                RegistryError::ConnectorNotRegistered(connection_record.connector.clone())
            })?;
        let connection = conn_factory.create_connection(&connection_record).await?;

        // Create action instance via its factory
        let act_factory = self.action_factories.get(&action_record.connector).ok_or_else(|| {
            RegistryError::ConnectorNotRegistered(action_record.connector.clone())
        })?;

        act_factory.create_action(action_record, connection).await
    }

    /// Derive MCP input/output schemas for an action by instantiating it and
    /// calling the Action's MCP extension hooks.
    pub async fn derive_mcp_schemas(
        &self,
        action_record: &ActionRecord,
    ) -> RegistryResult<(JsonValue, Option<JsonValue>)> {
        let action = self.instantiate_action_for_record(action_record).await?;
        let input = action.mcp_input_schema(action_record);
        let output = action.mcp_output_schema(action_record);
        Ok((input, output))
    }
}

/// Wrapper to convert Arc<dyn Connection> to Box<dyn Connection>
/// This is a temporary solution until we redesign the factory interfaces
struct ConnectionWrapper(Arc<dyn Connection>);

impl AsAny for ConnectionWrapper {
    fn as_any(&self) -> &dyn std::any::Any {
        self.0.as_any()
    }
}

#[async_trait::async_trait]
impl Connection for ConnectionWrapper {
    fn trn(&self) -> &Trn {
        self.0.trn()
    }

    fn connector_kind(&self) -> &ConnectorKind {
        self.0.connector_kind()
    }

    async fn health_check(&self) -> RegistryResult<bool> {
        self.0.health_check().await
    }

    fn metadata(&self) -> HashMap<String, JsonValue> {
        self.0.metadata()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openact_store::memory::{MemoryActionRepository, MemoryConnectionStore};
    use serde_json::json;

    #[tokio::test]
    async fn test_registry_creation() {
        let connection_store = MemoryConnectionStore::new();
        let action_repository = MemoryActionRepository::new();

        let registry = ConnectorRegistry::new(connection_store, action_repository);

        assert_eq!(registry.registered_connectors().len(), 0);

        let stats = registry.stats().await;
        assert_eq!(stats["registered_connectors"], json!(0));
        assert_eq!(stats["cached_connections"], json!(0));
    }
}
