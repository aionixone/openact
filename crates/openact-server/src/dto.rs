//! REST API DTOs

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Response envelope wrapper
#[derive(Serialize)]
pub struct ResponseEnvelope<T> {
    pub success: bool,
    pub data: T,
    pub metadata: ResponseMeta,
}

/// Response metadata
#[derive(Serialize)]
pub struct ResponseMeta {
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_trn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

/// List query parameters
#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub connection: Option<String>,
    #[serde(default)]
    pub name_prefix: Option<String>,
    // RFC3339 timestamps
    #[serde(default)]
    pub created_after: Option<String>,
    #[serde(default)]
    pub created_before: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page() -> u32 {
    1
}
fn default_page_size() -> u32 {
    50
}

/// Pagination info
#[derive(Serialize)]
pub struct Pagination {
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

/// Kind summary
#[derive(Serialize)]
pub struct KindSummary {
    pub name: String,
    pub description: String,
    pub category: String,
}

/// Action summary
#[derive(Serialize)]
pub struct ActionSummary {
    pub name: String,
    pub connector: String,
    pub connection: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub action_trn: String,
    pub mcp_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema_digest: Option<String>,
}

/// Action schema response
#[derive(Serialize)]
pub struct ActionSchemaResponse {
    pub input_schema: Value,
    pub output_schema: Value,
    pub examples: Vec<Example>,
}

/// Example for action usage
#[derive(Serialize)]
pub struct Example {
    pub name: String,
    pub input: Value,
}

/// Execute request
#[derive(Deserialize)]
pub struct ExecuteRequest {
    pub input: Value,
    #[serde(default)]
    pub options: Option<ExecuteOptions>,
}

/// Execute options
#[derive(Deserialize)]
pub struct ExecuteOptions {
    pub timeout_ms: Option<u64>,
    pub dry_run: Option<bool>,
    pub validate: Option<bool>,
}

/// Execute response
#[derive(Serialize)]
pub struct ExecuteResponse {
    pub result: Value,
}

/// Stepflow command execution response payload
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepflowCommandResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_ttl: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

// Inline execution DTOs
#[derive(Deserialize)]
pub struct ExecuteInlineRequest {
    /// Action name to execute (must exist in provided actions list)
    pub action: String,
    /// Optional tenant context for this inline execution (overrides header)
    #[serde(default)]
    pub tenant: Option<String>,
    /// Inline connection definitions (JSON array of objects)
    #[serde(default)]
    pub connections: Option<Vec<Value>>,
    /// Inline action definitions (JSON array of objects)
    #[serde(default)]
    pub actions: Option<Vec<Value>>,
    /// Input payload for the action
    pub input: Value,
    /// Optional options: timeout_ms, dry_run
    #[serde(default)]
    pub options: Option<ExecuteOptions>,
}
