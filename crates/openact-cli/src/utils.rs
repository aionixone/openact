//! Utility functions for the CLI

use crate::error::CliResult;
use colored::{ColoredString, Colorize};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// Initialize tracing with proper filtering
pub fn init_tracing() -> CliResult<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .finish();

    tracing::subscriber::set_global_default(subscriber).map_err(|e| {
        crate::error::CliError::General(format!("Failed to set tracing subscriber: {}", e))
    })?;

    Ok(())
}

/// Utility for colored console output
pub struct ColoredOutput;

impl ColoredOutput {
    pub fn success(msg: &str) -> ColoredString {
        msg.green().bold()
    }

    pub fn error(msg: &str) -> ColoredString {
        msg.red().bold()
    }

    pub fn warning(msg: &str) -> ColoredString {
        msg.yellow().bold()
    }

    pub fn info(msg: &str) -> ColoredString {
        msg.blue()
    }

    pub fn dim(msg: &str) -> ColoredString {
        msg.dimmed()
    }

    pub fn highlight(msg: &str) -> ColoredString {
        msg.cyan().bold()
    }
}

/// Format duration in a human-readable way
pub fn format_duration(duration: std::time::Duration) -> String {
    let ms = duration.as_millis();
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{:.1}m", ms as f64 / 60_000.0)
    }
}

/// Truncate text to a maximum length with ellipsis
pub fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len.saturating_sub(3)])
    }
}

/// Parse TRN from string, validating format
pub fn parse_trn(trn_str: &str) -> CliResult<openact_core::Trn> {
    if !trn_str.starts_with("trn:") {
        return Err(crate::error::CliError::InvalidArgument(format!(
            "Invalid TRN format: '{}'. TRN must start with 'trn:'",
            trn_str
        )));
    }
    Ok(openact_core::Trn::new(trn_str))
}

/// Validate file exists and is readable
pub fn validate_file_exists(path: &str) -> CliResult<()> {
    if !std::path::Path::new(path).exists() {
        return Err(crate::error::CliError::FileNotFound(path.to_string()));
    }
    Ok(())
}

/// Create parent directories if they don't exist
pub fn ensure_parent_dir(path: &std::path::Path) -> CliResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}
