//! Generic asynchronous connector – provides a skeleton implementation that
//! returns a running handle immediately. This is the foundation for future
//! long-running task integrations.

pub mod config;
mod factory;

pub use factory::{GenericAsyncAction, GenericAsyncConnection, GenericAsyncFactory};
