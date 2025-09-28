pub mod error;
pub mod sanitization;
pub mod store;
pub mod types;

// Re-export commonly used types
pub use error::{CoreError, CoreResult};
pub use sanitization::{sanitize_json_value, sanitize_string, create_debug_string, is_sensitive_field};
pub use types::{
    ActionRecord, ActionTrnComponents, AuthConnection, Checkpoint, ConnectionRecord,
    ConnectionTrnComponents, ConnectorKind, McpOverrides, Trn, TrnComponents,
};

// Re-export MCP types
pub use types::{
    McpManifest, McpServerCapabilities, McpServerInfo, McpServerManifest, McpToolsSchema,
    McpToolsetManifest, ParameterManifest, ParameterMcpManifest, ProtocolVersion, ToolboxManifest,
    ToolsetManifest,
};
