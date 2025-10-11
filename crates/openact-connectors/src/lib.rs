pub mod error;
pub mod auth;

// Conditional compilation for each connector
#[cfg(feature = "http")]
pub mod http;

pub mod generic_async;

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
pub use http::{HttpConnection, HttpAction, HttpExecutor, HttpExecutionResult, HttpFactory};

pub use generic_async::{GenericAsyncAction, GenericAsyncConnection, GenericAsyncFactory};

#[cfg(feature = "postgresql")]
pub use postgresql::{PostgresConnection, PostgresExecutor, PostgresFactory};

#[cfg(feature = "mysql")]
pub use mysql::{MysqlConnection, MysqlExecutor};

#[cfg(feature = "redis")]
pub use redis::{RedisConnection, RedisExecutor};

// Convenience registrar functions
#[cfg(feature = "http")]
pub fn http_registrar() -> openact_registry::ConnectorRegistrar {
    HttpFactory::registrar()
}

pub fn generic_async_registrar() -> openact_registry::ConnectorRegistrar {
    GenericAsyncFactory::registrar()
}

#[cfg(feature = "postgresql")]
pub fn postgresql_registrar() -> openact_registry::ConnectorRegistrar {
    PostgresFactory::registrar()
}
