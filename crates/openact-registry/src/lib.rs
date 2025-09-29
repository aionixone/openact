pub mod error;
pub mod factory;
pub mod registry;

// Re-export commonly used types
pub use error::{RegistryError, RegistryResult};
pub use factory::{ActionFactory, ConnectionFactory};
pub use registry::{ConnectorRegistry, ExecutionContext, ExecutionResult};

/// A simple function type alias to allow connectors to expose a registrar function
pub type ConnectorRegistrar = fn(&mut ConnectorRegistry);
