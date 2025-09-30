//! Server error types

use axum::{http::StatusCode, response::Json};
use serde::Serialize;

pub type ServerResult<T> = Result<T, ServerError>;

/// Server error enum
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Execution timeout")]
    Timeout,

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Rate limited")]
    RateLimit,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Upstream error: {0}")]
    Upstream(String),

    #[error("MCP error: {0}")]
    Mcp(#[from] openact_mcp::McpError),
}

/// Error response DTO
#[derive(Serialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub error: ErrorDetails,
    pub metadata: super::dto::ResponseMeta,
}

#[derive(Serialize)]
pub struct ErrorDetails {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ServerError {
    pub fn to_http_response(&self, request_id: String) -> (StatusCode, Json<ErrorResponse>) {
        let (status, code, message) = match self {
            ServerError::InvalidInput(msg) => {
                (StatusCode::BAD_REQUEST, "INVALID_INPUT", msg.clone())
            }
            ServerError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone()),
            ServerError::Timeout => {
                (StatusCode::REQUEST_TIMEOUT, "TIMEOUT", "Request timeout".into())
            }
            ServerError::Forbidden(msg) => (StatusCode::FORBIDDEN, "FORBIDDEN", msg.clone()),
            ServerError::RateLimit => {
                (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMITED", "Rate limit exceeded".into())
            }
            ServerError::Internal(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", msg.clone())
            }
            ServerError::Upstream(msg) => (StatusCode::BAD_GATEWAY, "UPSTREAM_ERROR", msg.clone()),
            ServerError::Mcp(e) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", e.to_string()),
        };

        let response = ErrorResponse {
            success: false,
            error: ErrorDetails { code: code.to_string(), message, details: None },
            metadata: super::dto::ResponseMeta {
                request_id,
                tenant: None,
                execution_time_ms: None,
                action_trn: None,
                version: None,
                warnings: None,
            },
        };

        (status, Json(response))
    }
}
