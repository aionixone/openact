//! Action execution command

use crate::{
    cli::OutputFormat,
    error::CliResult,
    utils::{format_duration, parse_trn, validate_file_exists, ColoredOutput},
};
use openact_registry::{ConnectorRegistry, ExecutionContext, ExecutionResult};
use openact_store::sql_store::SqlStore;
use serde_json::{json, Value as JsonValue};
use std::path::Path;
use tracing::{debug, info};

#[cfg(feature = "http")]
use openact_registry::HttpFactory;
#[cfg(feature = "postgresql")]
use openact_registry::PostgresFactory;
use std::sync::Arc;

pub struct ExecuteCommand;

impl ExecuteCommand {
    pub async fn run(
        db_path: &str,
        action_trn: &str,
        input: Option<String>,
        input_file: Option<String>,
        format: OutputFormat,
        output_file: Option<String>,
        show_metadata: bool,
    ) -> CliResult<()> {
        // Parse and validate action TRN
        let trn = parse_trn(action_trn)?;

        info!("Executing action: {}", action_trn);

        // Create database connection
        let store = SqlStore::new(db_path).await?;

        // Create and configure registry
        let mut registry = ConnectorRegistry::new(store.clone(), store.clone());

        // Register HTTP factory if available
        #[cfg(feature = "http")]
        {
            let http_factory = Arc::new(HttpFactory::new());
            registry.register_connection_factory(http_factory.clone());
            registry.register_action_factory(http_factory);
        }

        #[cfg(feature = "postgresql")]
        {
            let pg_factory = Arc::new(PostgresFactory::new());
            registry.register_connection_factory(pg_factory.clone());
            registry.register_action_factory(pg_factory);
        }

        // Parse input data
        let input_data = Self::parse_input(input, input_file).await?;

        debug!(
            "Input data: {}",
            serde_json::to_string(&input_data).unwrap_or_else(|_| "invalid".to_string())
        );

        // Create execution context
        let context = ExecutionContext::new()
            .with_metadata("cli_version".to_string(), json!(env!("CARGO_PKG_VERSION")))
            .with_metadata("action_trn".to_string(), json!(action_trn));

        // Execute action
        let start_time = std::time::Instant::now();
        let result = registry.execute(&trn, input_data, Some(context)).await?;
        let total_duration = start_time.elapsed();

        // Display results
        Self::display_result(&result, total_duration, format, output_file, show_metadata).await?;

        Ok(())
    }

    async fn parse_input(
        input: Option<String>,
        input_file: Option<String>,
    ) -> CliResult<JsonValue> {
        match (input, input_file) {
            (Some(input_str), None) => {
                // Parse input from command line
                serde_json::from_str(&input_str).map_err(|e| {
                    crate::error::CliError::InvalidArgument(format!("Invalid JSON input: {}", e))
                })
            }
            (None, Some(file_path)) => {
                // Read input from file
                validate_file_exists(&file_path)?;
                let content = tokio::fs::read_to_string(&file_path).await?;

                // Try to parse as JSON first, then YAML
                if let Ok(json_data) = serde_json::from_str::<JsonValue>(&content) {
                    Ok(json_data)
                } else {
                    serde_yaml::from_str::<JsonValue>(&content).map_err(|e| {
                        crate::error::CliError::InvalidArgument(format!(
                            "Invalid JSON/YAML input file: {}",
                            e
                        ))
                    })
                }
            }
            (None, None) => {
                // No input provided, use empty object
                Ok(json!({}))
            }
            (Some(_), Some(_)) => {
                // Both provided (should be caught by clap conflicts_with)
                Err(crate::error::CliError::InvalidArgument(
                    "Cannot specify both --input and --input-file".to_string(),
                ))
            }
        }
    }

    async fn display_result(
        result: &ExecutionResult,
        total_duration: std::time::Duration,
        format: OutputFormat,
        output_file: Option<String>,
        show_metadata: bool,
    ) -> CliResult<()> {
        // Prepare output data
        let output_data = if show_metadata {
            json!({
                "output": result.output,
                "metadata": {
                    "execution_id": result.context.execution_id,
                    "total_duration_ms": total_duration.as_millis(),
                    "context_metadata": result.context.metadata,
                    "result_metadata": result.metadata
                }
            })
        } else {
            result.output.clone()
        };

        // Format output
        let formatted_output = match format {
            OutputFormat::Table => {
                // For table format, we'll display a summary and the JSON output
                if show_metadata {
                    Self::display_table_with_metadata(result, total_duration)?
                } else {
                    format.format_json(&result.output)?
                }
            }
            _ => format.format_json(&output_data)?,
        };

        // Output to file or console
        if let Some(ref output_path) = output_file {
            crate::utils::ensure_parent_dir(Path::new(&output_path))?;
            tokio::fs::write(&output_path, &formatted_output).await?;
            println!(
                "{}",
                ColoredOutput::success(&format!("✓ Output saved to: {}", output_path))
            );
        } else {
            println!("{}", formatted_output);
        }

        // Display execution summary (only to console, not to file)
        if output_file.is_none() && format == OutputFormat::Table {
            Self::display_execution_summary(result, total_duration);
        }

        Ok(())
    }

    fn display_table_with_metadata(
        result: &ExecutionResult,
        total_duration: std::time::Duration,
    ) -> CliResult<String> {
        let mut output = String::new();

        // Execution summary
        output.push_str(&format!(
            "Execution ID: {}\n",
            ColoredOutput::highlight(&result.context.execution_id)
        ));
        output.push_str(&format!(
            "Duration: {}\n",
            ColoredOutput::info(&format_duration(total_duration))
        ));
        output.push_str("\n");

        // Result metadata
        if !result.metadata.is_empty() {
            output.push_str(&format!("{}\n", ColoredOutput::highlight("Metadata:")));
            for (key, value) in &result.metadata {
                output.push_str(&format!("  {}: {}\n", key, value));
            }
            output.push_str("\n");
        }

        // Output data
        output.push_str(&format!("{}\n", ColoredOutput::highlight("Output:")));
        output.push_str(&serde_json::to_string_pretty(&result.output)?);

        Ok(output)
    }

    fn display_execution_summary(result: &ExecutionResult, total_duration: std::time::Duration) {
        println!(
            "\n{}",
            ColoredOutput::success("✓ Action executed successfully")
        );
        println!(
            "Execution ID: {}",
            ColoredOutput::dim(&result.context.execution_id)
        );
        println!(
            "Total Duration: {}",
            ColoredOutput::info(&format_duration(total_duration))
        );

        // Show result metadata if available
        if let Some(duration_ms) = result.metadata.get("duration_ms") {
            if let Some(ms) = duration_ms.as_u64() {
                let action_duration = std::time::Duration::from_millis(ms);
                println!(
                    "Action Duration: {}",
                    ColoredOutput::info(&format_duration(action_duration))
                );
            }
        }

        if let Some(status_code) = result.metadata.get("status_code") {
            println!(
                "Status Code: {}",
                ColoredOutput::highlight(&status_code.to_string())
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_parse_input_json_string() {
        let input = r#"{"key": "value"}"#;
        let result = ExecuteCommand::parse_input(Some(input.to_string()), None)
            .await
            .unwrap();
        assert_eq!(result, json!({"key": "value"}));
    }

    #[tokio::test]
    async fn test_parse_input_from_file() {
        let temp_file = NamedTempFile::with_suffix(".json").unwrap();
        let content = r#"{"test": true}"#;
        fs::write(&temp_file.path(), content).unwrap();

        let result =
            ExecuteCommand::parse_input(None, Some(temp_file.path().to_str().unwrap().to_string()))
                .await
                .unwrap();

        assert_eq!(result, json!({"test": true}));
    }

    #[tokio::test]
    async fn test_parse_input_yaml_file() {
        let temp_file = NamedTempFile::with_suffix(".yaml").unwrap();
        let content = "test: true\nvalue: 42";
        fs::write(&temp_file.path(), content).unwrap();

        let result =
            ExecuteCommand::parse_input(None, Some(temp_file.path().to_str().unwrap().to_string()))
                .await
                .unwrap();

        assert_eq!(result, json!({"test": true, "value": 42}));
    }

    #[tokio::test]
    async fn test_parse_input_empty() {
        let result = ExecuteCommand::parse_input(None, None).await.unwrap();
        assert_eq!(result, json!({}));
    }

    #[tokio::test]
    async fn test_parse_input_invalid_json() {
        let result = ExecuteCommand::parse_input(Some("invalid json".to_string()), None).await;
        assert!(result.is_err());
    }
}
