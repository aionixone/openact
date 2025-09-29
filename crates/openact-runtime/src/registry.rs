use openact_config::ConfigManifest;
use openact_core::{
    create_debug_string,
    store::{ActionRepository, ConnectionStore},
    ActionRecord, ConnectionRecord,
};
use openact_registry::{ConnectorRegistrar, ConnectorRegistry};
use openact_store::{MemoryActionRepository, MemoryConnectionStore};

use crate::error::{RuntimeError, RuntimeResult};
use crate::helpers::records_from_manifest;

/// Build a ConnectorRegistry from connection and action records
/// This is the core function that all execution paths should use
pub async fn registry_from_records(
    connection_records: Vec<ConnectionRecord>,
    action_records: Vec<ActionRecord>,
    feature_flags: &[&str],
) -> RuntimeResult<ConnectorRegistry> {
    // Create memory stores and populate them
    let connection_store = MemoryConnectionStore::new();
    let action_repository = MemoryActionRepository::new();

    // Populate connection store
    for record in connection_records {
        connection_store
            .upsert(&record)
            .await
            .map_err(|e| RuntimeError::registry(format!("Failed to store connection: {}", e)))?;
    }

    // Populate action repository
    for record in action_records {
        // Sanitize config_json for logging (it's already a JsonValue)
        let sanitized_config = create_debug_string("config_json", &record.config_json);

        tracing::debug!(
            "Storing action record: trn={}, connector={}, name={}, {}",
            record.trn.as_str(),
            record.connector.as_str(),
            record.name,
            sanitized_config
        );
        action_repository
            .upsert(&record)
            .await
            .map_err(|e| RuntimeError::registry(format!("Failed to store action: {}", e)))?;
    }

    // Build registry with feature flags
    let registry = ConnectorRegistry::new(connection_store, action_repository);

    // Legacy feature flags are deprecated; use registry_from_records_ext with plugin registrars instead
    if !feature_flags.is_empty() {
        tracing::warn!(
            "Feature flags {:?} ignored - use registry_from_records_ext with plugin registrars",
            feature_flags
        );
    }

    Ok(registry)
}

/// A plugin registrar is a function that registers connector factories into the registry.
/// This allows connectors to self-register without changing runtime or config code.
pub type PluginRegistrar = ConnectorRegistrar;

/// Build a ConnectorRegistry and register both feature-flag connectors and plugin-registered connectors
pub async fn registry_from_records_ext(
    connection_records: Vec<ConnectionRecord>,
    action_records: Vec<ActionRecord>,
    feature_flags: &[&str],
    plugin_registrars: &[PluginRegistrar],
) -> RuntimeResult<ConnectorRegistry> {
    // Reuse base implementation
    let mut registry =
        registry_from_records(connection_records, action_records, feature_flags).await?;

    // Apply plugin registrars (connectors self-register here)
    for registrar in plugin_registrars {
        (registrar)(&mut registry);
    }

    Ok(registry)
}

/// Build a ConnectorRegistry from a config manifest using plugin registrars
/// This handles the file/JSON config path
pub async fn registry_from_manifest_ext(
    manifest: ConfigManifest,
    plugin_registrars: &[PluginRegistrar],
) -> RuntimeResult<ConnectorRegistry> {
    let (connection_records, action_records) = records_from_manifest(manifest).await?;
    registry_from_records_ext(connection_records, action_records, &[], plugin_registrars).await
}

/// Deprecated: Build a ConnectorRegistry from a config manifest
/// Use registry_from_manifest_ext with plugin registrars instead
pub async fn registry_from_manifest(
    manifest: ConfigManifest,
    feature_flags: &[&str],
) -> RuntimeResult<ConnectorRegistry> {
    let (connection_records, action_records) = records_from_manifest(manifest).await?;
    registry_from_records(connection_records, action_records, feature_flags).await
}

/// Helper for backwards compatibility - returns empty vec as feature flags are deprecated
/// Use registry_from_records_ext with plugin registrars instead
pub fn default_feature_flags() -> Vec<&'static str> {
    tracing::warn!(
        "default_feature_flags() is deprecated - use openact_plugins::registrars() instead"
    );
    Vec::new()
}
