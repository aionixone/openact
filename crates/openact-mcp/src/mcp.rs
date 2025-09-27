//! MCP (Model Context Protocol) types and handlers

use serde::{Deserialize, Serialize};
use serde_json::Value;

// MCP Protocol Versions
pub const PROTOCOL_VERSION_2024_11_05: &str = "2024-11-05";
pub const PROTOCOL_VERSION_2025_03_26: &str = "2025-03-26";
pub const PROTOCOL_VERSION_2025_06_18: &str = "2025-06-18";
pub const LATEST_PROTOCOL_VERSION: &str = PROTOCOL_VERSION_2025_06_18;

pub const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[
    PROTOCOL_VERSION_2024_11_05,
    PROTOCOL_VERSION_2025_03_26,
    PROTOCOL_VERSION_2025_06_18,
];

// MCP Method Names
pub const METHOD_INITIALIZE: &str = "initialize";
pub const METHOD_PING: &str = "ping";
pub const METHOD_TOOLS_LIST: &str = "tools/list";
pub const METHOD_TOOLS_CALL: &str = "tools/call";

/// MCP Initialize Request
#[derive(Debug, Deserialize)]
pub struct InitializeRequest {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: Option<ClientCapabilities>,
    #[serde(rename = "clientInfo")]
    pub client_info: Option<Implementation>,
}

/// MCP Initialize Response
#[derive(Debug, Serialize)]
pub struct InitializeResponse {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: Implementation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

/// Client capabilities
#[derive(Debug, Deserialize)]
pub struct ClientCapabilities {
    #[serde(default)]
    pub experimental: Value,
    #[serde(default)]
    pub sampling: Value,
}

/// Server capabilities  
#[derive(Debug, Serialize)]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
}

#[derive(Debug, Serialize)]
pub struct ToolsCapability {
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ResourcesCapability {
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct PromptsCapability {
    #[serde(rename = "listChanged", skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Implementation info
#[derive(Debug, Serialize, Deserialize)]
pub struct Implementation {
    pub name: String,
    pub version: String,
}

/// Tools List Request
#[derive(Debug, Deserialize)]
pub struct ToolsListRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Tools List Response
#[derive(Debug, Serialize)]
pub struct ToolsListResponse {
    pub tools: Vec<Tool>,
    #[serde(rename = "nextCursor", skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// MCP Tool definition
#[derive(Debug, Serialize)]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Tools Call Request
#[derive(Debug, Deserialize)]
pub struct ToolsCallRequest {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// Tools Call Response
#[derive(Debug, Serialize)]
pub struct ToolsCallResponse {
    pub content: Vec<Content>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Content types for tool responses
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum Content {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

impl Content {
    pub fn text(text: String) -> Self {
        Content::Text { text }
    }

    pub fn image(data: String, mime_type: String) -> Self {
        Content::Image { data, mime_type }
    }
}
