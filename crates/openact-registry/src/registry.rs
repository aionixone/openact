//! Main registry implementation for managing connectors and executing actions

use crate::{
    error::{RegistryError, RegistryResult},
    factory::{Action, ActionFactory, Connection, ConnectionFactory},
};
use openact_core::{
    store::{ActionRepository, ConnectionStore},
    types::ConnectorMetadata,
    ActionRecord, ConnectionRecord, ConnectorKind, Trn,
};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
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
    /// Cache for derived MCP schemas/annotations to avoid repeated instantiation
    mcp_derive_cache: Arc<RwLock<HashMap<ActionCacheKey, McpDeriveCacheEntry>>>,
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
            mcp_derive_cache: Arc::new(RwLock::new(HashMap::new())),
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
        let connection_arc = self.create_connection(&connection_record).await?;

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
    ) -> RegistryResult<Arc<dyn Connection>> {
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

        factory.create_action(action_record, connection).await
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
        // Try cache first
        let key = ActionCacheKey(action_record.trn.clone(), action_record.version);
        if let Some((input, output)) = self.try_get_cached_schemas(&key).await {
            return Ok((input, output));
        }

        // Derive by instantiating the action, then cache
        let action = self.instantiate_action_for_record(action_record).await?;
        let input = action.mcp_input_schema(action_record);
        let output = action.mcp_output_schema(action_record);

        self.cache_schemas(&key, input.clone(), output.clone()).await;
        Ok((input, output))
    }

    /// Derive MCP annotations for an action (optional). Returns a JSON value compatible with
    /// openact-mcp-types::ToolAnnotations, to be deserialized by the server layer if present.
    pub async fn derive_mcp_annotations(
        &self,
        action_record: &ActionRecord,
    ) -> RegistryResult<Option<JsonValue>> {
        // Try cache first
        let key = ActionCacheKey(action_record.trn.clone(), action_record.version);
        if let Some(anno) = self.try_get_cached_annotations(&key).await {
            return Ok(anno);
        }

        // Derive by instantiating the action, then cache (merging with any existing schema cache)
        let action = self.instantiate_action_for_record(action_record).await?;
        let anno = action.mcp_annotations(action_record);
        self.cache_annotations(&key, anno.clone()).await;
        Ok(anno)
    }
}

/// TTL for derived MCP schemas/annotations cache
const MCP_DERIVE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ActionCacheKey(Trn, i64);

#[derive(Debug, Clone)]
struct McpDeriveCacheEntry {
    input_schema: Option<JsonValue>,
    output_schema: Option<JsonValue>,
    annotations: Option<JsonValue>,
    inserted_at: Instant,
}

impl ConnectorRegistry {
    fn cache_entry_fresh(entry: &McpDeriveCacheEntry) -> bool {
        entry.inserted_at.elapsed() < MCP_DERIVE_TTL
    }

    fn clone_cached_schemas(entry: &McpDeriveCacheEntry) -> Option<(JsonValue, Option<JsonValue>)> {
        match &entry.input_schema {
            Some(input) if Self::cache_entry_fresh(entry) => {
                Some((input.clone(), entry.output_schema.clone()))
            }
            _ => None,
        }
    }

    async fn try_get_cached_schemas(
        &self,
        key: &ActionCacheKey,
    ) -> Option<(JsonValue, Option<JsonValue>)> {
        let cache = self.mcp_derive_cache.read().await;
        cache.get(key).and_then(Self::clone_cached_schemas)
    }

    async fn cache_schemas(
        &self,
        key: &ActionCacheKey,
        input: JsonValue,
        output: Option<JsonValue>,
    ) {
        let mut cache = self.mcp_derive_cache.write().await;
        let existing = cache.get(key).cloned();
        let annotations = existing.and_then(|e| e.annotations);
        cache.insert(
            key.clone(),
            McpDeriveCacheEntry {
                input_schema: Some(input),
                output_schema: output,
                annotations,
                inserted_at: Instant::now(),
            },
        );
    }

    async fn try_get_cached_annotations(&self, key: &ActionCacheKey) -> Option<Option<JsonValue>> {
        let cache = self.mcp_derive_cache.read().await;
        cache.get(key).and_then(|e| {
            if Self::cache_entry_fresh(e) {
                Some(e.annotations.clone())
            } else {
                None
            }
        })
    }

    async fn cache_annotations(&self, key: &ActionCacheKey, annotations: Option<JsonValue>) {
        let mut cache = self.mcp_derive_cache.write().await;
        let existing = cache.get(key).cloned();
        let (input_schema, output_schema) = match existing {
            Some(e) if Self::cache_entry_fresh(&e) => (e.input_schema, e.output_schema),
            _ => (None, None),
        };
        cache.insert(
            key.clone(),
            McpDeriveCacheEntry {
                input_schema,
                output_schema,
                annotations,
                inserted_at: Instant::now(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::factory::{Action, ActionFactory, AsAny, Connection, ConnectionFactory};
    use chrono::Utc;
    use openact_store::memory::{MemoryActionRepository, MemoryConnectionStore};
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct CountingFactory {
        creations: Arc<AtomicUsize>,
    }

    struct CountingConnection {
        trn: Trn,
        connector: ConnectorKind,
    }

    struct CountingAction {
        trn: Trn,
        connector: ConnectorKind,
    }

    impl AsAny for CountingConnection {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[async_trait::async_trait]
    impl Connection for CountingConnection {
        fn trn(&self) -> &Trn {
            &self.trn
        }

        fn connector_kind(&self) -> &ConnectorKind {
            &self.connector
        }

        async fn health_check(&self) -> RegistryResult<bool> {
            Ok(true)
        }

        fn metadata(&self) -> HashMap<String, JsonValue> {
            HashMap::new()
        }
    }

    impl AsAny for CountingAction {
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[async_trait::async_trait]
    impl Action for CountingAction {
        fn trn(&self) -> &Trn {
            &self.trn
        }

        fn connector_kind(&self) -> &ConnectorKind {
            &self.connector
        }

        async fn execute(&self, _input: JsonValue) -> RegistryResult<JsonValue> {
            Ok(json!({"ok": true}))
        }

        fn metadata(&self) -> HashMap<String, JsonValue> {
            HashMap::new()
        }

        fn mcp_input_schema(&self, _record: &ActionRecord) -> JsonValue {
            json!({ "type": "object" })
        }

        fn mcp_output_schema(&self, _record: &ActionRecord) -> Option<JsonValue> {
            Some(json!({ "type": "object" }))
        }
    }

    #[async_trait::async_trait]
    impl ConnectionFactory for CountingFactory {
        fn connector_kind(&self) -> ConnectorKind {
            ConnectorKind::new("test")
        }

        fn metadata(&self) -> ConnectorMetadata {
            ConnectorMetadata {
                kind: ConnectorKind::new("test"),
                display_name: "Test".into(),
                description: "Counting connector".into(),
                category: "test".into(),
                supported_operations: vec![],
                supports_auth: false,
                example_config: None,
                version: "1.0".into(),
            }
        }

        async fn create_connection(
            &self,
            record: &ConnectionRecord,
        ) -> RegistryResult<Arc<dyn Connection>> {
            Ok(Arc::new(CountingConnection {
                trn: record.trn.clone(),
                connector: record.connector.clone(),
            }))
        }
    }

    #[async_trait::async_trait]
    impl ActionFactory for CountingFactory {
        fn connector_kind(&self) -> ConnectorKind {
            ConnectorKind::new("test")
        }

        fn metadata(&self) -> ConnectorMetadata {
            <Self as ConnectionFactory>::metadata(self)
        }

        async fn create_action(
            &self,
            action_record: &ActionRecord,
            _connection: Arc<dyn Connection>,
        ) -> RegistryResult<Box<dyn Action>> {
            self.creations.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(CountingAction {
                trn: action_record.trn.clone(),
                connector: action_record.connector.clone(),
            }))
        }
    }

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

    #[tokio::test]
    async fn derive_mcp_schemas_uses_cache() {
        let connection_store = MemoryConnectionStore::new();
        let action_repository = MemoryActionRepository::new();

        let counter = Arc::new(AtomicUsize::new(0));
        let factory = Arc::new(CountingFactory { creations: counter.clone() });

        let mut registry =
            ConnectorRegistry::new(connection_store.clone(), action_repository.clone());
        registry.register_connection_factory(factory.clone());
        registry.register_action_factory(factory);

        let now = Utc::now();
        let connector = ConnectorKind::new("test");
        let connection_trn = Trn::new("trn:openact:tenant:connection/test/conn@v1");
        let action_trn = Trn::new("trn:openact:tenant:action/test/run@v1");

        ConnectionStore::upsert(
            &connection_store,
            &ConnectionRecord {
                trn: connection_trn.clone(),
                connector: connector.clone(),
                name: "conn".into(),
                config_json: json!({}),
                created_at: now,
                updated_at: now,
                version: 1,
            },
        )
        .await
        .unwrap();

        ActionRepository::upsert(
            &action_repository,
            &ActionRecord {
                trn: action_trn.clone(),
                connector: connector.clone(),
                name: "run".into(),
                connection_trn: connection_trn.clone(),
                config_json: json!({}),
                mcp_enabled: true,
                mcp_overrides: None,
                created_at: now,
                updated_at: now,
                version: 1,
            },
        )
        .await
        .unwrap();

        let action_record =
            ActionRepository::get(&action_repository, &action_trn).await.unwrap().unwrap();

        registry.derive_mcp_schemas(&action_record).await.unwrap();
        registry.derive_mcp_schemas(&action_record).await.unwrap();

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
