use std::sync::Arc;

use openact_core::{ActionRecord, ConnectionRecord, store::{ConnectionStore, ActionRepository}};
use openact_config::ConfigManifest;
use openact_registry::ConnectorRegistry;
use openact_store::{MemoryConnectionStore, MemoryActionRepository};

use crate::error::{RuntimeError, RuntimeResult};
use crate::helpers::records_from_manifest;

#[cfg(feature = "http")]
use openact_registry::HttpFactory;

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
        connection_store.upsert(&record).await
            .map_err(|e| RuntimeError::registry(format!("Failed to store connection: {}", e)))?;
    }
    
    // Populate action repository  
    for record in action_records {
        action_repository.upsert(&record).await
            .map_err(|e| RuntimeError::registry(format!("Failed to store action: {}", e)))?;
    }
    
    // Build registry with feature flags
    let mut registry = ConnectorRegistry::new(connection_store, action_repository);
    
    // Register connectors based on feature flags
    register_connectors(&mut registry, feature_flags)?;
    
    Ok(registry)
}

/// Build a ConnectorRegistry from a config manifest
/// This handles the file/JSON config path
pub async fn registry_from_manifest(
    manifest: ConfigManifest,
    feature_flags: &[&str],
) -> RuntimeResult<ConnectorRegistry> {
    let (connection_records, action_records) = records_from_manifest(manifest).await?;
    registry_from_records(connection_records, action_records, feature_flags).await
}

/// Register connectors based on feature flags
fn register_connectors(
    registry: &mut ConnectorRegistry,
    feature_flags: &[&str],
) -> RuntimeResult<()> {
    for &feature in feature_flags {
        match feature {
            #[cfg(feature = "http")]
            "http" => {
                let http_factory = Arc::new(HttpFactory::new());
                registry.register_connection_factory(http_factory.clone());
                registry.register_action_factory(http_factory);
                tracing::debug!("Registered HTTP connector");
            }
            _ => {
                tracing::warn!("Unknown feature flag: {}", feature);
            }
        }
    }
    Ok(())
}

/// Helper to get default feature flags based on compilation features
pub fn default_feature_flags() -> Vec<&'static str> {
    let mut flags = Vec::new();
    
    #[cfg(feature = "http")]
    flags.push("http");
    
    flags
}

#[cfg(test)]
mod tests {
    use super::*;
    use openact_core::{ConnectionRecord, ActionRecord, Trn, ConnectorKind};
    use serde_json::json;

    #[tokio::test]
    async fn test_registry_from_empty_records() {
        let registry = registry_from_records(vec![], vec![], &[]).await.unwrap();
        assert!(registry.registered_connectors().is_empty());
    }

    #[tokio::test]
    async fn test_registry_with_http_feature() {
        let connection_records = vec![
            ConnectionRecord {
                trn: Trn::new("test:conn:http1"),
                connector: ConnectorKind::new("http"),
                name: "api".to_string(),
                config_json: json!({"base_url": "https://api.example.com"}),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                version: 1,
            }
        ];
        
        let action_records = vec![
            ActionRecord {
                trn: Trn::new("test:action:get-user"),
                connector: ConnectorKind::new("http"),
                name: "get_user".to_string(),
                connection_trn: Trn::new("test:conn:http1"),
                config_json: json!({"method": "GET", "path": "/users/{id}"}),
                mcp_enabled: true,
                mcp_overrides: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                version: 1,
            }
        ];

        #[cfg(feature = "http")]
        {
            let registry = registry_from_records(
                connection_records, 
                action_records, 
                &["http"]
            ).await.unwrap();
            
            let connectors = registry.registered_connectors();
            assert!(connectors.iter().any(|k| k.as_str() == "http"));
        }
    }
}