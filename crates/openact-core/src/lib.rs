pub mod error;
pub mod store;
pub mod types;

// Re-export commonly used types
pub use error::{CoreError, CoreResult};
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
