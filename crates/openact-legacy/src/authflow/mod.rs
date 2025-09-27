//! AuthFlow subsystem: workflow/DSL/callback/server for complex OAuth flows

// Real submodules under authflow
pub mod actions;
pub mod dsl;
pub mod engine;
pub mod workflow;
// moved to crate::server::authflow::* (behind features)
#[cfg(feature = "server")]
pub use crate::server::authflow as server;
#[cfg(feature = "callback")]
pub use crate::server::authflow::callback;
