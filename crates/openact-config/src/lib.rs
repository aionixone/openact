pub mod env_resolver;
pub mod error;
pub mod loader;
pub mod manager;
pub mod schema;
pub mod schema_validator;

// Re-export commonly used types
pub use env_resolver::{EnvResolver, EnvResolverError};
pub use error::{ConfigError, ConfigResult};
pub use loader::{ConfigLoader, FileFormat};
pub use manager::{
    ConfigManager, ConfigManagerError, ConflictResolution, ExportOptions, ImportConflict,
    ImportOptions, ImportResult, SyncStrategy, VersioningStrategy,
};
pub use schema::{ActionConfig, ConfigManifest, ConnectionConfig, ConnectorConfig};
pub use schema_validator::{ConnectorValidator, SchemaValidationError, SchemaValidator};
