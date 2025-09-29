//! MCP module - delegates to openact-mcp

use crate::{AppState, ServerError, ServerResult};
use openact_mcp::GovernanceConfig;

/// Serve MCP over stdio
pub async fn serve_stdio(app_state: AppState, governance: GovernanceConfig) -> ServerResult<()> {
    // Convert AppState to openact_mcp::AppState using Arc directly (no clone of inner SqlStore)
    let mcp_app_state = openact_mcp::AppState::from_arc(app_state.store);
    openact_mcp::serve_stdio(mcp_app_state, governance).await.map_err(ServerError::from)
}

/// Serve MCP over HTTP
pub async fn serve_http(
    app_state: AppState,
    governance: GovernanceConfig,
    addr: &str,
) -> ServerResult<()> {
    // Convert AppState to openact_mcp::AppState using Arc directly (no clone of inner SqlStore)
    let mcp_app_state = openact_mcp::AppState::from_arc(app_state.store);
    openact_mcp::serve_http(mcp_app_state, governance, addr).await.map_err(ServerError::from)
}
