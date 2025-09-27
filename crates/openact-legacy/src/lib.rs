pub mod cli;
pub mod executor;
pub mod models;
pub mod oauth;
pub mod store;

// Workflow-related modules remain public (original paths)
// Deprecated: moved under authflow
// pub mod dsl;
// pub mod actions;
// pub mod engine;
// pub mod callback;
// #[cfg(feature = "server")]
// pub mod server;

// Workflow path is always available
pub mod authflow;

// Also expose API module used by callback/server
pub mod api;

// Keep run_flow via authflow
pub use authflow::engine::run_flow;

pub mod config;
pub mod utils;

pub mod app;
pub mod interface;
pub mod observability;
#[cfg(feature = "server")]
pub mod server;
pub mod templates;
