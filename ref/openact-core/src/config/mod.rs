//! Configuration management for OpenAct v2
//! 
//! Manages AWS EventBridge-compatible Connection and Step Functions-compatible Task configurations

pub mod connection;
pub mod task;
pub mod types;
pub mod registry;

pub use connection::{ConnectionConfig, ConnectionConfigStore};
pub use task::{TaskConfig, TaskConfigStore};
pub use types::*;
pub use registry::load_connections_from_dir;
