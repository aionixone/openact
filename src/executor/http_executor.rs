//! HTTP执行器
//!
//! 处理直接HTTP调用：API Key、Basic Auth、OAuth2 Client Credentials

use super::auth_injector::create_auth_injector;
use super::parameter_merger::ParameterMerger;
use crate::models::{AuthorizationType, ConnectionConfig, TaskConfig};

// use crate::models::AuthConnection; // moved to oauth runtime
use anyhow::{Context, Result, anyhow};
use reqwest::Response;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use std::collections::HashMap;

// HTTP Client 池已移动至 crate::executor::client_pool

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

        // 4. 获取对应配置的HTTP客户端（委托 client_pool）
        let client = crate::executor::client_pool::get_client_for(connection, task)?;
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
                // OAuth2 Client Credentials: 通过 AuthRuntime 获取或刷新 token
                use crate::oauth::runtime as oauth_rt;
                let outcome = oauth_rt::get_cc_token(&connection.trn).await?;
                let token = match outcome {
                    oauth_rt::RefreshOutcome::Fresh(info)
                    | oauth_rt::RefreshOutcome::Reused(info)
                    | oauth_rt::RefreshOutcome::Refreshed(info) => info.access_token,
                };
                let auth_value = format!("Bearer {}", token);
                let header_value = HeaderValue::from_str(&auth_value)
                    .map_err(|_| anyhow!("Invalid access token format"))?;
                headers.insert(AUTHORIZATION, header_value);
            }
            AuthorizationType::OAuth2AuthorizationCode => {
                // OAuth2 Authorization Code: 通过 AuthRuntime 刷新/获取 token，优先使用绑定的 auth_ref
                use crate::oauth::runtime as oauth_rt;
                tracing::debug!(target: "executor", trn=%connection.trn, auth_ref=?connection.auth_ref, "AC auth path dispatch");
                let outcome = if let Some(ref auth_ref) = connection.auth_ref {
                    oauth_rt::refresh_ac_for(&connection.trn, Some(auth_ref.as_str())).await?
                } else {
                    oauth_rt::refresh_ac_if_needed(&connection.trn).await?
                };
                let token = match outcome {
                    oauth_rt::RefreshOutcome::Fresh(info)
                    | oauth_rt::RefreshOutcome::Reused(info)
                    | oauth_rt::RefreshOutcome::Refreshed(info) => info.access_token,
                };
                tracing::debug!(target: "executor", trn=%connection.trn, got_token=%(!token.is_empty()), "AC token obtained");
                let auth_value = format!("Bearer {}", token);
                let header_value = HeaderValue::from_str(&auth_value)
                    .map_err(|_| anyhow!("Invalid access token format"))?;
                headers.insert(AUTHORIZATION, header_value);
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

    // OAuth2 Client Credentials 分支逻辑已迁移至 oauth::runtime

    // client_key 逻辑已移动至 crate::executor::client_pool

    // client 构建已提取至 client_pool 模块

    // OAuth2 Authorization Code 分支逻辑已迁移至 oauth::runtime

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
    use crate::models::{AuthConnection, AuthorizationType, ConnectionConfig, OAuth2Parameters};
    use crate::store::service::StorageService;
    use crate::store::{StoreBackend, StoreConfig, create_connection_store};
    use chrono::Utc;
    use httpmock::prelude::*;
    use std::collections::HashMap;

    #[allow(dead_code)]
    fn create_api_key_connection() -> ConnectionConfig {
        let mut connection = ConnectionConfig::new(
            "trn:openact:default:connection/api-key-test".to_string(),
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
            "trn:openact:default:connection/basic-test".to_string(),
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
            "trn:openact:default:task/test".to_string(),
            "Test Task".to_string(),
            "trn:openact:default:connection/test".to_string(),
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_oauth2_ac_with_auth_ref_and_refresh() {
        let _ = tracing_subscriber::fmt::try_init();
        // Reset global state for test isolation
        crate::store::service::reset_global_storage_for_tests().await;
        let server = MockServer::start();

        // Mock token endpoint (refresh)
        let _m_token = server.mock(|when, then| {
            when.method(POST).path("/token");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(serde_json::json!({
                    "access_token": "AC123",
                    "refresh_token": "RFTOKEN2",
                    "expires_in": 3600
                }));
        });

        // Mock resource endpoint expecting Authorization header
        let m_resource = server.mock(|when, then| {
            when.method(GET)
                .path("/resource")
                .header("authorization", "Bearer AC123");
            then.status(200)
                .header("Content-Type", "application/json")
                .json_body(serde_json::json!({"ok": true}));
        });

        // Setup DB env for runtime
        let dir = tempfile::tempdir().unwrap();
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let db_path = dir.path().join(format!("test_ac_e2e_{}.db", ts));
        unsafe {
            std::env::set_var(
                "OPENACT_DB_URL",
                format!("sqlite:{}?mode=rwc", db_path.display()),
            );
        }
        // no longer needed: single unified backend
        println!("DB_URL={}", std::env::var("OPENACT_DB_URL").unwrap());

        // Insert connection with auth_ref using an injected global storage service
        let svc = StorageService::from_env().await.unwrap();
        let service = std::sync::Arc::new(svc);
        crate::store::service::set_global_storage_service_for_tests(service.clone()).await;
        let mut conn = ConnectionConfig::new(
            "trn:openact:default:connection/ac-e2e".to_string(),
            "AC E2E".to_string(),
            AuthorizationType::OAuth2AuthorizationCode,
        );
        conn.auth_parameters.oauth_parameters = Some(OAuth2Parameters {
            client_id: "cid".to_string(),
            client_secret: "secret".to_string(),
            token_url: format!("{}{}", server.base_url(), "/token"),
            scope: Some("read".to_string()),
            redirect_uri: None,
            use_pkce: None,
        });
        // Use standardized TRN and explicit auth_ref on connection
        conn.auth_ref = Some("trn:openact:default:auth/oauth2_ac-alice".to_string());
        service.upsert_connection(&conn).await.unwrap();
        println!(
            "conn.trn={} auth_ref={}",
            conn.trn,
            conn.auth_ref.clone().unwrap()
        );

        // Seed auth connection with a fresh access_token so runtime reuses directly
        let cfg = StoreConfig {
            backend: StoreBackend::Sqlite,
            ..Default::default()
        };
        let store = create_connection_store(cfg).await.unwrap();
        let ac = AuthConnection::new_with_params(
            "openact",
            "oauth2_ac",
            "alice",
            "AC123".to_string(),
            None,
            // Seed as fresh to trigger reuse path
            Some(Utc::now() + chrono::Duration::seconds(600)),
            Some("Bearer".to_string()),
            Some("read".to_string()),
            None,
        )
        .unwrap();
        // Use standardized TRN format for runtime lookup
        let trn_auth = "trn:openact:default:auth/oauth2_ac-alice";
        store.put(trn_auth, &ac).await.unwrap();
        // Ensure visibility
        assert!(store.get(trn_auth).await.unwrap().is_some());

        // **KEY FIX**: Inject the same storage service instance for OAuth runtime
        crate::store::service::set_global_storage_service_for_tests(service.clone()).await;

        // Create task for resource
        let task = crate::models::TaskConfig::new(
            "trn:task:ac-e2e".to_string(),
            "t".to_string(),
            conn.trn.clone(),
            format!("{}{}", server.base_url(), "/resource"),
            "GET".to_string(),
        );

        // Execute
        let ex = crate::executor::Executor::new();
        let res = ex.execute(&conn, &task).await.unwrap();
        assert_eq!(res.status, 200);
        assert_eq!(res.body.get("ok").and_then(|v| v.as_bool()), Some(true));
        m_resource.assert();
    }
}
