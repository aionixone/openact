//! Error types for the registry system

use openact_core::{ConnectorKind, Trn};
use thiserror::Error;

/// Registry-specific errors
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("Connector '{0}' is not registered")]
    ConnectorNotRegistered(ConnectorKind),

    #[error("Connection not found: {0}")]
    ConnectionNotFound(Trn),

    #[error("Action not found: {0}")]
    ActionNotFound(Trn),

    #[error("Failed to create connection: {0}")]
    ConnectionCreationFailed(String),

    #[error("Failed to create action: {0}")]
    ActionCreationFailed(String),

    #[error("Action execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Invalid input data: {0}")]
    InvalidInput(String),

    #[error("Store operation failed: {0}")]
    StoreError(String),

    #[error("Serialization/deserialization error: {0}")]
    SerializationError(String),

    #[error("Configuration error: {0}")]
    ConfigurationError(String),
}

impl From<openact_core::error::CoreError> for RegistryError {
    fn from(err: openact_core::error::CoreError) -> Self {
        Self::StoreError(err.to_string())
    }
}

impl From<serde_json::Error> for RegistryError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationError(err.to_string())
    }
}

/// Registry result type
pub type RegistryResult<T> = Result<T, RegistryError>;
