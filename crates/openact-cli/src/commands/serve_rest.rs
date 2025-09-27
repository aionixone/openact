//! REST API server command

use crate::error::CliResult;
use clap::Args;
use openact_mcp::GovernanceConfig;
use openact_server::AppState;

/// Arguments for the serve-rest command
#[derive(Debug, Args)]
pub struct ServeRestArgs {
    /// Host and port to bind to
    #[arg(long, default_value = "127.0.0.1:8080")]
    pub addr: String,

    /// Allow patterns for action filtering (multiple allowed)
    #[arg(long = "allow")]
    pub allow_patterns: Vec<String>,

    /// Deny patterns for action filtering (multiple allowed)
    #[arg(long = "deny")]
    pub deny_patterns: Vec<String>,

    /// Maximum concurrent action executions
    #[arg(long, default_value = "10")]
    pub max_concurrency: usize,

    /// Timeout for action executions in seconds
    #[arg(long, default_value = "30")]
    pub timeout_secs: u64,
}

/// Execute the serve-rest command
pub async fn execute(args: ServeRestArgs, db_path: &str) -> CliResult<()> {
    tracing::info!("Starting REST API server on {}", args.addr);
    tracing::info!("Database: {}", db_path);

    // Create app state from database path
    let app_state = AppState::from_db_path(db_path).await?;

    // Build governance config
    let governance = GovernanceConfig::new(
        args.allow_patterns,
        args.deny_patterns,
        args.max_concurrency,
        args.timeout_secs,
    );

    // Start REST API server
    openact_server::serve_rest(app_state, governance, &args.addr).await?;

    Ok(())
}
