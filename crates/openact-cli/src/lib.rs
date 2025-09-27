pub mod cli;
pub mod commands;
pub mod error;
pub mod utils;

// Re-export commonly used types
pub use cli::{Cli, Commands};
pub use error::{CliError, CliResult};
pub use utils::{init_tracing, ColoredOutput};
