//! REST API module

pub mod handlers;
pub mod router;
pub mod services;

pub use router::create_router;

use crate::{AppState, ServerError, ServerResult};
use openact_mcp::GovernanceConfig;
use std::net::SocketAddr;

/// Serve REST API
pub async fn serve(
    app_state: AppState,
    governance: GovernanceConfig,
    addr: &str,
) -> ServerResult<()> {
    let app = create_router(app_state, governance);

    let addr: SocketAddr = addr
        .parse()
        .map_err(|e| ServerError::InvalidInput(format!("Invalid address: {}", e)))?;

    tracing::info!("Starting REST API server on {}", addr);

    axum::serve(
        tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| ServerError::Internal(format!("Failed to bind: {}", e)))?,
        app,
    )
    .await
    .map_err(|e| ServerError::Internal(format!("Server error: {}", e)))?;

    Ok(())
}
