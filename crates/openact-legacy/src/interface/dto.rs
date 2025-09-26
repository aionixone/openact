use crate::models::common::{
    HttpPolicy, MultiValue, NetworkConfig, ResponsePolicy, RetryPolicy, TimeoutConfig,
};
use crate::models::connection::{AuthParameters, AuthorizationType, InvocationHttpParameters};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "openapi")]
#[allow(unused_imports)] // Used in schema examples via json! macro
use serde_json::json;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ExecuteRequestDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<ExecuteOverridesDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>, // status-only | headers-only | body-only | full
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(example = json!({
    "status": 200,
    "headers": {
        "content-type": "application/json; charset=utf-8",
        "x-ratelimit-remaining": "4999"
    },
    "body": {
        "id": 123456789,
        "login": "octocat",
        "name": "The Octocat",
        "public_repos": 8,
        "followers": 4000
    }
})))]
pub struct ExecuteResponseDto {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: serde_json::Value,
}

/// Ad-hoc execution request - execute action without persistent task
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(example = json!({
    "connection_trn": "trn:openact:default:connection/github-api@v1",
    "method": "GET",
    "endpoint": "/user/repos",
    "query_params": {
        "type": ["owner"],
        "sort": ["updated"],
        "per_page": ["10"]
    },
    "headers": {
        "Accept": ["application/vnd.github+json"]
    },
    "timeout_ms": 10000
})))]
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
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(example = json!({
    "trn": "trn:openact:default:connection/github-api@v1",
    "authorization_type": "oauth2",
    "has_auth_ref": true,
    "status": "ready",
    "expires_at": "2023-12-31T23:59:59Z",
    "seconds_to_expiry": 2592000,
    "message": null
})))]
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
#[cfg_attr(feature = "openapi", derive(ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(example = json!({
    "trn": "trn:openact:default:connection/github-api@v1",
    "name": "GitHub API Connection",
    "authorization_type": "oauth2",
    "auth_parameters": {
        "oauth_parameters": {
            "authorization_url": "https://github.com/login/oauth/authorize",
            "token_url": "https://github.com/login/oauth/access_token",
            "client_id": "github_client_123",
            "client_secret": "***redacted***",
            "scope": "user:email repo"
        }
    },
    "invocation_http_parameters": {
        "base_url": "https://api.github.com",
        "default_headers": {
            "Accept": "application/vnd.github+json",
            "User-Agent": "OpenAct/1.0"
        }
    },
    "timeout_config": {
        "connect_timeout_ms": 5000,
        "request_timeout_ms": 30000
    }
})))]
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
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[cfg_attr(feature = "openapi", schema(example = json!({
    "trn": "trn:openact:default:task/get-user-repos@v1",
    "name": "Get User Repositories",
    "connection_trn": "trn:openact:default:connection/github-api@v1",
    "api_endpoint": "/user/repos",
    "method": "GET",
    "query_params": {
        "type": {"value": "owner"},
        "sort": {"value": "updated"},
        "per_page": {"value": "50"}
    },
    "timeout_config": {
        "request_timeout_ms": 10000
    },
    "retry_policy": {
        "max_attempts": 3,
        "initial_delay_ms": 1000,
        "max_delay_ms": 5000,
        "backoff_multiplier": 2.0
    }
})))]
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
    pub fn to_config(
        self,
        existing_version: Option<i64>,
        existing_created_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> crate::models::ConnectionConfig {
        use crate::models::ConnectionConfig;
        use chrono::Utc;

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
    pub fn to_config(
        self,
        existing_version: Option<i64>,
        existing_created_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> crate::models::TaskConfig {
        use crate::models::TaskConfig;
        use chrono::Utc;

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
    use crate::models::connection::{AuthParameters, AuthorizationType};
    use chrono::{DateTime, Utc};

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
