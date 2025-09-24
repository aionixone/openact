//! API documentation and schema definitions for OpenAPI integration
//!
//! This module contains OpenAPI-specific configurations, schema definitions,
//! and documentation utilities that are conditionally compiled when the
//! `openapi` feature is enabled.

#[cfg(feature = "openapi")]
pub mod openapi;

#[cfg(feature = "openapi")]
pub use openapi::*;
