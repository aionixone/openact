//! Execute action from configuration file

use crate::cli::OutputFormat;
use crate::error::{CliError, CliResult};
use crate::utils::{read_input_data, write_output_data, ColoredOutput};
use openact_config::{ConfigLoader, ConfigManager};
use openact_runtime::{
    execute_action, records_from_manifest, registry_from_records_ext, ExecutionOptions,
};
use std::path::Path;
use std::time::Duration;
use tracing::{info, warn};

pub async fn execute(
    config_file: &str,
    action_name: &str,
    input: Option<String>,
    input_file: Option<String>,
    format: OutputFormat,
    output: Option<String>,
    show_metadata: bool,
    dry_run: bool,
    timeout: u64,
) -> CliResult<()> {
    info!("Loading configuration from file: {}", config_file);

    // Load configuration file
    let config_path = Path::new(config_file);
    if !config_path.exists() {
        return Err(CliError::ConfigError(format!(
            "Configuration file not found: {}",
            config_file
        )));
    }

    let loader = ConfigLoader::new("cli");
    let manifest = loader
        .load_from_file(config_path)
        .await
        .map_err(|e| CliError::ConfigError(format!("Failed to load config: {}", e)))?;

    // Validate configuration
    let config_manager = ConfigManager::new();
    config_manager
        .validate(&manifest)
        .map_err(|e| CliError::ConfigError(format!("Invalid configuration: {}", e)))?;

    info!("Configuration loaded successfully");

    // Convert manifest to records to get the correct TRNs
    let (connection_records, action_records) = records_from_manifest(manifest.clone())
        .await
        .map_err(|e| CliError::RuntimeError(format!("Failed to convert manifest: {}", e)))?;

    // Find the action in the action records and get its TRN
    let action_trn = action_records
        .iter()
        .find(|record| record.name == action_name)
        .ok_or_else(|| {
            CliError::ActionNotFound(format!(
                "Action '{}' not found in configuration file",
                action_name
            ))
        })?
        .trn
        .clone();

    info!("Found action: {}", action_trn.as_str());

    // Build registry from records using plugin registrars
    let registry = registry_from_records_ext(
        connection_records,
        action_records,
        &[],
        &openact_plugins::registrars(),
    )
    .await
    .map_err(|e| CliError::RuntimeError(format!("Failed to build registry: {}", e)))?;

    info!("Registry built with {} connector plugins", openact_plugins::registrars().len());

    // Parse input data
    let input_data = read_input_data(input, input_file)?;

    // Set up execution options
    let execution_options = ExecutionOptions {
        timeout: Some(Duration::from_secs(timeout)),
        dry_run,
        tenant_id: Some("cli".to_string()),
        context: None,
    };

    // Execute the action
    info!("Executing action: {}", action_trn);
    if dry_run {
        info!("Running in dry-run mode");
    }

    let result = execute_action(&registry, action_trn.as_str(), input_data, execution_options)
        .await
        .map_err(|e| CliError::RuntimeError(format!("Execution failed: {}", e)))?;

    // Handle result
    if result.success {
        info!("Action executed successfully");

        let output_data = if show_metadata {
            serde_json::json!({
                "success": result.success,
                "output": result.output,
                "metadata": result.metadata
            })
        } else {
            result.output.unwrap_or(serde_json::json!({}))
        };

        let formatted_output = format
            .format_json(&output_data)
            .map_err(|e| CliError::SerializationError(e.to_string()))?;

        if let Some(output_file) = output {
            write_output_data(&output_file, &formatted_output)?;
            println!("{} Output written to: {}", ColoredOutput::success("✓"), output_file);
        } else {
            println!("{}", formatted_output);
        }
    } else {
        let error_msg = result.error.unwrap_or_else(|| "Unknown error".to_string());
        warn!("Action execution failed: {}", error_msg);

        if show_metadata {
            let error_data = serde_json::json!({
                "success": result.success,
                "error": error_msg,
                "metadata": result.metadata
            });
            let formatted_output = format
                .format_json(&error_data)
                .map_err(|e| CliError::SerializationError(e.to_string()))?;
            println!("{}", formatted_output);
        } else {
            println!("{} {}", ColoredOutput::error("Error:"), error_msg);
        }

        return Err(CliError::ExecutionError(error_msg));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_execute_file_missing_config() {
        let result = execute(
            "nonexistent.yaml",
            "test-action",
            None,
            None,
            OutputFormat::Json,
            None,
            false,
            false,
            30,
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_file_invalid_config() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "invalid yaml content [").unwrap();

        let result = execute(
            temp_file.path().to_str().unwrap(),
            "test-action",
            None,
            None,
            OutputFormat::Json,
            None,
            false,
            false,
            30,
        )
        .await;

        assert!(result.is_err());
    }
}
