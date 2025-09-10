#[derive(Debug, thiserror::Error)]
pub enum OpenApiToolError {
    #[error("Parsing error: {0}")]
    ParseError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Execution error: {0}")]
    ExecutionError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("I/O error: {0}")]
    IoError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("API call error: {0}")]
    ApiCallError(String),
    
    #[error("Callback error: {0}")]
    CallbackError(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Initialization error: {0}")]
    InitializationError(String),
}

impl From<std::io::Error> for OpenApiToolError {
    fn from(err: std::io::Error) -> Self {
        OpenApiToolError::IoError(err.to_string())
    }
}

impl From<reqwest::Error> for OpenApiToolError {
    fn from(err: reqwest::Error) -> Self {
        OpenApiToolError::NetworkError(err.to_string())
    }
}

impl From<serde_json::Error> for OpenApiToolError {
    fn from(err: serde_json::Error) -> Self {
        OpenApiToolError::ParseError(err.to_string())
    }
}

impl From<serde_yaml::Error> for OpenApiToolError {
    fn from(err: serde_yaml::Error) -> Self {
        OpenApiToolError::ParseError(err.to_string())
    }
}

impl From<sqlx::Error> for OpenApiToolError {
    fn from(err: sqlx::Error) -> Self {
        OpenApiToolError::DatabaseError(err.to_string())
    }
}

// Result type alias for convenience
pub type Result<T> = std::result::Result<T, OpenApiToolError>;

// Helper functions for creating common errors
impl OpenApiToolError {
    /// Shortcut method to create a parsing error
    pub fn parse<S: Into<String>>(msg: S) -> Self {
        OpenApiToolError::ParseError(msg.into())
    }

    /// Shortcut method to create a validation error
    pub fn validation<S: Into<String>>(msg: S) -> Self {
        OpenApiToolError::ValidationError(msg.into())
    }

    /// Shortcut method to create a database error
    pub fn database<S: Into<String>>(msg: S) -> Self {
        OpenApiToolError::DatabaseError(msg.into())
    }

    /// Shortcut method to create an execution error
    pub fn execution<S: Into<String>>(msg: S) -> Self {
        OpenApiToolError::ExecutionError(msg.into())
    }

    /// Shortcut method to create a configuration error
    pub fn config<S: Into<String>>(msg: S) -> Self {
        OpenApiToolError::ConfigError(msg.into())
    }

    /// Shortcut method to create an IO error
    pub fn io<S: Into<String>>(msg: S) -> Self {
        OpenApiToolError::IoError(msg.into())
    }

    /// Shortcut method to create a network error
    pub fn network<S: Into<String>>(msg: S) -> Self {
        OpenApiToolError::NetworkError(msg.into())
    }

    /// Shortcut method to create an API call error
    pub fn api_call<S: Into<String>>(msg: S) -> Self {
        OpenApiToolError::ApiCallError(msg.into())
    }

    /// Shortcut method to create a callback error
    pub fn callback<S: Into<String>>(msg: S) -> Self {
        OpenApiToolError::CallbackError(msg.into())
    }

    /// Shortcut method to create a not found error
    pub fn not_found<S: Into<String>>(msg: S) -> Self {
        OpenApiToolError::NotFound(msg.into())
    }

    /// Shortcut method to create an initialization error
    pub fn initialization<S: Into<String>>(msg: S) -> Self {
        OpenApiToolError::InitializationError(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_creation() {
        let err = OpenApiToolError::parse("Invalid JSON");
        assert!(matches!(err, OpenApiToolError::ParseError(_)));
        assert_eq!(err.to_string(), "Parsing error: Invalid JSON");
    }
    
    #[test]
    fn test_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let tool_err = OpenApiToolError::from(io_err);
        assert!(matches!(tool_err, OpenApiToolError::IoError(_)));
    }
} 