//! Unified Executor Module
//!
//! Provides a unified API call executor supporting all authentication types:
//! - API Key, Basic Auth, OAuth2 Client Credentials, OAuth2 Authorization Code
//! - Automatically handles token refresh, parameter merging, and authentication injection

pub mod auth_injector;
pub mod client_pool;
pub mod http_executor;
pub mod parameter_merger;

#[cfg(test)]
pub mod integration_tests;

pub use auth_injector::{AuthInjectionError, AuthInjector};
pub use http_executor::HttpExecutor;
pub use parameter_merger::{MergedParameters, ParameterMerger};

use crate::models::{ConnectionConfig, TaskConfig};
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::Value;

/// Execution Result
#[derive(Debug)]
pub struct ExecutionResult {
    pub status: u16,
    pub headers: std::collections::HashMap<String, String>,
    pub body: Value,
}

/// Main Executor: Handles API calls for all authentication types
pub struct Executor {
    http_executor: HttpExecutor,
}

impl Executor {
    /// Create a new Executor instance
    pub fn new() -> Self {
        Self {
            http_executor: HttpExecutor::new(),
        }
    }

    /// Execute an API call (supports all authentication types, including automatic token refresh)
    pub async fn execute(
        &self,
        connection: &ConnectionConfig,
        task: &TaskConfig,
    ) -> Result<ExecutionResult> {
        let response = self.http_executor.execute(connection, task).await?;

        // Extract response information
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        // Read and apply ResponsePolicy
        let effective = task.response_policy.clone().unwrap_or_default();

        // Get content-type
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let bytes = response.bytes().await?;
        if bytes.len() > effective.max_body_bytes {
            anyhow::bail!(
                "response body exceeds max_body_bytes: {} > {}",
                bytes.len(),
                effective.max_body_bytes
            );
        }

        let body: Value = if content_type.contains("json") {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).to_string()))
        } else if content_type.starts_with("text/") {
            Value::String(String::from_utf8_lossy(&bytes).to_string())
        } else {
            if effective.allow_binary {
                Value::Object(serde_json::Map::from_iter(vec![(
                    "binary".to_string(),
                    Value::String(STANDARD.encode(&bytes)),
                )]))
            } else {
                anyhow::bail!(
                    "binary response not allowed by ResponsePolicy (content-type: {})",
                    content_type
                );
            }
        };

        Ok(ExecutionResult {
            status,
            headers,
            body,
        })
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ApiKeyAuthParameters, AuthorizationType, TaskConfig};
    use httpmock::prelude::*;

    fn make_api_key_connection() -> crate::models::ConnectionConfig {
        let mut c = crate::models::ConnectionConfig::new(
            "trn:openact:default:connection/perm".to_string(),
            "conn".to_string(),
            AuthorizationType::ApiKey,
        );
        c.auth_parameters.api_key_auth_parameters = Some(ApiKeyAuthParameters {
            api_key_name: "X-API-Key".to_string(),
            api_key_value: "k".to_string(),
        });
        c
    }

    #[tokio::test]
    async fn response_policy_max_body_exceeded() {
        let server = MockServer::start();
        let _m = server.mock(|when, then| {
            when.method(GET).path("/big");
            then.status(200)
                .header("Content-Type", "application/json")
                .body("{\"a\":\"".to_string() + &"x".repeat(1024 * 1024) + "\"}");
        });

        let conn = make_api_key_connection();
        let mut task = TaskConfig::new(
            "trn:openact:default:task/max".to_string(),
            "t".to_string(),
            conn.trn.clone(),
            format!("{}{}", server.base_url(), "/big"),
            "GET".to_string(),
        );
        task.response_policy = Some(crate::models::ResponsePolicy {
            allow_binary: false,
            max_body_bytes: 1024,
            ..Default::default()
        });

        let ex = Executor::new();
        let res = ex.execute(&conn, &task).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn response_policy_binary_disallowed() {
        let server = MockServer::start();
        let _m = server.mock(|when, then| {
            when.method(GET).path("/bin");
            then.status(200)
                .header("Content-Type", "application/octet-stream")
                .body(vec![1u8, 2, 3, 4]);
        });

        let conn = make_api_key_connection();
        let mut task = TaskConfig::new(
            "trn:openact:default:task/bin".to_string(),
            "t".to_string(),
            conn.trn.clone(),
            format!("{}{}", server.base_url(), "/bin"),
            "GET".to_string(),
        );
        task.response_policy = Some(crate::models::ResponsePolicy {
            allow_binary: false,
            max_body_bytes: 8 * 1024 * 1024,
            ..Default::default()
        });

        let ex = Executor::new();
        let res = ex.execute(&conn, &task).await;
        assert!(res.is_err());
    }
}
