use thiserror::Error;

pub type RuntimeResult<T> = Result<T, RuntimeError>;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Registry error: {0}")]
    Registry(String),
    
    #[error("Execution error: {0}")]
    Execution(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Action not found: {0}")]
    ActionNotFound(String),
    
    #[error("Connection not found: {0}")]
    ConnectionNotFound(String),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Store error: {0}")]
    Store(#[from] openact_store::StoreError),
    
    #[error("Config error: {0}")]
    ConfigError(#[from] openact_config::ConfigError),
}

impl RuntimeError {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }
    
    pub fn registry(msg: impl Into<String>) -> Self {
        Self::Registry(msg.into())
    }
    
    pub fn execution(msg: impl Into<String>) -> Self {
        Self::Execution(msg.into())
    }
    
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }
}
