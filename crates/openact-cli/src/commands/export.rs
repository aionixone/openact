//! Configuration export command

use crate::{
    cli::OutputFormat,
    error::CliResult,
    utils::{ensure_parent_dir, ColoredOutput},
};
use openact_config::{ConfigManager, ExportOptions};
use openact_core::ConnectorKind;
use openact_store::sql_store::SqlStore;
use std::path::Path;
use tracing::info;

pub struct ExportCommand;

impl ExportCommand {
    pub async fn run(
        db_path: &str,
        file: &str,
        connectors: Vec<String>,
        include_sensitive: bool,
        pretty: bool,
    ) -> CliResult<()> {
        info!("Exporting configuration to: {}", file);

        // Create database connection
        let store = SqlStore::new(db_path).await?;

        // Create config manager
        let config_manager = ConfigManager::new();

        // Convert connector strings to ConnectorKind
        let connector_kinds: Vec<ConnectorKind> =
            connectors.into_iter().map(ConnectorKind::new).collect();

        // Prepare export options
        let options = ExportOptions {
            connectors: connector_kinds,
            include_sensitive,
            resolve_env_vars: false,
        };

        // Export from database
        let manifest = config_manager
            .export_from_db(&store, &store, &options)
            .await?;

        // Determine output format from file extension
        let output_format = Self::determine_format(file, pretty);

        // Format the manifest
        let content = match output_format {
            OutputFormat::Yaml => serde_yaml::to_string(&manifest)?,
            OutputFormat::Pretty => serde_json::to_string_pretty(&manifest)?,
            OutputFormat::Json => serde_json::to_string(&manifest)?,
            OutputFormat::Table => {
                return Err(crate::error::CliError::InvalidArgument(
                    "Table format not supported for export".to_string(),
                ));
            }
        };

        // Ensure parent directory exists
        ensure_parent_dir(Path::new(file))?;

        // Write to file
        std::fs::write(file, content)?;

        // Display summary
        let connector_count = manifest.connectors.len();
        let mut connection_count = 0;
        let mut action_count = 0;

        for connector_config in manifest.connectors.values() {
            connection_count += connector_config.connections.len();
            action_count += connector_config.actions.len();
        }

        println!("{}", ColoredOutput::success("✓ Export completed"));
        println!("Output file: {}", ColoredOutput::highlight(file));
        println!("Summary:");
        println!(
            "  Connectors: {}",
            ColoredOutput::highlight(&connector_count.to_string())
        );
        println!(
            "  Connections: {}",
            ColoredOutput::highlight(&connection_count.to_string())
        );
        println!(
            "  Actions: {}",
            ColoredOutput::highlight(&action_count.to_string())
        );

        if !include_sensitive {
            println!(
                "  {}",
                ColoredOutput::warning("⚠ Sensitive data was redacted")
            );
        }

        Ok(())
    }

    fn determine_format(file: &str, pretty: bool) -> OutputFormat {
        let path = Path::new(file);
        let extension = path.extension().and_then(|ext| ext.to_str());

        match extension {
            Some("yaml") | Some("yml") => OutputFormat::Yaml,
            Some("json") => {
                if pretty {
                    OutputFormat::Pretty
                } else {
                    OutputFormat::Json
                }
            }
            _ => {
                // Default to pretty JSON if extension is unclear
                if pretty {
                    OutputFormat::Pretty
                } else {
                    OutputFormat::Json
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use openact_core::store::ConnectionStore;
    use openact_core::ConnectionRecord;
    use serde_json::json;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_export_to_yaml() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let output_path = temp_dir.path().join("export.yaml");

        // Initialize database with some data
        let store = SqlStore::new(db_path.to_str().unwrap()).await.unwrap();
        store.migrate().await.unwrap();

        // Add test data
        let connection = ConnectionRecord {
            trn: openact_core::Trn::new("trn:openact:test:connection/http/test"),
            connector: ConnectorKind::new("http"),
            name: "test".to_string(),
            config_json: json!({
                "base_url": "https://api.example.com",
                "authorization": "api_key",
                "auth_parameters": {
                    "api_key_auth_parameters": null,
                    "basic_auth_parameters": null,
                    "oauth_parameters": null
                }
            }),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        };
        ConnectionStore::upsert(&store, &connection).await.unwrap();

        // Run export
        let result = ExportCommand::run(
            db_path.to_str().unwrap(),
            output_path.to_str().unwrap(),
            vec!["http".to_string()],
            false,
            true,
        )
        .await;

        assert!(result.is_ok());
        assert!(output_path.exists());

        // Verify content is valid YAML
        let content = std::fs::read_to_string(&output_path).unwrap();
        let _parsed: serde_yaml::Value = serde_yaml::from_str(&content).unwrap();
    }

    #[tokio::test]
    async fn test_export_to_json() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let output_path = temp_dir.path().join("export.json");

        // Initialize database
        let store = SqlStore::new(db_path.to_str().unwrap()).await.unwrap();
        store.migrate().await.unwrap();

        // Run export
        let result = ExportCommand::run(
            db_path.to_str().unwrap(),
            output_path.to_str().unwrap(),
            vec![], // Export all connectors
            true,   // Include sensitive data
            false,  // Compact JSON
        )
        .await;

        assert!(result.is_ok());
        assert!(output_path.exists());

        // Verify content is valid JSON
        let content = std::fs::read_to_string(&output_path).unwrap();
        let _parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    }
}
