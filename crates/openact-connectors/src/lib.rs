pub mod error;
pub mod auth;

// Conditional compilation for each connector
#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "postgresql")]
pub mod postgresql;

#[cfg(feature = "mysql")]
pub mod mysql;

#[cfg(feature = "redis")]
pub mod redis;

// Re-export commonly used types
pub use error::{ConnectorError, ConnectorResult};
pub use auth::{AuthConnection, AuthConnectionStore, TokenInfo, RefreshOutcome};

// Re-export connector modules when enabled
#[cfg(feature = "http")]
pub use http::{HttpConnection, HttpAction, HttpExecutor, HttpExecutionResult};

#[cfg(feature = "postgresql")]
pub use postgresql::{PostgresConnection, PostgresExecutor};

#[cfg(feature = "mysql")]
pub use mysql::{MysqlConnection, MysqlExecutor};

#[cfg(feature = "redis")]
pub use redis::{RedisConnection, RedisExecutor};
