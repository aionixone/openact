//! OpenAct v2 错误类型定义

use thiserror::Error;

pub type Result<T> = std::result::Result<T, OpenActError>;

#[derive(Debug, Error)]
pub enum OpenActError {
    #[error("TRN error: {0}")]
    Trn(String),
    
    #[error("Connection config error: {0}")]
    ConnectionConfig(String),
    
    #[error("Task config error: {0}")]
    TaskConfig(String),
    
    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("Authentication error: {0}")]
    Auth(String),
    
    #[error("Parameter merge error: {0}")]
    ParameterMerge(String),
    
    #[error("JSONata expression error: {0}")]
    JSONataExpr(String),
    
    #[error("AuthFlow integration error: {0}")]
    AuthFlow(String),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Timeout error: operation timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
    
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),
    
    #[error("Circuit breaker open: {0}")]
    CircuitBreaker(String),
}

impl OpenActError {
    pub fn trn(msg: impl Into<String>) -> Self {
        Self::Trn(msg.into())
    }
    
    pub fn connection_config(msg: impl Into<String>) -> Self {
        Self::ConnectionConfig(msg.into())
    }
    
    pub fn task_config(msg: impl Into<String>) -> Self {
        Self::TaskConfig(msg.into())
    }
    
    pub fn auth(msg: impl Into<String>) -> Self {
        Self::Auth(msg.into())
    }
    
    pub fn parameter_merge(msg: impl Into<String>) -> Self {
        Self::ParameterMerge(msg.into())
    }
    
    pub fn jsonata_expr(msg: impl Into<String>) -> Self {
        Self::JSONataExpr(msg.into())
    }
    
    pub fn invalid_config(msg: impl Into<String>) -> Self {
        Self::InvalidConfig(msg.into())
    }
    
    pub fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }
    
    pub fn timeout(timeout_ms: u64) -> Self {
        Self::Timeout { timeout_ms }
    }
    
    pub fn rate_limit(msg: impl Into<String>) -> Self {
        Self::RateLimit(msg.into())
    }
    
    pub fn circuit_breaker(msg: impl Into<String>) -> Self {
        Self::CircuitBreaker(msg.into())
    }
}
