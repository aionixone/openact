use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuthConnection {
    pub tenant: String,
    pub provider: String,
    pub user_id: String,
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default = "default_token_type")]
    pub token_type: String,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub extra: Value,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

fn default_token_type() -> String { "Bearer".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenActConnection {
    pub trn: String,
    pub tenant: String,
    pub provider: String,
    #[serde(default)]
    pub name: Option<String>,
    pub auth_kind: String,
    #[serde(default)]
    pub auth_ref: Option<String>,
    #[serde(default)]
    pub network_config_json: Option<String>,
    #[serde(default)]
    pub tls_config_json: Option<String>,
    #[serde(default)]
    pub http_policy_json: Option<String>,
    #[serde(default)]
    pub default_headers_json: Option<String>,
    #[serde(default)]
    pub default_query_params_json: Option<String>,
    #[serde(default)]
    pub default_body_json: Option<String>,
    #[serde(default)]
    pub secrets_encrypted: Option<String>,
    #[serde(default)]
    pub secrets_nonce: Option<String>,
    #[serde(default)]
    pub key_version: i32,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenActTask {
    pub trn: String,
    pub tenant: String,
    pub connection_trn: String,
    pub api_endpoint: String,
    pub method: String,
    #[serde(default)]
    pub headers_json: Option<String>,
    #[serde(default)]
    pub query_params_json: Option<String>,
    #[serde(default)]
    pub request_body_json: Option<String>,
    #[serde(default)]
    pub pagination_json: Option<String>,
    #[serde(default)]
    pub http_policy_json: Option<String>,
    #[serde(default)]
    pub response_policy_json: Option<String>,
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub version: i32,
}
