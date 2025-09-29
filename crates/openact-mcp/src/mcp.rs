//! MCP (Model Context Protocol) types and handlers

// Re-export key MCP types from generated crate for server use
pub use openact_mcp_types::{
    CallToolRequestParams as ToolsCallRequest,
    CallToolResult as ToolsCallResponse,
    ClientCapabilities,
    ContentBlock,
    InitializeRequestParams as InitializeRequest,
    InitializeResult as InitializeResponse,
    Implementation,
    ListToolsRequestParams as ToolsListRequest,
    ListToolsResult as ToolsListResponse,
    ServerCapabilities,
    ServerCapabilitiesPrompts as PromptsCapability,
    ServerCapabilitiesResources as ResourcesCapability,
    ServerCapabilitiesTools as ToolsCapability,
    TextContent,
    Tool,
    ToolAnnotations,
    ToolInputSchema,
    ToolOutputSchema,
};

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

// Type definitions are imported from openact-mcp-types

// Note: Content types come from openact-mcp-types as ContentBlock and specific structs
