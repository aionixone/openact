use openact_core::CoreError;

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("Core error: {0}")]
    Core(#[from] CoreError),
    
    #[cfg(feature = "http")]
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    
    #[cfg(any(feature = "postgresql", feature = "mysql"))]
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[cfg(feature = "redis")]
    #[error("Redis error: {0}")]
    Redis(#[from] redis::RedisError),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Authentication failed: {0}")]
    Authentication(String),
    
    #[error("Timeout: {0}")]
    Timeout(String),
    
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Connection error: {0}")]
    Connection(String),
}

// Add conversion from ConnectorError to CoreError
impl From<ConnectorError> for CoreError {
    fn from(err: ConnectorError) -> Self {
        match err {
            ConnectorError::Io(e) => CoreError::Io(e.to_string()),
            ConnectorError::Serialization(e) => CoreError::Serde(e.to_string()),
            ConnectorError::Core(e) => e,
            
            #[cfg(feature = "http")]
            ConnectorError::Http(e) => CoreError::Other(format!("HTTP error: {}", e)),
            
            #[cfg(any(feature = "postgresql", feature = "mysql"))]
            ConnectorError::Database(e) => CoreError::Db(e.to_string()),
            
            #[cfg(feature = "redis")]
            ConnectorError::Redis(e) => CoreError::Other(format!("Redis error: {}", e)),
            
            ConnectorError::InvalidConfig(msg) => CoreError::Invalid(msg),
            ConnectorError::Authentication(msg) => CoreError::Invalid(msg),
            ConnectorError::Timeout(msg) => CoreError::Other(msg),
            ConnectorError::ExecutionFailed(msg) => CoreError::Other(msg),
            ConnectorError::Validation(msg) => CoreError::Invalid(msg),
            ConnectorError::Connection(msg) => CoreError::Other(msg),
        }
    }
}

pub type ConnectorResult<T> = Result<T, ConnectorError>;
