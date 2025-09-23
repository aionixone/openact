use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::models::common::{RetryPolicy, MultiValue, NetworkConfig, TimeoutConfig, HttpPolicy, ResponsePolicy};
use crate::models::connection::{AuthorizationType, AuthParameters, InvocationHttpParameters};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecuteOverridesDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<HashMap<String, Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecuteRequestDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<ExecuteOverridesDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>, // status-only | headers-only | body-only | full
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteResponseDto {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: serde_json::Value,
}

/// Ad-hoc execution request - execute action without persistent task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdhocExecuteRequestDto {
    /// Connection TRN to use for authentication
    pub connection_trn: String,
    /// HTTP method (GET, POST, PUT, DELETE, etc.)
    pub method: String,
    /// API endpoint URL
    pub endpoint: String,
    /// Optional headers (replaces MultiValue with Vec<String> for simplicity)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, Vec<String>>>,
    /// Optional query parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<HashMap<String, Vec<String>>>,
    /// Optional request body
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
    /// Optional timeout configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_config: Option<TimeoutConfig>,
    /// Optional network configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_config: Option<NetworkConfig>,
    /// Optional HTTP policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_policy: Option<HttpPolicy>,
    /// Optional response policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_policy: Option<ResponsePolicy>,
    /// Optional retry policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatusDto {
    pub trn: String,
    pub authorization_type: AuthorizationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_auth_ref: Option<bool>,
    /// one of: ready | expiring_soon | expired | unbound | not_issued | not_authorized | misconfigured
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seconds_to_expiry: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListQueryDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
}

/// Connection upsert request DTO (without metadata fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionUpsertRequest {
    pub trn: String,
    pub name: String,
    pub authorization_type: AuthorizationType,
    pub auth_parameters: AuthParameters,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invocation_http_parameters: Option<InvocationHttpParameters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_config: Option<NetworkConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_config: Option<TimeoutConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_policy: Option<HttpPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<RetryPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_ref: Option<String>,
}

/// Task upsert request DTO (without metadata fields)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskUpsertRequest {
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
}

impl ConnectionUpsertRequest {
    /// Convert to ConnectionConfig with metadata
    /// For create operations, pass None for existing_created_at
    /// For update operations, pass Some(existing.created_at)
    pub fn to_config(self, existing_version: Option<i64>, existing_created_at: Option<chrono::DateTime<chrono::Utc>>) -> crate::models::ConnectionConfig {
        use chrono::Utc;
        use crate::models::ConnectionConfig;
        
        let now = Utc::now();
        let version = existing_version.map(|v| v + 1).unwrap_or(1);
        let created_at = existing_created_at.unwrap_or(now);
        
        ConnectionConfig {
            trn: self.trn,
            name: self.name,
            authorization_type: self.authorization_type,
            auth_parameters: self.auth_parameters,
            invocation_http_parameters: self.invocation_http_parameters,
            network_config: self.network_config,
            timeout_config: self.timeout_config,
            http_policy: self.http_policy,
            retry_policy: self.retry_policy,
            auth_ref: self.auth_ref,
            created_at,
            updated_at: now,
            version,
        }
    }
}

impl TaskUpsertRequest {
    /// Convert to TaskConfig with metadata
    /// For create operations, pass None for existing_created_at
    /// For update operations, pass Some(existing.created_at)
    pub fn to_config(self, existing_version: Option<i64>, existing_created_at: Option<chrono::DateTime<chrono::Utc>>) -> crate::models::TaskConfig {
        use chrono::Utc;
        use crate::models::TaskConfig;
        
        let now = Utc::now();
        let version = existing_version.map(|v| v + 1).unwrap_or(1);
        let created_at = existing_created_at.unwrap_or(now);
        
        TaskConfig {
            trn: self.trn,
            name: self.name,
            connection_trn: self.connection_trn,
            api_endpoint: self.api_endpoint,
            method: self.method,
            headers: self.headers,
            query_params: self.query_params,
            request_body: self.request_body,
            timeout_config: self.timeout_config,
            network_config: self.network_config,
            http_policy: self.http_policy,
            response_policy: self.response_policy,
            retry_policy: self.retry_policy,
            created_at,
            updated_at: now,
            version,
        }
    }
}




#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use crate::models::connection::{AuthorizationType, AuthParameters};

    #[test]
    fn test_connection_upsert_request_to_config() {
        let req = ConnectionUpsertRequest {
            trn: "trn:openact:test:connection/test@v1".to_string(),
            name: "Test Connection".to_string(),
            authorization_type: AuthorizationType::ApiKey,
            auth_parameters: AuthParameters {
                api_key_auth_parameters: None,
                basic_auth_parameters: None,
                oauth_parameters: None,
            },
            invocation_http_parameters: None,
            network_config: None,
            timeout_config: None,
            http_policy: None,
            retry_policy: None,
            auth_ref: None,
        };

        // Test creation (no existing data)
        let config_create = req.clone().to_config(None, None);
        assert_eq!(config_create.trn, req.trn);
        assert_eq!(config_create.name, req.name);
        assert_eq!(config_create.version, 1);
        assert_eq!(config_create.created_at, config_create.updated_at);

        // Test update (with existing data)
        let existing_created_at = DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let config_update = req.to_config(Some(5), Some(existing_created_at));
        assert_eq!(config_update.version, 6);
        assert_eq!(config_update.created_at, existing_created_at);
        assert!(config_update.updated_at > existing_created_at);
    }
}
