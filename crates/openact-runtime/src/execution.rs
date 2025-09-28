use std::time::Duration;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use openact_registry::{ConnectorRegistry, ExecutionContext};
use openact_core::Trn;
use crate::error::{RuntimeError, RuntimeResult};

/// Options for action execution
#[derive(Debug, Clone)]
pub struct ExecutionOptions {
    /// Timeout for execution
    pub timeout: Option<Duration>,
    /// Dry run mode - validate but don't execute
    pub dry_run: bool,
    /// Tenant ID for execution context
    pub tenant_id: Option<String>,
    /// Additional execution context
    pub context: Option<Value>,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            timeout: Some(Duration::from_secs(30)),
            dry_run: false,
            tenant_id: None,
            context: None,
        }
    }
}

/// Result of action execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    /// Whether execution was successful
    pub success: bool,
    /// Output data from execution
    pub output: Option<Value>,
    /// Error message if execution failed
    pub error: Option<String>,
    /// Execution metadata
    pub metadata: ExecutionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    /// Action TRN that was executed
    pub action_trn: String,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// Whether this was a dry run
    pub dry_run: bool,
    /// Timestamp of execution
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Execute an action using the provided registry
/// This is the unified execution function for all paths
pub async fn execute_action(
    registry: &ConnectorRegistry,
    action_trn: &str,
    input: Value,
    options: ExecutionOptions,
) -> RuntimeResult<ExecutionResult> {
    let start_time = std::time::Instant::now();
    let timestamp = chrono::Utc::now();
    
    tracing::info!(
        action_trn = %action_trn,
        dry_run = options.dry_run,
        "Starting action execution"
    );

    // Convert action_trn to Trn type
    let action_trn_obj = Trn::new(action_trn);
    
    // For dry run, skip existence check and return success
    if options.dry_run {
        tracing::info!(action_trn = %action_trn, "Dry run - validation only");
        return Ok(ExecutionResult {
            success: true,
            output: Some(serde_json::json!({
                "message": "Dry run successful - action would execute",
                "action_trn": action_trn,
                "input_received": input
            })),
            error: None,
            metadata: ExecutionMetadata {
                action_trn: action_trn.to_string(),
                duration_ms: start_time.elapsed().as_millis() as u64,
                dry_run: true,
                timestamp,
            },
        });
    }


    // Execute the action
    let execution_context = ExecutionContext::new();
    let execution_future = async {
        registry.execute(&action_trn_obj, input, Some(execution_context)).await
            .map_err(|e| RuntimeError::execution(e.to_string()))
    };

    // Apply timeout if specified
    let result = if let Some(timeout) = options.timeout {
        match tokio::time::timeout(timeout, execution_future).await {
            Ok(result) => result,
            Err(_) => {
                return Ok(ExecutionResult {
                    success: false,
                    output: None,
                    error: Some("Execution timed out".to_string()),
                    metadata: ExecutionMetadata {
                        action_trn: action_trn.to_string(),
                        duration_ms: start_time.elapsed().as_millis() as u64,
                        dry_run: false,
                        timestamp,
                    },
                });
            }
        }
    } else {
        execution_future.await
    };

    let duration_ms = start_time.elapsed().as_millis() as u64;

    match result {
        Ok(output) => {
            tracing::info!(
                action_trn = %action_trn,
                duration_ms = duration_ms,
                "Action execution completed successfully"
            );
            
            Ok(ExecutionResult {
                success: true,
                output: Some(output.output),
                error: None,
                metadata: ExecutionMetadata {
                    action_trn: action_trn.to_string(),
                    duration_ms,
                    dry_run: false,
                    timestamp,
                },
            })
        }
        Err(e) => {
            tracing::error!(
                action_trn = %action_trn,
                duration_ms = duration_ms,
                error = %e,
                "Action execution failed"
            );
            
            Ok(ExecutionResult {
                success: false,
                output: None,
                error: Some(e.to_string()),
                metadata: ExecutionMetadata {
                    action_trn: action_trn.to_string(),
                    duration_ms,
                    dry_run: false,
                    timestamp,
                },
            })
        }
    }
}
