//! OpenAct MCP (Model Context Protocol) Server
//!
//! This crate provides MCP server capabilities for OpenAct, allowing OpenAct actions
//! to be exposed as MCP tools. Implementation follows the same pattern as the Go reference.

pub mod adapter;
pub mod app_state;
pub mod error;
pub mod governance;
pub mod jsonrpc;
pub mod mcp;
pub mod server;
pub mod server_rmcp;

// Re-export key types
pub use app_state::AppState;
pub use error::{McpError, McpResult};
pub use governance::GovernanceConfig;
pub use server::McpServer;

pub use server::{serve_http, serve_stdio};
pub use server_rmcp::serve_stdio_rmcp;
