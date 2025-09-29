//! Configuration import command

use crate::{
    cli::ConflictResolution,
    error::CliResult,
    utils::{validate_file_exists, ColoredOutput},
};
use openact_config::{ConfigManager, ImportOptions, ImportResult};
use openact_config::manager::VersioningStrategy;
use openact_store::sql_store::SqlStore;
use tracing::info;

pub struct ImportCommand;

impl ImportCommand {
    pub async fn run(
        db_path: &str,
        file: &str,
        conflict_resolution: ConflictResolution,
        dry_run: bool,
    ) -> CliResult<()> {
        // Validate input file exists
        validate_file_exists(file)?;

        info!("Importing configuration from: {}", file);

        // Create database connection
        let store = SqlStore::new(db_path).await?;

        // Create config manager
        let config_manager = ConfigManager::new();

        // Load configuration from file
        let manifest = config_manager.load_from_file(file).await?;

        // Validate configuration
        config_manager.validate(&manifest)?;

        // Prepare import options
        let options = ImportOptions {
            dry_run: false,
            force: matches!(conflict_resolution, ConflictResolution::Overwrite),
            validate: true,
            namespace: None,
            versioning: VersioningStrategy::AlwaysBump,
        };

        if dry_run {
            println!("{}", ColoredOutput::info("🔍 Dry run mode - no changes will be made"));

            // TODO: Implement dry run preview
            let connectors: Vec<_> = manifest.connectors.keys().collect();
            println!("Would import {} connector(s): {:?}", connectors.len(), connectors);

            for (connector_name, connector_config) in &manifest.connectors {
                println!("  {} connector:", ColoredOutput::highlight(connector_name));
                println!("    Connections: {}", connector_config.connections.len());
                println!("    Actions: {}", connector_config.actions.len());
            }

            return Ok(());
        }

        // Import to database
        let result = config_manager.import_to_db(&manifest, &store, &store, &options).await?;

        // Display results
        display_import_result(&result)?;

        Ok(())
    }
}

fn display_import_result(result: &ImportResult) -> CliResult<()> {
    println!("{}", ColoredOutput::success("✓ Import completed"));

    println!("Summary:");
    println!(
        "  Connections created: {}",
        ColoredOutput::highlight(&result.connections_created.to_string())
    );
    println!(
        "  Connections updated: {}",
        ColoredOutput::highlight(&result.connections_updated.to_string())
    );
    println!(
        "  Actions created: {}",
        ColoredOutput::highlight(&result.actions_created.to_string())
    );
    println!(
        "  Actions updated: {}",
        ColoredOutput::highlight(&result.actions_updated.to_string())
    );

    // Display conflicts if any
    if !result.conflicts.is_empty() {
        println!("\n{}", ColoredOutput::warning("⚠ Conflicts encountered:"));
        for conflict in &result.conflicts {
            println!(
                "  {} {}: {}",
                ColoredOutput::warning("•"),
                ColoredOutput::highlight(&conflict.resource_type),
                conflict.trn
            );
            println!("    Reason: {}", ColoredOutput::dim(&conflict.message));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::{tempdir, NamedTempFile};

    #[tokio::test]
    async fn test_import_valid_config() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.to_str().unwrap();

        // Create a temporary config file
        let config_file = NamedTempFile::with_suffix(".yaml").unwrap();
        let config_content = r#"
version: "1.0"
metadata:
  name: "test-config"
  description: "Test configuration"
connectors:
  http:
    connections:
      test-api:
        base_url: "https://api.example.com"
        authorization: "api_key"
        auth_parameters:
          api_key_auth_parameters: null
          basic_auth_parameters: null
          oauth_parameters: null
    actions:
      get-user:
        connection: "test-api"
        method: "GET"
        path: "/users/{id}"
"#;
        fs::write(&config_file.path(), config_content).unwrap();

        // Initialize database
        let store = SqlStore::new(db_path_str).await.unwrap();
        store.migrate().await.unwrap();

        // Run import
        let result = ImportCommand::run(
            db_path_str,
            config_file.path().to_str().unwrap(),
            ConflictResolution::Abort,
            false,
        )
        .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_import_dry_run() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db_path_str = db_path.to_str().unwrap();

        // Create a temporary config file
        let config_file = NamedTempFile::with_suffix(".json").unwrap();
        let config_content = json!({
            "version": "1.0",
            "metadata": {
                "name": "test-config"
            },
            "connectors": {
                "http": {
                    "connections": {},
                    "actions": {}
                }
            }
        });
        fs::write(&config_file.path(), serde_json::to_string_pretty(&config_content).unwrap())
            .unwrap();

        // Initialize database
        let store = SqlStore::new(db_path_str).await.unwrap();
        store.migrate().await.unwrap();

        // Run dry run
        let result = ImportCommand::run(
            db_path_str,
            config_file.path().to_str().unwrap(),
            ConflictResolution::Abort,
            true,
        )
        .await;

        assert!(result.is_ok());
    }
}
