//! Data models for OpenAct
//!
//! This module contains all data structures used throughout the OpenAct system,
//! organized by domain and responsibility.

// Connection configuration models
pub mod connection;
pub use connection::*;

// Authentication models (OAuth tokens, etc.)
pub mod auth;
pub use auth::*;

// Task configuration models
pub mod task;
pub use task::*;

// Common types and utilities
pub mod common;
pub use common::*;
