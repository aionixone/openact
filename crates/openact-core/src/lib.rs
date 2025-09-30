pub mod error;
pub mod sanitization;
pub mod store;
pub mod types;
pub mod resolve;
pub mod policy;
pub mod stream;

// Re-export commonly used types
pub use error::{CoreError, CoreResult};
pub use sanitization::{
    create_debug_string, is_sensitive_field, sanitize_json_value, sanitize_string,
};
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
