use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ApiError {
    #[serde(rename = "error_code")]
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hints: Option<Vec<String>>,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
            hints: None,
        }
    }
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
    pub fn with_hints<T: Into<String>>(mut self, hints: impl IntoIterator<Item = T>) -> Self {
        self.hints = Some(hints.into_iter().map(Into::into).collect());
        self
    }
}

#[cfg(feature = "server")]
impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        use axum::{Json, http::StatusCode};
        let status = match self.code.as_str() {
            code if code.starts_with("validation.") => StatusCode::BAD_REQUEST,
            code if code.starts_with("not_found.") => StatusCode::NOT_FOUND,
            code if code.starts_with("conflict.") => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(self)).into_response()
    }
}

/// Error helper functions for consistent error responses
pub mod helpers {
    use super::ApiError;

    pub fn validation_error(subtype: &str, message: impl Into<String>) -> ApiError {
        ApiError::new(format!("validation.{}", subtype), message).with_hints([
            "Check required fields",
            "Verify parameter formats",
            "See API docs for this endpoint",
        ])
    }

    pub fn not_found_error(resource: &str) -> ApiError {
        ApiError::new(format!("not_found.{}", resource), "not found").with_hints([
            "Verify the identifier (TRN) is correct",
            "List resources to confirm existence",
            "Check tenant/context",
        ])
    }

    pub fn storage_error(message: impl Into<String>) -> ApiError {
        ApiError::new("internal.storage_error", message).with_hints([
            "Retry later",
            "Check database connectivity",
            "Inspect server logs for details",
        ])
    }

    pub fn execution_error(message: impl Into<String>) -> ApiError {
        ApiError::new("internal.execution_failed", message).with_hints([
            "Retry the request",
            "Check network/endpoint availability",
            "Validate authentication/authorization",
        ])
    }
}
