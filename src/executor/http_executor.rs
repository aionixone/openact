//! HTTP执行器
//!
//! 处理直接HTTP调用：API Key、Basic Auth、OAuth2 Client Credentials

use super::auth_injector::create_auth_injector;
use super::parameter_merger::ParameterMerger;
use crate::authflow::actions::{EnsureFreshTokenHandler, OAuth2ClientCredentialsHandler};
use crate::authflow::engine::TaskHandler;
use crate::models::{AuthorizationType, ConnectionConfig, TaskConfig};
use crate::store::{StoreBackend, StoreConfig, create_connection_store};

use crate::models::AuthConnection;
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use reqwest::Proxy;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use reqwest::{Client, Response};
use serde_json::json;
use std::collections::HashMap;
use std::sync::OnceLock;
use tokio::sync::Mutex;
use tokio::sync::oneshot;

// 可复用客户端池（按配置）
// 已由 CLIENT_POOL 取代（删除保留以消除未引用字段警告）
static CLIENT_POOL: OnceLock<Mutex<HashMap<String, Client>>> = OnceLock::new();
static CC_TOKEN_CACHE: OnceLock<Mutex<HashMap<String, (String, DateTime<Utc>)>>> = OnceLock::new();
static CC_INFLIGHT: OnceLock<
    Mutex<HashMap<String, Vec<oneshot::Sender<anyhow::Result<(String, DateTime<Utc>)>>>>>,
> = OnceLock::new();

/// HTTP执行器：处理直接HTTP调用
pub struct HttpExecutor {}

impl HttpExecutor {
    /// 创建新的HTTP执行器
    pub fn new() -> Self {
        Self {}
    }

    /// 执行HTTP请求
    pub async fn execute(
        &self,
        connection: &ConnectionConfig,
        task: &TaskConfig,
    ) -> Result<Response> {
        // 1. 合并参数（ConnectionWins策略）
        let mut merged =
            ParameterMerger::merge(connection, task).context("Failed to merge parameters")?;

        // 2. 注入认证信息
        self.inject_authentication(&mut merged.headers, &mut merged.query_params, connection)
            .await
            .context("Failed to inject authentication")?;

        // 3. 构建完整URL
        let url = self.build_url(&merged.endpoint, &merged.query_params)?;

        // 4. 获取对应配置的HTTP客户端
        let client = self.get_client_for(connection, task)?;
        // 5. 构建HTTP请求
        let mut request_builder = client
            .request(
                merged
                    .method
                    .parse()
                    .map_err(|e| anyhow!("Invalid HTTP method '{}': {}", merged.method, e))?,
                url,
            )
            .headers(merged.headers);

        // 6. 添加请求体（如果有）
        if let Some(body) = merged.body {
            request_builder = request_builder.json(&body);
        }

        // 7. 执行请求
        let response = request_builder
            .send()
            .await
            .context("Failed to send HTTP request")?;

        Ok(response)
    }

    /// 注入认证信息（包括OAuth2 token自动刷新）
    async fn inject_authentication(
        &self,
        headers: &mut reqwest::header::HeaderMap,
        query_params: &mut HashMap<String, String>,
        connection: &ConnectionConfig,
    ) -> Result<()> {
        match connection.authorization_type {
            AuthorizationType::OAuth2ClientCredentials => {
                // OAuth2 Client Credentials: 获取或刷新token，然后注入Bearer header
                self.handle_oauth2_client_credentials(headers, connection)
                    .await?;
            }
            AuthorizationType::OAuth2AuthorizationCode => {
                // OAuth2 Authorization Code: 从存储中获取token，必要时刷新，然后注入Bearer header
                self.handle_oauth2_authorization_code(headers, connection)
                    .await?;
            }
            _ => {
                // API Key和Basic Auth: 直接注入，无需token刷新
                let injector = create_auth_injector(&connection.authorization_type);
                injector
                    .inject_auth(headers, query_params, connection)
                    .map_err(|e| anyhow!("Authentication injection failed: {}", e))?;
            }
        }

        Ok(())
    }

    /// 处理OAuth2 Client Credentials认证
    async fn handle_oauth2_client_credentials(
        &self,
        headers: &mut reqwest::header::HeaderMap,
        connection: &ConnectionConfig,
    ) -> Result<()> {
        // 优先从持久层读取（如果开启sqlite后端），命中则回填内存缓存
        if let Some((token, exp)) = Self::maybe_get_db_cc_token(&connection.trn).await? {
            Self::cache_cc_token(&connection.trn, token.clone(), exp).await;
            let auth_value = format!("Bearer {}", token);
            let header_value = HeaderValue::from_str(&auth_value)
                .map_err(|_| anyhow!("Invalid access token format"))?;
            headers.insert(AUTHORIZATION, header_value);
            return Ok(());
        }

        // 其次从内存缓存读取（带 60s skew）
        if let Some(token) = Self::get_cached_cc_token(&connection.trn, 60).await {
            let auth_value = format!("Bearer {}", token);
            let header_value = HeaderValue::from_str(&auth_value)
                .map_err(|_| anyhow!("Invalid access token format"))?;
            headers.insert(AUTHORIZATION, header_value);
            return Ok(());
        }

        // Singleflight: 如果已有并发获取，等待其结果
        if let Some(rx) = Self::register_cc_inflight(&connection.trn).await? {
            let (token, _exp) = rx.await.map_err(|_| anyhow!("inflight channel closed"))??;
            let auth_value = format!("Bearer {}", token);
            let header_value = HeaderValue::from_str(&auth_value)
                .map_err(|_| anyhow!("Invalid access token format"))?;
            headers.insert(AUTHORIZATION, header_value);
            return Ok(());
        }

        let oauth_params = connection
            .auth_parameters
            .oauth_parameters
            .as_ref()
            .ok_or_else(|| {
                anyhow!(
                    "OAuth2 parameters missing for connection: {}",
                    connection.trn
                )
            })?;

        // 构建Client Credentials请求参数
        let mut request_params = json!({
            "tokenUrl": oauth_params.token_url,
            "clientId": oauth_params.client_id,
            "clientSecret": oauth_params.client_secret,
        });

        // 添加scope（如果有）
        if let Some(scope) = &oauth_params.scope {
            request_params["scopes"] = json!(scope.split_whitespace().collect::<Vec<_>>());
        }

        // 调用现有的OAuth2ClientCredentialsHandler
        let handler = OAuth2ClientCredentialsHandler::default();
        let result = handler
            .execute("oauth2.client_credentials", "", &request_params)
            .context("Failed to obtain OAuth2 Client Credentials token")?;

        // 提取access_token
        let access_token = result
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("No access_token in OAuth2 response"))?;

        // 计算过期时间（默认3600s）并写入缓存
        let now = Utc::now();
        let expires_in = result
            .get("expires_in")
            .and_then(|v| v.as_i64())
            .unwrap_or(3600);
        let expires_at = now + Duration::seconds(expires_in);
        // 先写入持久层（若启用sqlite），再写回内存缓存
        if let Ok(backend_env) = std::env::var("OPENACT_AUTH_BACKEND") {
            if backend_env.eq_ignore_ascii_case("sqlite") {
                let cfg = StoreConfig {
                    backend: StoreBackend::Sqlite,
                    ..Default::default()
                };
                if let Ok(store) = create_connection_store(cfg).await {
                    // 使用固定tenant/provider，将 connection.trn 作为 user_id 进行映射
                    let ac = AuthConnection::new_with_params(
                        "openact",
                        "oauth2_cc",
                        &connection.trn,
                        access_token.to_string(),
                        None,
                        Some(expires_at),
                        Some("Bearer".to_string()),
                        oauth_params.scope.clone(),
                        None,
                    )
                    .map_err(|e| anyhow!("failed to create AuthConnection: {}", e))?;
                    let trn_str = ac.trn.to_string();
                    if store.put(&trn_str, &ac).await.is_ok() {
                        let _ = store.cleanup_expired().await;
                    }
                }
            }
        }
        // 内存缓存写入（无论持久化是否成功）
        Self::cache_cc_token(&connection.trn, access_token.to_string(), expires_at).await;
        // 通知等待者
        let _ =
            Self::finish_cc_inflight(&connection.trn, Ok((access_token.to_string(), expires_at)))
                .await;

        // 注入Bearer token到Authorization header
        let auth_value = format!("Bearer {}", access_token);
        let header_value = HeaderValue::from_str(&auth_value)
            .map_err(|_| anyhow!("Invalid access token format"))?;
        headers.insert(AUTHORIZATION, header_value);

        Ok(())
    }

    async fn get_cached_cc_token(trn: &str, skew_seconds: i64) -> Option<String> {
        let cache = CC_TOKEN_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let guard = cache.lock().await;
        if let Some((token, exp)) = guard.get(trn) {
            if *exp > Utc::now() + Duration::seconds(skew_seconds) {
                return Some(token.clone());
            }
        }
        None
    }

    async fn cache_cc_token(trn: &str, token: String, expires_at: DateTime<Utc>) {
        let cache = CC_TOKEN_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = cache.lock().await;
        guard.insert(trn.to_string(), (token, expires_at));
    }

    async fn maybe_get_db_cc_token(trn: &str) -> Result<Option<(String, DateTime<Utc>)>> {
        match std::env::var("OPENACT_AUTH_BACKEND") {
            Ok(v) if v.eq_ignore_ascii_case("sqlite") => {
                let cfg = StoreConfig {
                    backend: StoreBackend::Sqlite,
                    ..Default::default()
                };
                let store = create_connection_store(cfg).await?;
                let trn_str = format!("trn:auth:openact:oauth2_cc:{}", trn); // matches AuthConnectionTrn format
                if let Some(conn) = store.get(&trn_str).await? {
                    if let Some(exp) = conn.expires_at {
                        if exp > Utc::now() + Duration::seconds(60) {
                            // skew
                            return Ok(Some((conn.access_token, exp)));
                        }
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn client_key(connection: &ConnectionConfig, task: &TaskConfig) -> String {
        let timeout = connection
            .timeout_config
            .as_ref()
            .or(task.timeout_config.as_ref());
        let network = connection
            .network_config
            .as_ref()
            .or(task.network_config.as_ref());
        let mut key = String::from("ua=OpenAct/0.1.0;");
        if let Some(t) = timeout {
            key.push_str(&format!(
                "ct={} rt={} tt={};",
                t.connect_ms, t.read_ms, t.total_ms
            ));
        }
        if let Some(n) = network {
            if let Some(p) = &n.proxy_url {
                key.push_str(&format!("proxy={};", p));
            }
            if let Some(tls) = &n.tls {
                key.push_str(&format!("vp={};", tls.verify_peer));
                key.push_str(&format!(
                    "ca={};",
                    tls.ca_pem.as_ref().map(|v| v.len()).unwrap_or(0)
                ));
                key.push_str(&format!("sn={};", tls.server_name.as_deref().unwrap_or("")));
            }
        }
        key
    }

    fn get_client_for(&self, connection: &ConnectionConfig, task: &TaskConfig) -> Result<Client> {
        let pool = CLIENT_POOL.get_or_init(|| Mutex::new(HashMap::new()));
        let key = Self::client_key(connection, task);
        // fast path: try lock and get
        if let Ok(guard) = pool.try_lock() {
            if let Some(c) = guard.get(&key) {
                return Ok(c.clone());
            }
        }
        // build new client
        let mut builder = Client::builder().user_agent("OpenAct/0.1.0");
        if let Some(t) = connection
            .timeout_config
            .as_ref()
            .or(task.timeout_config.as_ref())
        {
            builder = builder
                .connect_timeout(std::time::Duration::from_millis(t.connect_ms))
                .timeout(std::time::Duration::from_millis(t.total_ms));
        }
        if let Some(n) = connection
            .network_config
            .as_ref()
            .or(task.network_config.as_ref())
        {
            if let Some(p) = &n.proxy_url {
                builder =
                    builder.proxy(Proxy::all(p).map_err(|e| anyhow!("invalid proxy: {}", e))?);
            }
            if let Some(tls) = &n.tls {
                if !tls.verify_peer {
                    builder = builder.danger_accept_invalid_certs(true);
                }
                if let Some(ca) = &tls.ca_pem {
                    let cert = reqwest::Certificate::from_pem(ca)
                        .map_err(|e| anyhow!("invalid ca pem: {}", e))?;
                    builder = builder.add_root_certificate(cert);
                }
                // mTLS: 需要同时提供 client_cert_pem 与 client_key_pem
                if let (Some(cert_pem), Some(key_pem)) = (&tls.client_cert_pem, &tls.client_key_pem)
                {
                    // 将 cert 和 key 拼接为一个 PEM 文本，供 reqwest::Identity 解析
                    let mut combined = Vec::new();
                    combined.extend_from_slice(cert_pem);
                    if !combined.ends_with(b"\n") {
                        combined.extend_from_slice(b"\n");
                    }
                    combined.extend_from_slice(key_pem);
                    let id = reqwest::Identity::from_pem(&combined)
                        .map_err(|e| anyhow!("invalid client cert/key pem: {}", e))?;
                    builder = builder.identity(id);
                }
            }
        }
        let client = builder.build().context("Failed to create HTTP client")?;
        // store in pool (best-effort, avoid blocking in async context)
        if let Ok(mut guard) = pool.try_lock() {
            guard.insert(key, client.clone());
        }
        Ok(client)
    }

    async fn register_cc_inflight(
        trn: &str,
    ) -> Result<Option<oneshot::Receiver<anyhow::Result<(String, DateTime<Utc>)>>>> {
        let map = CC_INFLIGHT.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = map.lock().await;
        if let Some(waiters) = guard.get_mut(trn) {
            let (tx, rx) = oneshot::channel();
            waiters.push(tx);
            return Ok(Some(rx));
        } else {
            guard.insert(trn.to_string(), Vec::new());
            return Ok(None);
        }
    }

    async fn finish_cc_inflight(
        trn: &str,
        val: anyhow::Result<(String, DateTime<Utc>)>,
    ) -> Result<()> {
        let map = CC_INFLIGHT.get_or_init(|| Mutex::new(HashMap::new()));
        let mut guard = map.lock().await;
        if let Some(waiters) = guard.remove(trn) {
            for tx in waiters {
                let _ = tx.send(
                    val.as_ref()
                        .map(|(t, e)| (t.clone(), *e))
                        .map_err(|e| anyhow!("{}", e)),
                );
            }
        }
        Ok(())
    }

    /// 处理OAuth2 Authorization Code认证
    async fn handle_oauth2_authorization_code(
        &self,
        headers: &mut reqwest::header::HeaderMap,
        connection: &ConnectionConfig,
    ) -> Result<()> {
        let oauth_params = connection
            .auth_parameters
            .oauth_parameters
            .as_ref()
            .ok_or_else(|| {
                anyhow!(
                    "OAuth2 parameters missing for connection: {}",
                    connection.trn
                )
            })?;

        // 选择实际的存储（优先环境变量配置）
        let backend = match std::env::var("OPENACT_AUTH_BACKEND").ok().as_deref() {
            Some("sqlite") => StoreBackend::Sqlite,
            _ => StoreBackend::Memory,
        };
        let cfg = StoreConfig {
            backend,
            ..Default::default()
        };
        let store = create_connection_store(cfg).await?;
        // 使用现有的EnsureFreshTokenHandler来处理token刷新（接入DB或内存）
        let ensure_handler = EnsureFreshTokenHandler { store };
        let request_params = json!({
            "connection_ref": connection.trn,
            "tokenUrl": oauth_params.token_url,
            "clientId": oauth_params.client_id,
            "clientSecret": oauth_params.client_secret,
            "skewSeconds": 120  // 2分钟的安全边际
        });

        let result = ensure_handler
            .execute("ensure.fresh_token", "", &request_params)
            .context("Failed to ensure fresh OAuth2 token")?;

        // 提取access_token
        let access_token = result
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                anyhow!(
                    "OAuth2 Authorization Code token not found or expired. Please re-authorize connection: {}",
                    connection.trn
                )
            })?;

        // 注入Bearer token到Authorization header
        let auth_value = format!("Bearer {}", access_token);
        let header_value = HeaderValue::from_str(&auth_value)
            .map_err(|_| anyhow!("Invalid access token format"))?;
        headers.insert(AUTHORIZATION, header_value);

        Ok(())
    }

    /// 构建完整URL（包含query参数）
    fn build_url(&self, endpoint: &str, query_params: &HashMap<String, String>) -> Result<String> {
        if query_params.is_empty() {
            return Ok(endpoint.to_string());
        }

        let separator = if endpoint.contains('?') { "&" } else { "?" };
        let query_string = query_params
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        Ok(format!("{}{}{}", endpoint, separator, query_string))
    }
}

impl Default for HttpExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ApiKeyAuthParameters, BasicAuthParameters};
    use std::collections::HashMap;

    #[allow(dead_code)]
    fn create_api_key_connection() -> ConnectionConfig {
        let mut connection = ConnectionConfig::new(
            "trn:connection:api-key-test".to_string(),
            "API Key Test".to_string(),
            AuthorizationType::ApiKey,
        );

        connection.auth_parameters.api_key_auth_parameters = Some(ApiKeyAuthParameters {
            api_key_name: "X-API-Key".to_string(),
            api_key_value: "test-api-key-123".to_string(),
        });

        connection
    }

    #[allow(dead_code)]
    fn create_basic_auth_connection() -> ConnectionConfig {
        let mut connection = ConnectionConfig::new(
            "trn:connection:basic-test".to_string(),
            "Basic Auth Test".to_string(),
            AuthorizationType::Basic,
        );

        connection.auth_parameters.basic_auth_parameters = Some(BasicAuthParameters {
            username: "testuser".to_string(),
            password: "testpass".to_string(),
        });

        connection
    }

    #[allow(dead_code)]
    fn create_test_task() -> TaskConfig {
        TaskConfig::new(
            "trn:task:test".to_string(),
            "Test Task".to_string(),
            "trn:connection:test".to_string(),
            "https://api.example.com/users".to_string(),
            "GET".to_string(),
        )
    }

    #[test]
    fn test_build_url_no_params() {
        let executor = HttpExecutor::new();
        let params = HashMap::new();
        let url = executor
            .build_url("https://api.example.com/users", &params)
            .unwrap();
        assert_eq!(url, "https://api.example.com/users");
    }

    #[test]
    fn test_build_url_with_params() {
        let executor = HttpExecutor::new();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "10".to_string());
        params.insert("offset".to_string(), "20".to_string());

        let url = executor
            .build_url("https://api.example.com/users", &params)
            .unwrap();
        // URL参数顺序可能不同，所以检查包含关系
        assert!(url.starts_with("https://api.example.com/users?"));
        assert!(url.contains("limit=10"));
        assert!(url.contains("offset=20"));
    }

    #[test]
    fn test_build_url_existing_params() {
        let executor = HttpExecutor::new();
        let mut params = HashMap::new();
        params.insert("sort".to_string(), "name".to_string());

        let url = executor
            .build_url("https://api.example.com/users?existing=value", &params)
            .unwrap();
        assert!(url.contains("existing=value"));
        assert!(url.contains("sort=name"));
        assert!(url.contains("&"));
    }

    // 注意：实际的HTTP请求测试需要mock服务器，这里只测试URL构建逻辑
}
