//! Error handling for OpenAct MCP integration

use crate::jsonrpc::JsonRpcError;
use thiserror::Error;

/// Result type for MCP operations
pub type McpResult<T> = Result<T, McpError>;

/// Errors that can occur in MCP operations
#[derive(Debug, Error)]
pub enum McpError {
    #[error("OpenAct core error: {0}")]
    Core(#[from] openact_core::CoreError),

    #[error("Store error: {0}")]
    Store(#[from] openact_store::StoreError),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid tool arguments: {0}")]
    InvalidArguments(String),

    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Execution timeout")]
    Timeout,

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl McpError {
    /// Convert to JSON-RPC error
    pub fn to_jsonrpc_error(&self) -> JsonRpcError {
        match self {
            McpError::InvalidArguments(msg) => {
                JsonRpcError::invalid_params().with_data(serde_json::json!({
                    "message": msg
                }))
            }
            McpError::ToolNotFound(msg) => {
                JsonRpcError::method_not_found().with_data(serde_json::json!({
                    "message": msg
                }))
            }
            McpError::PermissionDenied(msg) => {
                JsonRpcError::invalid_request().with_data(serde_json::json!({
                    "message": format!("Permission denied: {}", msg)
                }))
            }
            McpError::Timeout => JsonRpcError::internal_error().with_data(serde_json::json!({
                "message": "Request timeout"
            })),
            _ => JsonRpcError::internal_error().with_data(serde_json::json!({
                "message": self.to_string()
            })),
        }
    }
}
