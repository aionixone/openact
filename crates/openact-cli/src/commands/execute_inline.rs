//! Execute action from inline configuration

use crate::cli::OutputFormat;
use crate::error::{CliError, CliResult};
use crate::utils::{read_input_data, write_output_data, ColoredOutput};
use openact_runtime::{
    execute_action, records_from_inline_config, registry_from_records_ext, ExecutionOptions,
};
use serde_json::Value as JsonValue;
use std::time::Duration;
use tracing::{info, warn};

pub async fn execute(
    config_json: &str,
    action_name: &str,
    input: Option<String>,
    input_file: Option<String>,
    format: OutputFormat,
    output: Option<String>,
    show_metadata: bool,
    dry_run: bool,
    timeout: u64,
) -> CliResult<()> {
    info!("Parsing inline configuration");

    // Parse inline configuration
    let config_value: JsonValue = serde_json::from_str(config_json)
        .map_err(|e| CliError::ConfigError(format!("Invalid JSON configuration: {}", e)))?;

    // Extract connections and actions
    let connections = config_value
        .get("connections")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().cloned().collect::<Vec<_>>());

    let actions = config_value
        .get("actions")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().cloned().collect::<Vec<_>>());

    if connections.is_none() && actions.is_none() {
        return Err(CliError::ConfigError(
            "Configuration must contain 'connections' and/or 'actions' arrays".to_string(),
        ));
    }

    // Convert to records
    let (connection_records, action_records) = records_from_inline_config(connections, actions)
        .map_err(|e| CliError::RuntimeError(format!("Failed to parse configuration: {}", e)))?;

    info!("Parsed {} connections and {} actions", connection_records.len(), action_records.len());

    // Find the requested action and get its TRN
    let action_trn = action_records
        .iter()
        .find(|record| record.name == action_name)
        .ok_or_else(|| {
            CliError::ActionNotFound(format!("Action '{}' not found in configuration", action_name))
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
    use serde_json::json;

    #[tokio::test]
    async fn test_execute_inline_invalid_json() {
        let result = execute(
            "invalid json",
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
    async fn test_execute_inline_missing_action() {
        let config = json!({
            "connections": [],
            "actions": [
                {
                    "trn": "test:action:other",
                    "connector": "http",
                    "name": "other",
                    "connection_trn": "test:conn:api"
                }
            ]
        });

        let result = execute(
            &config.to_string(),
            "missing-action",
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
    async fn test_execute_inline_empty_config() {
        let result =
            execute("{}", "test-action", None, None, OutputFormat::Json, None, false, false, 30)
                .await;

        assert!(result.is_err());
    }
}
