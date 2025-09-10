use anyhow::Result;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize the logging system
pub fn init_logger(verbose: bool) -> Result<()> {
    let env_filter = if verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_target(false).with_thread_ids(true))
        .init();

    Ok(())
}

/// Initialize logger for testing (quieter output)
#[cfg(test)]
pub fn init_test_logger() -> Result<()> {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_test_writer())
        .init();

    Ok(())
} 