//! Configuration manager for loading, validating, and importing/exporting configurations

use crate::env_resolver::{EnvResolver, EnvResolverError};
use crate::error::ConfigError;
use crate::loader::ConfigLoader;
use crate::schema::{ActionConfig, ConfigManifest, ConnectionConfig, ConnectorConfig};
use crate::schema_validator::{SchemaValidationError, SchemaValidator};
use chrono::Utc;
use openact_core::store::{ActionRepository, ConnectionStore};
use openact_core::{ActionRecord, ConnectionRecord, ConnectorKind, Trn};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during configuration management
#[derive(Debug, Error)]
pub enum ConfigManagerError {
    #[error("Environment resolution failed: {0}")]
    EnvResolution(#[from] EnvResolverError),
    #[error("Schema validation failed: {0}")]
    SchemaValidation(#[from] SchemaValidationError),
    #[error("Configuration loading failed: {0}")]
    ConfigLoading(#[from] ConfigError),
    #[error("Database operation failed: {0}")]
    Database(String),
    #[error("Duplicate resource name '{name}' in connector '{connector}'")]
    DuplicateResource { connector: String, name: String },
    #[error(
        "Connection '{connection}' not found for action '{action}' in connector '{connector}'"
    )]
    ConnectionNotFound { connector: String, action: String, connection: String },
    #[error("Invalid configuration structure: {0}")]
    InvalidStructure(String),
    #[error("Import conflict: {0}")]
    ImportConflict(String),
    #[error("Export failed: {0}")]
    ExportFailed(String),
}

/// Configuration management strategies
#[derive(Debug, Clone)]
pub enum SyncStrategy {
    /// File is the authority source - overwrite database
    FileToDb,
    /// Database is the authority source - overwrite file
    DbToFile,
    /// Merge configurations with conflict detection
    Merge { on_conflict: ConflictResolution },
}

/// Conflict resolution strategies
#[derive(Debug, Clone)]
pub enum ConflictResolution {
    /// Fail on any conflict
    Fail,
    /// File wins on conflicts
    PreferFile,
    /// Database wins on conflicts
    PreferDb,
    /// Use latest timestamp
    PreferLatest,
}

/// Import options
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// Whether to perform a dry run (validate but don't import)
    pub dry_run: bool,
    /// Whether to overwrite existing resources
    pub force: bool,
    /// Whether to validate before importing
    pub validate: bool,
    /// Namespace prefix for imported resources
    pub namespace: Option<String>,
    /// Versioning strategy for TRN generation and import behavior
    pub versioning: VersioningStrategy,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            force: false,
            validate: true,
            namespace: None,
            versioning: VersioningStrategy::AlwaysBump,
        }
    }
}

/// Versioning strategies for import
#[derive(Debug, Clone)]
pub enum VersioningStrategy {
    /// Always create a new TRN with next version (default)
    AlwaysBump,
    /// Only bump when the incoming config differs from the latest; otherwise reuse latest and skip update
    ReuseIfUnchanged,
    /// Do not create or update; keep pointing to latest existing TRN (skip import)
    ForceRollbackToLatest,
}

/// Export options
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Specific connector types to export (empty = all)
    pub connectors: Vec<ConnectorKind>,
    /// Whether to include sensitive data (tokens, passwords)
    pub include_sensitive: bool,
    /// Whether to resolve environment variables in output
    pub resolve_env_vars: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self { connectors: vec![], include_sensitive: false, resolve_env_vars: false }
    }
}

/// Result of an import operation
#[derive(Debug, Clone)]
pub struct ImportResult {
    /// Number of connections created
    pub connections_created: usize,
    /// Number of connections updated
    pub connections_updated: usize,
    /// Number of actions created
    pub actions_created: usize,
    /// Number of actions updated
    pub actions_updated: usize,
    /// List of conflicts encountered
    pub conflicts: Vec<ImportConflict>,
    /// Whether this was a dry run
    pub dry_run: bool,
}

/// Import conflict information
#[derive(Debug, Clone)]
pub struct ImportConflict {
    /// Type of resource (connection, action)
    pub resource_type: String,
    /// TRN of the conflicting resource
    pub trn: Trn,
    /// Description of the conflict
    pub message: String,
}

/// Configuration manager for handling file-to-database operations
pub struct ConfigManager {
    env_resolver: EnvResolver,
    schema_validator: SchemaValidator,
    config_loader: ConfigLoader,
}

impl ConfigManager {
    /// Create a new configuration manager
    pub fn new() -> Self {
        Self {
            env_resolver: EnvResolver::default(),
            schema_validator: SchemaValidator::new(),
            config_loader: ConfigLoader::new("default"),
        }
    }

    /// Create with custom components
    pub fn with_components(
        env_resolver: EnvResolver,
        schema_validator: SchemaValidator,
        config_loader: ConfigLoader,
    ) -> Self {
        Self { env_resolver, schema_validator, config_loader }
    }

    /// Load configuration from file with environment variable resolution
    pub async fn load_from_file<P: AsRef<Path>>(
        &self,
        file_path: P,
    ) -> Result<ConfigManifest, ConfigManagerError> {
        // Load raw configuration
        let mut manifest = self.config_loader.load_from_file(file_path).await?;

        // Resolve environment variables
        for (_connector_name, connector_config) in &mut manifest.connectors {
            // Resolve connection configurations
            for (_connection_name, connection_config) in &mut connector_config.connections {
                connection_config.config = self.env_resolver.resolve(&connection_config.config)?;
            }

            // Resolve action configurations
            if !connector_config.actions.is_empty() {
                let actions = &mut connector_config.actions;
                for (_action_name, action_config) in actions {
                    // 1) Resolve env vars
                    action_config.config = self.env_resolver.resolve(&action_config.config)?;
                    // 2) Normalize schema: parameters -> input_schema (if present and input_schema missing)
                    action_config.config = normalize_action_schema(action_config.config.clone());
                }
            }
        }

        Ok(manifest)
    }

    /// Validate a configuration manifest
    pub fn validate(&self, manifest: &ConfigManifest) -> Result<(), ConfigManagerError> {
        let mut connection_refs = HashMap::new();

        for (connector_name, connector_config) in &manifest.connectors {
            let connector_kind = ConnectorKind::new(connector_name.clone());

            // Track connections for reference validation
            let mut connector_connections = std::collections::HashSet::new();

            // Validate connections
            for (connection_name, connection_config) in &connector_config.connections {
                // Check for duplicates
                if !connector_connections.insert(connection_name.clone()) {
                    return Err(ConfigManagerError::DuplicateResource {
                        connector: connector_name.clone(),
                        name: connection_name.clone(),
                    });
                }

                // Validate connection schema
                self.schema_validator
                    .validate_connection(&connector_kind, &connection_config.config)?;
            }

            connection_refs.insert(connector_name.clone(), connector_connections);

            // Validate actions
            if !connector_config.actions.is_empty() {
                let actions = &connector_config.actions;
                let mut action_names = std::collections::HashSet::new();

                for (action_name, action_config) in actions {
                    // Check for duplicate action names
                    if !action_names.insert(action_name.clone()) {
                        return Err(ConfigManagerError::DuplicateResource {
                            connector: connector_name.clone(),
                            name: action_name.clone(),
                        });
                    }

                    // Validate connection reference
                    if let Some(connector_connections) = connection_refs.get(connector_name) {
                        if !connector_connections.contains(&action_config.connection) {
                            return Err(ConfigManagerError::ConnectionNotFound {
                                connector: connector_name.clone(),
                                action: action_name.clone(),
                                connection: action_config.connection.clone(),
                            });
                        }
                    }

                    // Validate action schema (connector-specific structural checks)
                    self.schema_validator
                        .validate_action(&connector_kind, &action_config.config)?;

                    // If action.config.input_schema exists, validate it is a valid JSON Schema
                    if let Some(schema_val) = action_config.config.get("input_schema") {
                        if !schema_val.is_object() {
                            return Err(ConfigManagerError::InvalidStructure(format!(
                                "input_schema for action '{}' in connector '{}' must be a JSON object",
                                action_name, connector_name
                            )));
                        }
                        // Compile schema to ensure it is valid
                        let compiled =
                            jsonschema::JSONSchema::compile(schema_val).map_err(|e| {
                                ConfigManagerError::InvalidStructure(format!(
                                    "Invalid JSON Schema for action '{}' in connector '{}': {}",
                                    action_name, connector_name, e
                                ))
                            })?;
                        // Optional: quick self-validation with empty object
                        let dummy = serde_json::json!({});
                        let _ = compiled.validate(&dummy); // ignore result; we only care schema compiles
                    }
                }
            }
        }

        Ok(())
    }

    /// Import configuration manifest to database
    pub async fn import_to_db<C, A>(
        &self,
        manifest: &ConfigManifest,
        connection_repo: &C,
        action_repo: &A,
        options: &ImportOptions,
    ) -> Result<ImportResult, ConfigManagerError>
    where
        C: ConnectionStore,
        A: ActionRepository,
    {
        // For SQL stores, we should use transactions. For now, continue with current implementation
        // TODO: Add transaction support for atomicity
        self.import_to_db_impl(manifest, connection_repo, action_repo, options).await
    }

    /// Internal implementation of import_to_db
    async fn import_to_db_impl<C, A>(
        &self,
        manifest: &ConfigManifest,
        connection_repo: &C,
        action_repo: &A,
        options: &ImportOptions,
    ) -> Result<ImportResult, ConfigManagerError>
    where
        C: ConnectionStore,
        A: ActionRepository,
    {
        // Validate before importing if requested
        if options.validate {
            self.validate(manifest)?;
        }

        let mut import_result = ImportResult {
            connections_created: 0,
            connections_updated: 0,
            actions_created: 0,
            actions_updated: 0,
            conflicts: vec![],
            dry_run: options.dry_run,
        };

        // Process each connector
        for (connector_name, connector_config) in &manifest.connectors {
            let connector_kind = ConnectorKind::new(connector_name.clone());

            // Preload existing records for this connector to determine next versions
            let existing_conn_records = connection_repo
                .list_by_connector(connector_kind.canonical().as_str())
                .await
                .map_err(|e| ConfigManagerError::Database(e.to_string()))?;

            // Compute versioned TRNs for all connections first, to keep actions referencing consistent versions
            // Map: resource_name -> (planned_trn, skip_import)
            let mut planned_connection_trns: HashMap<String, (Trn, bool)> = HashMap::new();
            for (connection_name, connection_config) in &connector_config.connections {
                let resource_name = match options.namespace.as_deref() {
                    Some(ns) => format!("{}-{}", ns, connection_name),
                    None => connection_name.clone(),
                };
                // Determine latest version and plan
                let mut latest: Option<(&ConnectionRecord, i64)> = None;
                for rec in existing_conn_records.iter() {
                    if let Some(parsed) = rec.trn.parse_connection() {
                        if parsed.tenant == "default" && parsed.name == resource_name {
                            let v = parsed.version;
                            if latest.as_ref().map(|(_, lv)| v > *lv).unwrap_or(true) {
                                latest = Some((rec, v));
                            }
                        }
                    }
                }
                let (planned_trn, skip_import) = match options.versioning.clone() {
                    VersioningStrategy::AlwaysBump => {
                        let next_ver = latest.map(|(_, v)| v + 1).unwrap_or(1);
                        (
                            self.build_connection_trn_with_version(
                                &connector_kind,
                                &resource_name,
                                next_ver,
                            ),
                            false,
                        )
                    }
                    VersioningStrategy::ReuseIfUnchanged => {
                        if let Some((rec, v)) = latest {
                            if rec.config_json == connection_config.config {
                                (
                                    self.build_connection_trn_with_version(
                                        &connector_kind,
                                        &resource_name,
                                        v,
                                    ),
                                    true,
                                )
                            } else {
                                (
                                    self.build_connection_trn_with_version(
                                        &connector_kind,
                                        &resource_name,
                                        v + 1,
                                    ),
                                    false,
                                )
                            }
                        } else {
                            (
                                self.build_connection_trn_with_version(
                                    &connector_kind,
                                    &resource_name,
                                    1,
                                ),
                                false,
                            )
                        }
                    }
                    VersioningStrategy::ForceRollbackToLatest => {
                        if let Some((_, v)) = latest {
                            (
                                self.build_connection_trn_with_version(
                                    &connector_kind,
                                    &resource_name,
                                    v,
                                ),
                                true,
                            )
                        } else {
                            (
                                self.build_connection_trn_with_version(
                                    &connector_kind,
                                    &resource_name,
                                    1,
                                ),
                                options.dry_run,
                            )
                        }
                    }
                };
                planned_connection_trns.insert(resource_name, (planned_trn, skip_import));
            }

            // Import connections with planned versioned TRNs
            for (connection_name, connection_config) in &connector_config.connections {
                let resource_name = match options.namespace.as_deref() {
                    Some(ns) => format!("{}-{}", ns, connection_name),
                    None => connection_name.clone(),
                };
                let (connection_trn, skip_connection_import) = planned_connection_trns
                    .get(&resource_name)
                    .expect("planned connection TRN must exist")
                    .clone();
                if skip_connection_import {
                    continue;
                }

                match self
                    .import_connection(
                        &connection_trn,
                        &connector_kind,
                        connection_name,
                        connection_config,
                        connection_repo,
                        options,
                    )
                    .await?
                {
                    ImportAction::Created => import_result.connections_created += 1,
                    ImportAction::Updated => import_result.connections_updated += 1,
                    ImportAction::Conflict(conflict) => import_result.conflicts.push(conflict),
                }
            }

            // Import actions
            if !connector_config.actions.is_empty() {
                let actions = &connector_config.actions;
                // Preload existing actions for next-version calculation
                let existing_action_records = action_repo
                    .list_by_connector(&connector_kind)
                    .await
                    .map_err(|e| ConfigManagerError::Database(e.to_string()))?;

                for (action_name, action_config) in actions {
                    let resource_name = match options.namespace.as_deref() {
                        Some(ns) => format!("{}-{}", ns, action_name),
                        None => action_name.clone(),
                    };
                    // Compute next/reuse version for action
                    let mut latest: Option<(&ActionRecord, i64)> = None;
                    for rec in existing_action_records.iter() {
                        if rec.name == resource_name {
                            if let Some(parsed) = rec.trn.parse_action() {
                                if parsed.tenant == "default" && parsed.name == resource_name {
                                    let v = parsed.version;
                                    if latest.as_ref().map(|(_, lv)| v > *lv).unwrap_or(true) {
                                        latest = Some((rec, v));
                                    }
                                }
                            }
                        }
                    }
                    let (action_trn, skip_action_import) = match options.versioning.clone() {
                        VersioningStrategy::AlwaysBump => {
                            let next_ver = latest.map(|(_, v)| v + 1).unwrap_or(1);
                            (
                                self.build_action_trn_with_version(
                                    &connector_kind,
                                    &resource_name,
                                    next_ver,
                                ),
                                false,
                            )
                        }
                        VersioningStrategy::ReuseIfUnchanged => {
                            if let Some((rec, v)) = latest {
                                if rec.config_json == action_config.config {
                                    (
                                        self.build_action_trn_with_version(
                                            &connector_kind,
                                            &resource_name,
                                            v,
                                        ),
                                        true,
                                    )
                                } else {
                                    (
                                        self.build_action_trn_with_version(
                                            &connector_kind,
                                            &resource_name,
                                            v + 1,
                                        ),
                                        false,
                                    )
                                }
                            } else {
                                (
                                    self.build_action_trn_with_version(
                                        &connector_kind,
                                        &resource_name,
                                        1,
                                    ),
                                    false,
                                )
                            }
                        }
                        VersioningStrategy::ForceRollbackToLatest => {
                            if let Some((_, v)) = latest {
                                (
                                    self.build_action_trn_with_version(
                                        &connector_kind,
                                        &resource_name,
                                        v,
                                    ),
                                    true,
                                )
                            } else {
                                (
                                    self.build_action_trn_with_version(
                                        &connector_kind,
                                        &resource_name,
                                        1,
                                    ),
                                    options.dry_run,
                                )
                            }
                        }
                    };

                    // Use the planned versioned connection TRN for this connector
                    let conn_res_name = match options.namespace.as_deref() {
                        Some(ns) => format!("{}-{}", ns, &action_config.connection),
                        None => action_config.connection.clone(),
                    };
                    let (connection_trn, _) =
                        planned_connection_trns.get(&conn_res_name).cloned().ok_or_else(|| {
                            ConfigManagerError::InvalidStructure(format!(
                                "Missing connection '{}' for action '{}' in connector '{}'",
                                action_config.connection, action_name, connector_name
                            ))
                        })?;

                    if skip_action_import {
                        continue;
                    }

                    match self
                        .import_action(
                            &action_trn,
                            &connector_kind,
                            action_name,
                            &connection_trn,
                            action_config,
                            action_repo,
                            options,
                        )
                        .await?
                    {
                        ImportAction::Created => import_result.actions_created += 1,
                        ImportAction::Updated => import_result.actions_updated += 1,
                        ImportAction::Conflict(conflict) => import_result.conflicts.push(conflict),
                    }
                }
            }
        }

        Ok(import_result)
    }

    /// Export configuration from database to manifest
    pub async fn export_from_db<C, A>(
        &self,
        connection_repo: &C,
        action_repo: &A,
        options: &ExportOptions,
    ) -> Result<ConfigManifest, ConfigManagerError>
    where
        C: ConnectionStore,
        A: ActionRepository,
    {
        let mut manifest = ConfigManifest {
            version: "1.0".to_string(),
            metadata: None,
            connectors: HashMap::new(),
        };

        // Dynamically discover connector types from database
        let connector_types = if options.connectors.is_empty() {
            // Auto-discover all connector types that have connections
            connection_repo
                .list_distinct_connectors()
                .await
                .map_err(|e| ConfigManagerError::Database(e.to_string()))?
        } else {
            options.connectors.clone()
        };

        let mut connections_by_connector: HashMap<ConnectorKind, Vec<ConnectionRecord>> =
            HashMap::new();

        for connector_kind in &connector_types {
            let connections = connection_repo
                .list_by_connector(&connector_kind.to_string())
                .await
                .map_err(|e| ConfigManagerError::Database(e.to_string()))?;

            if !connections.is_empty() {
                connections_by_connector.insert(connector_kind.clone(), connections);
            }
        }

        // Process each connector
        for (connector_kind, connections) in connections_by_connector {
            let connector_name = connector_kind.to_string();
            let mut connector_config =
                ConnectorConfig { connections: HashMap::new(), actions: HashMap::new() };

            // Add connections
            for connection in connections {
                let connection_name = self.extract_resource_name(&connection.trn)?;
                let mut config_json = connection.config_json.clone();

                // Optionally remove sensitive data
                if !options.include_sensitive {
                    self.remove_sensitive_data(&mut config_json);
                }

                connector_config.connections.insert(
                    connection_name,
                    ConnectionConfig { description: None, config: config_json, metadata: None },
                );
            }

            // Get actions for this connector
            let actions = action_repo
                .list_by_connector(&connector_kind)
                .await
                .map_err(|e| ConfigManagerError::Database(e.to_string()))?;

            // Add actions
            let actions_map = &mut connector_config.actions;
            for action in actions {
                let action_name = self.extract_resource_name(&action.trn)?;
                let connection_name = self.extract_resource_name(&action.connection_trn)?;

                let mut config_json = action.config_json.clone();
                if !options.include_sensitive {
                    self.remove_sensitive_data(&mut config_json);
                }

                // Extract MCP overrides from action record and convert to metadata
                let mut metadata = None;
                if let Some(ref mcp_overrides) = action.mcp_overrides {
                    let mut meta_map = std::collections::HashMap::new();
                    meta_map.insert(
                        "mcp_overrides".to_string(),
                        serde_json::to_value(mcp_overrides).unwrap(),
                    );
                    metadata = Some(meta_map);
                }

                actions_map.insert(
                    action_name,
                    ActionConfig {
                        connection: connection_name,
                        description: None,
                        mcp_enabled: if action.mcp_enabled { Some(true) } else { None },
                        config: config_json,
                        metadata,
                    },
                );
            }

            manifest.connectors.insert(connector_name, connector_config);
        }

        Ok(manifest)
    }

    // Helper methods

    fn build_connection_trn_with_version(
        &self,
        connector: &ConnectorKind,
        resource_name: &str,
        version: i64,
    ) -> Trn {
        Trn::new(format!(
            "trn:openact:default:connection/{}/{}@v{}",
            connector, resource_name, version
        ))
    }

    fn build_action_trn_with_version(
        &self,
        connector: &ConnectorKind,
        resource_name: &str,
        version: i64,
    ) -> Trn {
        Trn::new(format!("trn:openact:default:action/{}/{}@v{}", connector, resource_name, version))
    }

    fn extract_resource_name(&self, trn: &Trn) -> Result<String, ConfigManagerError> {
        let trn_str = trn.as_str();
        let parts: Vec<&str> = trn_str.split('/').collect();
        if parts.len() >= 2 {
            let name_with_version = parts[parts.len() - 1];
            // Remove @version suffix if present
            let name = name_with_version.split('@').next().unwrap_or(name_with_version);
            Ok(name.to_string())
        } else {
            Err(ConfigManagerError::InvalidStructure(format!("Invalid TRN format: {}", trn_str)))
        }
    }

    async fn import_connection<C>(
        &self,
        trn: &Trn,
        connector: &ConnectorKind,
        name: &str,
        config: &ConnectionConfig,
        repo: &C,
        options: &ImportOptions,
    ) -> Result<ImportAction, ConfigManagerError>
    where
        C: ConnectionStore,
    {
        let existing =
            repo.get(trn).await.map_err(|e| ConfigManagerError::Database(e.to_string()))?;

        let record = ConnectionRecord {
            trn: trn.clone(),
            connector: connector.clone(),
            name: name.to_string(),
            config_json: config.config.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: existing.as_ref().map(|r| r.version + 1).unwrap_or(1),
        };

        match existing {
            Some(existing_record) => {
                if !options.force && existing_record.config_json != config.config {
                    return Ok(ImportAction::Conflict(ImportConflict {
                        resource_type: "connection".to_string(),
                        trn: trn.clone(),
                        message: "Configuration differs from existing record".to_string(),
                    }));
                }

                if !options.dry_run {
                    repo.upsert(&record)
                        .await
                        .map_err(|e| ConfigManagerError::Database(e.to_string()))?;
                }
                Ok(ImportAction::Updated)
            }
            None => {
                if !options.dry_run {
                    repo.upsert(&record)
                        .await
                        .map_err(|e| ConfigManagerError::Database(e.to_string()))?;
                }
                Ok(ImportAction::Created)
            }
        }
    }

    async fn import_action<A>(
        &self,
        trn: &Trn,
        connector: &ConnectorKind,
        name: &str,
        connection_trn: &Trn,
        config: &ActionConfig,
        repo: &A,
        options: &ImportOptions,
    ) -> Result<ImportAction, ConfigManagerError>
    where
        A: ActionRepository,
    {
        let existing =
            repo.get(trn).await.map_err(|e| ConfigManagerError::Database(e.to_string()))?;

        // Extract MCP overrides from metadata if present
        let mcp_overrides = if let Some(ref metadata) = config.metadata {
            if let Some(overrides_value) = metadata.get("mcp_overrides") {
                serde_json::from_value(overrides_value.clone()).ok()
            } else {
                None
            }
        } else {
            None
        };

        let record = ActionRecord {
            trn: trn.clone(),
            connector: connector.clone(),
            name: name.to_string(),
            connection_trn: connection_trn.clone(),
            config_json: config.config.clone(),
            mcp_enabled: config.mcp_enabled.unwrap_or(false),
            mcp_overrides,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: existing.as_ref().map(|r| r.version + 1).unwrap_or(1),
        };

        match existing {
            Some(existing_record) => {
                if !options.force
                    && (existing_record.config_json != config.config
                        || existing_record.mcp_enabled != record.mcp_enabled
                        || existing_record.mcp_overrides != record.mcp_overrides)
                {
                    return Ok(ImportAction::Conflict(ImportConflict {
                        resource_type: "action".to_string(),
                        trn: trn.clone(),
                        message: "Configuration differs from existing record".to_string(),
                    }));
                }

                if !options.dry_run {
                    repo.upsert(&record)
                        .await
                        .map_err(|e| ConfigManagerError::Database(e.to_string()))?;
                }
                Ok(ImportAction::Updated)
            }
            None => {
                if !options.dry_run {
                    repo.upsert(&record)
                        .await
                        .map_err(|e| ConfigManagerError::Database(e.to_string()))?;
                }
                Ok(ImportAction::Created)
            }
        }
    }

    fn remove_sensitive_data(&self, config: &mut JsonValue) {
        if let JsonValue::Object(obj) = config {
            // Extended list of common sensitive keys
            let sensitive_keys = [
                // Basic auth
                "password",
                "passwd",
                "pwd",
                // Tokens
                "token",
                "access_token",
                "refresh_token",
                "bearer_token",
                "auth_token",
                // API keys
                "api_key",
                "apikey",
                "key",
                "private_key",
                // Secrets
                "secret",
                "client_secret",
                "shared_secret",
                // Authorization
                "authorization",
                "credential",
                "credentials",
                // Certificates and signatures
                "cert",
                "certificate",
                "signature",
                "private",
                "pem",
            ];

            // First pass: identify and redact leaf values only
            for obj_key in obj.keys().cloned().collect::<Vec<_>>() {
                if let Some(value) = obj.get(&obj_key) {
                    // Only redact if the value is a primitive (not an object or array)
                    if !value.is_object() && !value.is_array() {
                        let key_lower = obj_key.to_lowercase();
                        for sensitive_key in &sensitive_keys {
                            if key_lower.contains(sensitive_key) {
                                obj.insert(
                                    obj_key.clone(),
                                    JsonValue::String("***REDACTED***".to_string()),
                                );
                                break;
                            }
                        }
                    }
                }
            }

            // Recursively handle nested objects
            for (_, value) in obj.iter_mut() {
                if let JsonValue::Object(_) = value {
                    self.remove_sensitive_data(value);
                }
            }
        }
    }
}

fn normalize_action_schema(mut config: JsonValue) -> JsonValue {
    // If input_schema already exists and is an object, trust it
    if let Some(s) = config.get("input_schema") {
        if s.is_object() {
            return config;
        }
    }
    if config.get("schema").and_then(|s| if s.is_object() { Some(()) } else { None }).is_some() {
        // Clone value first to avoid borrow conflict
        let schema_value = config.get("schema").cloned().unwrap();
        if let Some(obj) = config.as_object_mut() {
            obj.insert("input_schema".to_string(), schema_value);
        }
        return config;
    }

    // If parameters present, convert to schema
    if let Some(params) = config.get("parameters").and_then(|v| v.as_array()) {
        use serde_json::Map;
        let mut properties = Map::new();
        let mut required: Vec<String> = Vec::new();
        for p in params {
            if let Some(name) = p.get("name").and_then(|v| v.as_str()) {
                let typ = p.get("type").and_then(|v| v.as_str()).unwrap_or("string");
                let desc = p.get("description").and_then(|v| v.as_str());
                let req = p.get("required").and_then(|v| v.as_bool()).unwrap_or(false);
                let mut prop = serde_json::json!({ "type": typ });
                if let Some(d) = desc {
                    prop["description"] = serde_json::Value::String(d.to_string());
                }
                properties.insert(name.to_string(), prop);
                if req {
                    required.push(name.to_string());
                }
            }
        }
        let mut out = serde_json::json!({ "type": "object", "properties": properties });
        if !required.is_empty() {
            out["required"] = serde_json::Value::Array(
                required.into_iter().map(serde_json::Value::String).collect(),
            );
        }
        if let Some(obj) = config.as_object_mut() {
            obj.insert("input_schema".to_string(), out);
        }
        return config;
    }

    config
}

impl Default for ConfigManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Action taken during import
#[derive(Debug)]
enum ImportAction {
    Created,
    Updated,
    Conflict(ImportConflict),
}

#[cfg(test)]
mod tests {
    use super::*;
    use openact_store::{MemoryActionRepository, MemoryConnectionStore};
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_load_and_validate_config() {
        let config_content = r#"
version: "1.0"
connections:
  github:
    kind: http
    base_url: "${OPENACT_BASE_URL:https://api.github.com}"
    auth:
      type: "bearer"
      token: "${OPENACT_TOKEN}"

actions:
  get-user:
    connection: "github"
    config:
      method: "GET"
      path: "/user"
"#;

        // Set up environment
        std::env::set_var("OPENACT_BASE_URL", "https://api.github.com");
        std::env::set_var("OPENACT_TOKEN", "test-token");

        // Write to temp file with .yaml extension
        let mut temp_file = NamedTempFile::with_suffix(".yaml").unwrap();
        temp_file.write_all(config_content.as_bytes()).unwrap();

        let manager = ConfigManager::new();
        let manifest = manager.load_from_file(temp_file.path()).await.unwrap();

        // Validate loaded configuration
        assert!(manager.validate(&manifest).is_ok());

        // Check environment variable resolution
        let github_connection = &manifest.connectors["http"].connections["github"];
        assert_eq!(github_connection.config["base_url"], json!("https://api.github.com"));
        assert_eq!(github_connection.config["auth"]["token"], json!("test-token"));

        // Cleanup
        std::env::remove_var("OPENACT_BASE_URL");
        std::env::remove_var("OPENACT_TOKEN");
    }

    #[tokio::test]
    async fn test_import_to_db() {
        let manifest = ConfigManifest {
            version: "1.0".to_string(),
            metadata: None,
            connectors: {
                let mut connectors = HashMap::new();
                let mut connector_config =
                    ConnectorConfig { connections: HashMap::new(), actions: HashMap::new() };

                connector_config.connections.insert(
                    "test-conn".to_string(),
                    ConnectionConfig {
                        description: None,
                        config: json!({
                            "base_url": "https://api.test.com",
                            "auth": { "type": "none" }
                        }),
                        metadata: None,
                    },
                );

                connector_config.actions.insert(
                    "test-action".to_string(),
                    ActionConfig {
                        connection: "test-conn".to_string(),
                        description: None,
                        mcp_enabled: Some(true),
                        config: json!({
                            "method": "GET",
                            "path": "/test"
                        }),
                        metadata: None,
                    },
                );

                connectors.insert("http".to_string(), connector_config);
                connectors
            },
        };

        let connection_repo = MemoryConnectionStore::new();
        let action_repo = MemoryActionRepository::new();
        let manager = ConfigManager::new();
        let options = ImportOptions::default();

        let result = manager
            .import_to_db(&manifest, &connection_repo, &action_repo, &options)
            .await
            .unwrap();

        assert_eq!(result.connections_created, 1);
        assert_eq!(result.actions_created, 1);
        assert_eq!(result.conflicts.len(), 0);

        // Verify data was imported
        let connection_trn =
            Trn::new("trn:openact:default:connection/http/test-conn@v1".to_string());
        let imported_connection = connection_repo.get(&connection_trn).await.unwrap().unwrap();
        assert_eq!(imported_connection.name, "test-conn");

        let action_trn = Trn::new("trn:openact:default:action/http/test-action@v1".to_string());
        let imported_action = action_repo.get(&action_trn).await.unwrap().unwrap();
        assert_eq!(imported_action.name, "test-action");
        assert!(imported_action.mcp_enabled);
    }

    #[tokio::test]
    async fn test_export_from_db() {
        let connection_repo = MemoryConnectionStore::new();
        let action_repo = MemoryActionRepository::new();

        // Insert test data
        let connection_record = ConnectionRecord {
            trn: Trn::new("trn:openact:default:connection/http/test-conn@v1".to_string()),
            connector: ConnectorKind::new("http"),
            name: "test-conn".to_string(),
            config_json: json!({
                "base_url": "https://api.test.com",
                "auth": { "type": "bearer", "token": "secret-token" }
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        };
        connection_repo.upsert(&connection_record).await.unwrap();

        let action_record = ActionRecord {
            trn: Trn::new("trn:openact:default:action/http/test-action@v1".to_string()),
            connector: ConnectorKind::new("http"),
            name: "test-action".to_string(),
            connection_trn: connection_record.trn.clone(),
            config_json: json!({
                "method": "GET",
                "path": "/test"
            }),
            mcp_enabled: true,
            mcp_overrides: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        };
        action_repo.upsert(&action_record).await.unwrap();

        let manager = ConfigManager::new();
        let options = ExportOptions::default();

        let manifest =
            manager.export_from_db(&connection_repo, &action_repo, &options).await.unwrap();

        assert!(manifest.connectors.contains_key("http"));
        let http_connector = &manifest.connectors["http"];
        assert!(http_connector.connections.contains_key("test-conn"));
        assert!(http_connector.actions.contains_key("test-action"));

        // Check that sensitive data is redacted
        let connection = &http_connector.connections["test-conn"];
        assert_eq!(connection.config["auth"]["token"], json!("***REDACTED***"));
    }

    #[tokio::test]
    async fn test_validation_errors() {
        let manager = ConfigManager::new();

        // Test missing connection reference
        let invalid_manifest = ConfigManifest {
            version: "1.0".to_string(),
            metadata: None,
            connectors: {
                let mut connectors = HashMap::new();
                let mut connector_config =
                    ConnectorConfig { connections: HashMap::new(), actions: HashMap::new() };

                connector_config.actions.insert(
                    "test-action".to_string(),
                    ActionConfig {
                        connection: "nonexistent-conn".to_string(),
                        description: None,
                        mcp_enabled: None,
                        config: json!({
                            "method": "GET",
                            "path": "/test"
                        }),
                        metadata: None,
                    },
                );

                connectors.insert("http".to_string(), connector_config);
                connectors
            },
        };

        let result = manager.validate(&invalid_manifest);
        assert!(matches!(result, Err(ConfigManagerError::ConnectionNotFound { .. })));
    }
}
