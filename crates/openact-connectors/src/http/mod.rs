pub mod connection;
pub mod actions;
pub mod executor;
pub mod oauth;
pub mod client_cache;
pub mod url_builder;
pub mod timeout_manager;
pub mod retry_manager;
pub mod policy_manager;
pub mod mcp_converter;
pub mod body_builder;
pub mod validator;
pub mod factory;

#[cfg(test)]
pub mod mcp_integration_demo;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod timeout_demo;

#[cfg(test)]
mod retry_demo;

#[cfg(test)]
mod action_examples;

#[cfg(test)]
mod body_examples;

pub use connection::HttpConnection;
pub use actions::HttpAction;
pub use executor::{HttpExecutor, HttpExecutionResult};
pub use factory::HttpFactory;
pub use validator::{
    HTTP_CONNECTION_FIELDS, HTTP_ACTION_FIELDS,
    filter_http_connection_fields, filter_http_action_fields,
    validate_http_connection, validate_http_action,
};
