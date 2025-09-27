//! MCP server commands

use anyhow::Result;
use clap::Args;
use tracing::info;

use openact_server::{serve_mcp_http, serve_mcp_stdio, AppState, GovernanceConfig};

#[derive(Args)]
pub struct ServeMcpArgs {
    /// Enable stdio transport
    #[arg(long)]
    pub stdio: bool,

    /// Enable HTTP transport
    #[arg(long)]
    pub http: Option<String>,

    /// Allow specific tools (patterns supported, e.g., "http.*", "http.get")
    #[arg(
        long,
        help = "Allow specific tools (patterns supported, e.g., 'http.*', 'http.get')"
    )]
    pub allow: Vec<String>,

    /// Deny specific tools (patterns supported, e.g., "http.*", "http.post")
    #[arg(
        long,
        help = "Deny specific tools (patterns supported, e.g., 'http.*', 'http.post')"
    )]
    pub deny: Vec<String>,

    /// Maximum concurrent tool executions
    #[arg(
        long,
        default_value = "10",
        help = "Maximum concurrent tool executions"
    )]
    pub max_concurrency: usize,

    /// Tool execution timeout in seconds
    #[arg(long, default_value = "30", help = "Tool execution timeout in seconds")]
    pub timeout_secs: u64,
}

pub async fn execute(args: ServeMcpArgs, db_path: &str) -> Result<()> {
    info!("Starting OpenAct MCP server");
    info!("Database: {}", db_path);

    // Create governance configuration
    let governance = GovernanceConfig::new(
        args.allow,
        args.deny,
        args.max_concurrency,
        args.timeout_secs,
    );

    // Create app state
    let app_state = AppState::from_db_path(db_path).await?;

    if let Some(addr) = args.http {
        info!("Starting HTTP MCP server on {}", addr);
        serve_mcp_http(app_state, governance, &addr).await?;
    } else if args.stdio {
        info!("Starting stdio MCP server");
        serve_mcp_stdio(app_state, governance).await?;
    } else {
        // Default to stdio if neither is specified
        info!("No transport specified, defaulting to stdio");
        serve_mcp_stdio(app_state, governance).await?;
    }

    Ok(())
}
