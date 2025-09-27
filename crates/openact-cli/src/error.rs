//! Error types for the CLI

use thiserror::Error;

/// CLI-specific errors
#[derive(Debug, Error)]
pub enum CliError {
    #[error("Configuration error: {0}")]
    Config(#[from] openact_config::ConfigError),

    #[error("Registry error: {0}")]
    Registry(#[from] openact_registry::RegistryError),

    #[error("Store error: {0}")]
    Store(#[from] openact_core::error::CoreError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Action not found: {0}")]
    ActionNotFound(String),

    #[error("Connection not found: {0}")]
    ConnectionNotFound(String),

    #[error("Execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Database not initialized. Run 'openact migrate' first.")]
    DatabaseNotInitialized,

    #[error("General error: {0}")]
    General(String),
}

impl From<anyhow::Error> for CliError {
    fn from(err: anyhow::Error) -> Self {
        Self::General(err.to_string())
    }
}

impl From<openact_config::ConfigManagerError> for CliError {
    fn from(err: openact_config::ConfigManagerError) -> Self {
        Self::Config(openact_config::ConfigError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            err.to_string(),
        )))
    }
}

impl From<openact_store::StoreError> for CliError {
    fn from(err: openact_store::StoreError) -> Self {
        Self::Store(openact_core::error::CoreError::Io(err.to_string()))
    }
}

impl From<openact_server::ServerError> for CliError {
    fn from(err: openact_server::ServerError) -> Self {
        Self::General(err.to_string())
    }
}

/// CLI result type
pub type CliResult<T> = Result<T, CliError>;
