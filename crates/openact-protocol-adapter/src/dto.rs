use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Protocol-agnostic tool specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub annotations: Option<JsonValue>,
    pub input_schema: JsonValue,
    #[serde(default)]
    pub output_schema: Option<JsonValue>,
}

/// Protocol-agnostic tool invocation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeRequest {
    pub tool: String,
    #[serde(default)]
    pub tenant: Option<String>,
    /// Arbitrary JSON arguments defined by tool schema
    #[serde(default)]
    pub args: JsonValue,
}

/// Protocol-agnostic invocation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeResult {
    /// Structured content (authoritative)
    pub structured: JsonValue,
    /// Optional text fallback (e.g., pretty JSON)
    #[serde(default)]
    pub text_fallback: Option<String>,
}

/// Protocol-agnostic error model
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
#[error("{code}: {message}")]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub data: Option<JsonValue>,
}

impl ProtocolError {
    pub fn new<C: Into<String>, M: Into<String>>(
        code: C,
        message: M,
        data: Option<JsonValue>,
    ) -> Self {
        Self { code: code.into(), message: message.into(), data }
    }
}
