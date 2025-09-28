pub mod error;
pub mod factory;
pub mod registry;

#[cfg(feature = "http")]
pub mod http_factory;

#[cfg(feature = "postgresql")]
pub mod postgres_factory;

// Re-export commonly used types
pub use error::{RegistryError, RegistryResult};
pub use factory::{ActionFactory, ConnectionFactory};
pub use registry::{ConnectorRegistry, ExecutionContext, ExecutionResult};

#[cfg(feature = "http")]
pub use http_factory::HttpFactory;

#[cfg(feature = "postgresql")]
pub use postgres_factory::PostgresFactory;
