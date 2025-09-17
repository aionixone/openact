use anyhow::Result;
use openact_storage::{
    config::DatabaseConfig,
    encryption::FieldEncryption,
    models::{OpenActConnection, OpenActTask},
    pool::get_pool,
    repos::{OpenActConnectionRepository, OpenActTaskRepository},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub struct Registry {
    pub conn_repo: OpenActConnectionRepository,
    pub task_repo: OpenActTaskRepository,
}

impl Registry {
    pub async fn from_env() -> Result<Self> {
        let cfg = DatabaseConfig::from_env();
        let pool = get_pool(&cfg).await?;
        let enc = FieldEncryption::from_env().ok();
        Ok(Self {
            conn_repo: OpenActConnectionRepository::new(pool.clone(), enc),
            task_repo: OpenActTaskRepository::new(pool.clone()),
        })
    }

    // Narrow CRUD
    pub async fn upsert_connection(&self, conn: &OpenActConnection) -> Result<()> {
        self.conn_repo.upsert(conn).await.map_err(Into::into)
    }
    pub async fn get_connection(&self, trn: &str) -> Result<Option<OpenActConnection>> {
        self.conn_repo.get(trn).await.map_err(Into::into)
    }
    pub async fn delete_connection(&self, trn: &str) -> Result<bool> {
        self.conn_repo.delete(trn).await.map_err(Into::into)
    }

    pub async fn upsert_task(&self, task: &OpenActTask) -> Result<()> {
        self.task_repo.upsert(task).await.map_err(Into::into)
    }
    pub async fn get_task(&self, trn: &str) -> Result<Option<OpenActTask>> {
        self.task_repo.get(trn).await.map_err(Into::into)
    }
    pub async fn delete_task(&self, trn: &str) -> Result<bool> {
        self.task_repo.delete(trn).await.map_err(Into::into)
    }
}

// Wide Task DTOs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WideTask {
    pub trn: String,
    pub tenant: String,
    #[serde(rename = "ConnectionTrn")]
    pub connection_trn: String,
    #[serde(rename = "ApiEndpoint")]
    pub api_endpoint: String,
    #[serde(rename = "Method")]
    pub method: String,
    #[serde(rename = "InvocationHttpParameters", default)]
    pub invocation_http_parameters: Option<WideInvocationHttpParameters>,
    #[serde(rename = "Pagination", default)]
    pub pagination: Option<Value>,
    #[serde(rename = "HttpPolicy", default)]
    pub http_policy: Option<Value>,
    #[serde(rename = "ResponsePolicy", default)]
    pub response_policy: Option<Value>,
}

fn multi_map_to_params(map: &serde_json::Map<String, Value>) -> Vec<WideHeaderParam> {
    let mut out = Vec::new();
    for (k, v) in map {
        match v {
            Value::Array(arr) => {
                for vv in arr {
                    out.push(WideHeaderParam { key: k.clone(), value: vv.as_str().unwrap_or(&vv.to_string()).to_string() });
                }
            }
            other => {
                out.push(WideHeaderParam { key: k.clone(), value: other.as_str().unwrap_or(&other.to_string()).to_string() });
            }
        }
    }
    out
}

fn body_value_to_params(body: &Value) -> Vec<WideHeaderParam> {
    match body {
        Value::Object(obj) => obj
            .iter()
            .flat_map(|(k, v)| match v {
                Value::Array(arr) => arr.iter().map(|vv| WideHeaderParam { key: k.clone(), value: vv.as_str().unwrap_or(&vv.to_string()).to_string() }).collect::<Vec<_>>(),
                other => vec![WideHeaderParam { key: k.clone(), value: other.as_str().unwrap_or(&other.to_string()).to_string() }],
            })
            .collect(),
        other => vec![WideHeaderParam { key: "body".to_string(), value: other.as_str().unwrap_or(&other.to_string()).to_string() }],
    }
}

impl Registry {
    // wide -> narrow
    pub async fn upsert_task_wide(&self, wide: &WideTask) -> Result<()> {
        let (headers_json, query_params_json, request_body_json) = if let Some(inv) = &wide.invocation_http_parameters {
            let headers = Value::Object(make_multi_map(&inv.header_parameters));
            let query = Value::Object(make_multi_map(&inv.query_string_parameters));
            let body = make_body_object(&inv.body_parameters);
            (Some(headers.to_string()), Some(query.to_string()), Some(body.to_string()))
        } else { (None, None, None) };

        let now = chrono::Utc::now();
        let task = OpenActTask {
            trn: wide.trn.clone(),
            tenant: wide.tenant.clone(),
            connection_trn: wide.connection_trn.clone(),
            api_endpoint: wide.api_endpoint.clone(),
            method: wide.method.clone(),
            headers_json,
            query_params_json,
            request_body_json,
            pagination_json: wide.pagination.as_ref().map(|v| v.to_string()),
            http_policy_json: wide.http_policy.as_ref().map(|v| v.to_string()),
            response_policy_json: wide.response_policy.as_ref().map(|v| v.to_string()),
            created_at: now,
            updated_at: now,
            version: 0,
        };
        self.upsert_task(&task).await
    }

    // narrow -> wide
    pub async fn get_task_wide(&self, trn: &str) -> Result<Option<WideTask>> {
        if let Some(task) = self.get_task(trn).await? {
            // headers/query
            let header_params = match task.headers_json.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()) {
                Some(Value::Object(map)) => multi_map_to_params(&map),
                _ => Vec::new(),
            };
            let query_params = match task.query_params_json.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()) {
                Some(Value::Object(map)) => multi_map_to_params(&map),
                _ => Vec::new(),
            };
            let body_params = match task.request_body_json.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()) {
                Some(v) => body_value_to_params(&v),
                None => Vec::new(),
            };

            let invocation = if header_params.is_empty() && query_params.is_empty() && body_params.is_empty() {
                None
            } else {
                Some(WideInvocationHttpParameters {
                    header_parameters: header_params,
                    query_string_parameters: query_params,
                    body_parameters: body_params,
                })
            };

            let wide = WideTask {
                trn: task.trn,
                tenant: task.tenant,
                connection_trn: task.connection_trn,
                api_endpoint: task.api_endpoint,
                method: task.method,
                invocation_http_parameters: invocation,
                pagination: task.pagination_json.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                http_policy: task.http_policy_json.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
                response_policy: task.response_policy_json.and_then(|s| serde_json::from_str::<Value>(&s).ok()),
            };
            Ok(Some(wide))
        } else {
            Ok(None)
        }
    }
}
// Wide format DTOs (minimal; expand later)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WideHeaderParam {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "Value")]
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WideInvocationHttpParameters {
    #[serde(rename = "HeaderParameters", default)]
    pub header_parameters: Vec<WideHeaderParam>,
    #[serde(rename = "QueryStringParameters", default)]
    pub query_string_parameters: Vec<WideHeaderParam>,
    #[serde(rename = "BodyParameters", default)]
    pub body_parameters: Vec<WideHeaderParam>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "AuthorizationType")]
pub enum WideAuth {
    #[serde(rename = "API_KEY")]
    ApiKey { #[serde(rename = "AuthParameters")] auth_parameters: WideAuthParams },
    #[serde(rename = "OAUTH")]
    OAuth { #[serde(rename = "AuthParameters")] auth_parameters: WideAuthParams },
    #[serde(rename = "BEARER_STATIC")]
    BearerStatic { #[serde(rename = "AuthParameters")] auth_parameters: WideAuthParams },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WideAuthParams {
    #[serde(rename = "ApiKeyAuthParameters", default)]
    pub api_key_auth_parameters: Option<ApiKeyParams>,
    #[serde(rename = "OAuthParameters", default)]
    pub oauth_parameters: Option<Value>,
    #[serde(rename = "BearerToken", default)]
    pub bearer_token: Option<String>,
    #[serde(rename = "InvocationHttpParameters", default)]
    pub invocation_http_parameters: Option<WideInvocationHttpParameters>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyParams {
    #[serde(rename = "ApiKeyName")]
    pub api_key_name: String,
    #[serde(rename = "ApiKeyValue")]
    pub api_key_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WideConnection {
    pub trn: String,
    pub tenant: String,
    pub provider: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub auth: WideAuth,
}

pub fn make_multi_map(items: &[WideHeaderParam]) -> serde_json::Map<String, Value> {
    let mut map: serde_json::Map<String, Value> = serde_json::Map::new();
    for it in items {
        let entry = map
            .entry(it.key.clone())
            .or_insert_with(|| Value::Array(vec![]));
        if let Value::Array(arr) = entry {
            arr.push(Value::String(it.value.clone()));
        }
    }
    map
}

pub fn make_body_object(items: &[WideHeaderParam]) -> Value {
    let mut obj = serde_json::Map::new();
    for it in items {
        match obj.get_mut(&it.key) {
            Some(Value::Array(arr)) => arr.push(Value::String(it.value.clone())),
            Some(v) => {
                *v = Value::Array(vec![v.clone(), Value::String(it.value.clone())]);
            }
            None => {
                obj.insert(it.key.clone(), Value::String(it.value.clone()));
            }
        }
    }
    Value::Object(obj)
}

impl Registry {
    pub async fn upsert_connection_wide(&self, wide: &WideConnection) -> Result<()> {
        // Map wide → narrow
        let (auth_kind, auth_ref, secrets_json) = match &wide.auth {
            WideAuth::ApiKey { auth_parameters } => {
                let ap = auth_parameters;
                let secrets = if let Some(kv) = &ap.api_key_auth_parameters {
                    serde_json::json!({"api_key":{"name":kv.api_key_name,"value":kv.api_key_value}})
                } else {
                    Value::Null
                };
                (
                    "API_KEY".to_string(),
                    None,
                    if secrets.is_null() { None } else { Some(secrets.to_string()) },
                )
            }
            WideAuth::OAuth { .. } => ("OAUTH".to_string(), None, None),
            WideAuth::BearerStatic { auth_parameters } => {
                let token = auth_parameters.bearer_token.clone();
                let secrets = token.map(|t| serde_json::json!({"bearer": t}).to_string());
                ("BEARER_STATIC".to_string(), None, secrets)
            }
        };

        let (default_headers_json, default_query_params_json, default_body_json) = match &wide.auth {
            WideAuth::ApiKey { auth_parameters }
            | WideAuth::OAuth { auth_parameters }
            | WideAuth::BearerStatic { auth_parameters } => {
                if let Some(inv) = &auth_parameters.invocation_http_parameters {
                    let headers = Value::Object(make_multi_map(&inv.header_parameters));
                    let query = Value::Object(make_multi_map(&inv.query_string_parameters));
                    let body = make_body_object(&inv.body_parameters);
                    (Some(headers.to_string()), Some(query.to_string()), Some(body.to_string()))
                } else {
                    (None, None, None)
                }
            }
        };

        let now = chrono::Utc::now();
        let conn = OpenActConnection {
            trn: wide.trn.clone(),
            tenant: wide.tenant.clone(),
            provider: wide.provider.clone(),
            name: wide.name.clone(),
            auth_kind,
            auth_ref,
            network_config_json: None,
            tls_config_json: None,
            http_policy_json: None,
            default_headers_json,
            default_query_params_json,
            default_body_json,
            secrets_encrypted: secrets_json,
            secrets_nonce: None,
            key_version: 0,
            created_at: now,
            updated_at: now,
            version: 0,
        };
        self.upsert_connection(&conn).await
    }
}
