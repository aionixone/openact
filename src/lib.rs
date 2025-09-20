pub mod store;
pub mod models;
pub mod executor;

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

