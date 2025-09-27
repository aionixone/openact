//! Task configuration models
//!
//! This module contains data structures for managing HTTP task configurations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

use super::common::{
    HttpPolicy, MultiValue, NetworkConfig, ResponsePolicy, RetryPolicy, TimeoutConfig,
};

/// HTTP Task configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TaskConfig {
    pub trn: String,
    pub name: String,
    pub connection_trn: String,
    pub api_endpoint: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, MultiValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_params: Option<HashMap<String, MultiValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_config: Option<TimeoutConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_config: Option<NetworkConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_policy: Option<HttpPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_policy: Option<ResponsePolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: i64,
}

impl TaskConfig {
    /// Create a new HTTP task with default values
    pub fn new(
        trn: String,
        name: String,
        connection_trn: String,
        api_endpoint: String,
        method: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            trn,
            name,
            connection_trn,
            api_endpoint,
            method,
            headers: None,
            query_params: None,
            request_body: None,
            timeout_config: None,
            network_config: None,
            http_policy: None,
            response_policy: None,
            retry_policy: None,
            created_at: now,
            updated_at: now,
            version: 1,
        }
    }
}
