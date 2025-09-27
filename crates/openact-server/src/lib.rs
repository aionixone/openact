//! OpenAct Server
//!
//! Provides both MCP and REST API endpoints for OpenAct actions.

pub mod app_state;
pub mod dto;
pub mod error;
pub mod mcp;
pub mod middleware;
pub mod restapi;

// Re-export key types
pub use app_state::AppState;
pub use error::{ServerError, ServerResult};
pub use openact_mcp::GovernanceConfig;

// MCP delegation (for backward compatibility)
pub async fn serve_mcp_stdio(
    app_state: AppState,
    governance: GovernanceConfig,
) -> ServerResult<()> {
    mcp::serve_stdio(app_state, governance).await
}

pub async fn serve_mcp_http(
    app_state: AppState,
    governance: GovernanceConfig,
    addr: &str,
) -> ServerResult<()> {
    mcp::serve_http(app_state, governance, addr).await
}

// REST API
pub async fn serve_rest(
    app_state: AppState,
    governance: GovernanceConfig,
    addr: &str,
) -> ServerResult<()> {
    restapi::serve(app_state, governance, addr).await
}

/// Unified server: run REST and/or MCP endpoints concurrently in one process
pub struct ServeConfig {
    pub rest_addr: Option<String>,
    pub mcp_http_addr: Option<String>,
    pub mcp_stdio: bool,
}

pub async fn serve_unified(
    app_state: AppState,
    governance: GovernanceConfig,
    cfg: ServeConfig,
) -> ServerResult<()> {
    let mut tasks: Vec<tokio::task::JoinHandle<ServerResult<()>>> = Vec::new();
    if let Some(addr) = cfg.rest_addr.clone() {
        let st = app_state.clone();
        let gv = governance.clone();
        tasks.push(tokio::spawn(
            async move { restapi::serve(st, gv, &addr).await },
        ));
    }
    if let Some(addr) = cfg.mcp_http_addr.clone() {
        let st = app_state.clone();
        let gv = governance.clone();
        tasks.push(tokio::spawn(
            async move { mcp::serve_http(st, gv, &addr).await },
        ));
    }
    if cfg.mcp_stdio {
        let st = app_state.clone();
        let gv = governance.clone();
        tasks.push(tokio::spawn(async move { mcp::serve_stdio(st, gv).await }));
    }

    // If no services configured, return error
    if tasks.is_empty() {
        return Err(ServerError::InvalidInput(
            "No services configured to run".to_string(),
        ));
    }

    // Wait for any task to finish with error, or all to complete
    for t in tasks {
        t.await
            .map_err(|e| ServerError::Internal(format!("Join error: {}", e)))??;
    }
    Ok(())
}
