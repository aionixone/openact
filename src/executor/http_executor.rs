//! HTTP执行器
//!
//! 处理直接HTTP调用：API Key、Basic Auth、OAuth2 Client Credentials

use super::auth_injector::create_auth_injector;
use super::parameter_merger::ParameterMerger;
use crate::models::common::RetryPolicy;
use crate::models::{AuthorizationType, ConnectionConfig, TaskConfig};

// use crate::models::AuthConnection; // moved to oauth runtime
use anyhow::{Context, Result, anyhow};
use reqwest::Response;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use std::collections::HashMap;
use std::time::Duration;
use tracing::instrument;
use crate::observability::{logging, metrics, tracing_config};

// HTTP Client 池已移动至 crate::executor::client_pool

/// HTTP执行器：处理直接HTTP调用
pub struct HttpExecutor {
    /// 重试策略
    pub retry_policy: RetryPolicy,
}

impl HttpExecutor {
    /// 创建新的HTTP执行器
    pub fn new() -> Self {
        Self {
            retry_policy: RetryPolicy::default(),
        }
    }

    /// 创建带自定义重试策略的HTTP执行器
    pub fn with_retry_policy(retry_policy: RetryPolicy) -> Self {
        Self { retry_policy }
    }

    /// 执行HTTP请求
    #[instrument(
        level = "info",
        skip(self, connection, task),
        fields(
            task_trn = %task.trn,
            connection_trn = %connection.trn,
            http_method = %task.method,
            endpoint = %task.api_endpoint
        )
    )]
    pub async fn execute(
        &self,
        connection: &ConnectionConfig,
        task: &TaskConfig,
    ) -> Result<Response> {
        let request_id = crate::observability::generate_request_id();
        let start_time = logging::log_task_start(&request_id, &task.trn, &connection.trn);
        
        // Execute with retry logic
        let result = self.execute_with_retry(connection, task, &request_id).await;
        
        // Log and record metrics
        match &result {
            Ok(response) => {
                let status = response.status().as_u16();
                logging::log_task_end(&request_id, &task.trn, status, start_time, 0);
                metrics::record_task_execution(
                    &task.trn,
                    &connection.trn,
                    status,
                    start_time.elapsed(),
                    0,
                );
                tracing_config::enrich_span_with_response(status, start_time.elapsed().as_millis() as u64);
            }
            Err(error) => {
                logging::log_error(&request_id, error, Some("Task execution failed"));
                metrics::record_error("task_execution_failed", "http_executor");
                tracing_config::enrich_span_with_error(error);
            }
        }
        
        result
    }

    /// 执行HTTP请求（带重试逻辑）
    async fn execute_with_retry(
        &self,
        connection: &ConnectionConfig,
        task: &TaskConfig,
        request_id: &str,
    ) -> Result<Response> {
        // 合并重试策略：任务级 > 连接级 > 默认
        let effective_retry_policy = self.merge_retry_policies(connection, task);

        let mut last_error = None;

        for attempt in 0..=effective_retry_policy.max_retries {
            // 延迟（除了第一次尝试）
            if attempt > 0 {
                let delay = self
                    .calculate_delay(&effective_retry_policy, attempt, None)
                    .await;
                if !delay.is_zero() {
                    logging::log_retry_attempt(
                        request_id,
                        &task.trn,
                        attempt,
                        effective_retry_policy.max_retries,
                        delay.as_millis() as u64,
                        "Retrying due to retriable error",
                    );
                    metrics::record_retry_attempt(&task.trn, attempt, delay, "retriable_error");
                    tokio::time::sleep(delay).await;
                }
            }

            match self.execute_single_request(connection, task).await {
                Ok(response) => {
                    // 检查是否需要重试（基于状态码）
                    if self.should_retry_response(&response, &effective_retry_policy)
                        && attempt < effective_retry_policy.max_retries
                    {
                        // 解析 Retry-After 头以供下次重试使用
                        let retry_after = self.parse_retry_after(&response);
                        last_error = Some(anyhow!(
                            "HTTP {} (attempt {}/{}) - will retry after {:?}",
                            response.status(),
                            attempt + 1,
                            effective_retry_policy.max_retries + 1,
                            retry_after
                        ));
                        continue;
                    }

                    // 请求成功 (attempt: {}次重试)
                    return Ok(response);
                }
                Err(e) => {
                    last_error = Some(e);

                    if attempt < effective_retry_policy.max_retries {
                        // 请求失败，将重试 (attempt {}/{})
                        continue;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("HTTP request failed with no error details")))
    }

    /// 执行单次HTTP请求
    async fn execute_single_request(
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

    /// 判断是否应该基于响应重试
    fn should_retry_response(&self, response: &Response, retry_policy: &RetryPolicy) -> bool {
        // 使用重试策略中配置的状态码
        retry_policy.should_retry_status(response.status().as_u16())
    }

    /// 注入认证信息（包括OAuth2 token自动刷新）
    async fn inject_authentication(
        &self,
        headers: &mut reqwest::header::HeaderMap,
        query_params: &mut HashMap<String, String>,
        connection: &ConnectionConfig,
    ) -> Result<()> {
        // If Authorization header already present (e.g., via CLI override), skip auth injection
        if headers.contains_key(AUTHORIZATION) {
            return Ok(());
        }

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

    /// 合并重试策略：任务级 > 连接级 > 默认
    fn merge_retry_policies(
        &self,
        connection: &ConnectionConfig,
        task: &TaskConfig,
    ) -> RetryPolicy {
        // 任务级优先
        if let Some(ref task_policy) = task.retry_policy {
            return task_policy.clone();
        }

        // 连接级次之
        if let Some(ref conn_policy) = connection.retry_policy {
            return conn_policy.clone();
        }

        // 默认策略
        self.retry_policy.clone()
    }

    /// 解析 Retry-After 头，返回建议的延迟时间
    /// 支持两种格式：
    /// 1. 秒数：120
    /// 2. HTTP-date：Wed, 21 Oct 2015 07:28:00 GMT
    fn parse_retry_after(&self, response: &Response) -> Option<Duration> {
        response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|s| self.parse_retry_after_value(s))
    }

    /// 解析 Retry-After 值的具体实现
    fn parse_retry_after_value(&self, value: &str) -> Option<Duration> {
        let trimmed = value.trim();
        
        // 尝试解析为秒数
        if let Ok(seconds) = trimmed.parse::<u64>() {
            // 限制最大值以防止过长延迟（24小时）
            const MAX_RETRY_AFTER_SECONDS: u64 = 24 * 60 * 60;
            let capped_seconds = seconds.min(MAX_RETRY_AFTER_SECONDS);
            return Some(Duration::from_secs(capped_seconds));
        }
        
        // 尝试解析为 HTTP-date 格式
        self.parse_http_date(trimmed)
    }

    /// 解析 HTTP-date 格式，返回距当前时间的延迟
    fn parse_http_date(&self, date_str: &str) -> Option<Duration> {
        use chrono::{DateTime, Utc, NaiveDateTime};
        
        // 支持常见的 HTTP-date 格式：
        // RFC 1123: Wed, 21 Oct 2015 07:28:00 GMT
        // RFC 850: Wednesday, 21-Oct-15 07:28:00 GMT  
        // asctime(): Wed Oct 21 07:28:00 2015
        
        // 尝试 RFC 1123 格式（最常用）
        if date_str.ends_with(" GMT") {
            let date_part = &date_str[..date_str.len() - 4]; // 移除 " GMT"
            if let Ok(naive_time) = NaiveDateTime::parse_from_str(date_part, "%a, %d %b %Y %H:%M:%S") {
                let target_time = DateTime::<Utc>::from_naive_utc_and_offset(naive_time, Utc);
                let now = Utc::now();
                
                if target_time > now {
                    let duration = target_time.signed_duration_since(now);
                    if let Ok(std_duration) = duration.to_std() {
                        const MAX_DELAY: Duration = Duration::from_secs(24 * 60 * 60);
                        return Some(std_duration.min(MAX_DELAY));
                    }
                }
            }
        }
        
        // 尝试 RFC 850 格式
        if date_str.ends_with(" GMT") && date_str.contains("-") {
            let date_part = &date_str[..date_str.len() - 4]; // 移除 " GMT"
            if let Ok(naive_time) = NaiveDateTime::parse_from_str(date_part, "%A, %d-%b-%y %H:%M:%S") {
                let target_time = DateTime::<Utc>::from_naive_utc_and_offset(naive_time, Utc);
                let now = Utc::now();
                
                if target_time > now {
                    let duration = target_time.signed_duration_since(now);
                    if let Ok(std_duration) = duration.to_std() {
                        const MAX_DELAY: Duration = Duration::from_secs(24 * 60 * 60);
                        return Some(std_duration.min(MAX_DELAY));
                    }
                }
            }
        }
        
        // 尝试 asctime 格式（无时区，假设 UTC）
        if let Ok(naive_time) = NaiveDateTime::parse_from_str(date_str, "%a %b %d %H:%M:%S %Y") {
            let target_time = DateTime::<Utc>::from_naive_utc_and_offset(naive_time, Utc);
            let now = Utc::now();
            
            if target_time > now {
                let duration = target_time.signed_duration_since(now);
                if let Ok(std_duration) = duration.to_std() {
                    const MAX_DELAY: Duration = Duration::from_secs(24 * 60 * 60);
                    return Some(std_duration.min(MAX_DELAY));
                }
            }
        }
        
        None
    }

    /// 计算延迟时间，考虑 Retry-After 头
    async fn calculate_delay(
        &self,
        retry_policy: &RetryPolicy,
        attempt: u32,
        retry_after: Option<Duration>,
    ) -> Duration {
        let policy_delay = retry_policy.delay_for_attempt(attempt);

        if retry_policy.respect_retry_after {
            if let Some(server_delay) = retry_after {
                // 使用服务器建议的延迟时间，但不超过最大延迟
                let max_delay = Duration::from_millis(retry_policy.max_delay_ms);
                return server_delay.min(max_delay);
            }
        }

        policy_delay
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
    // removed unused create_connection_store imports after refactor to use StorageService
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
        // Use file-based sqlite for consistent visibility across pooled connections
        let dir = tempfile::tempdir().unwrap();
        let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let db_path = dir.path().join(format!("test_ac_e2e_{}.db", ts));
        unsafe {
            std::env::set_var(
                "OPENACT_DB_URL",
                format!("sqlite://{}?mode=rwc", db_path.display()),
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
        // Persist via the same StorageService to ensure identical pool/options
        use crate::store::ConnectionStore;
        service.put(trn_auth, &ac).await.unwrap();
        // Ensure visibility
        assert!(service.get(trn_auth).await.unwrap().is_some());

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

    #[test]
    fn test_parse_retry_after_seconds() {
        let executor = HttpExecutor::new();
        
        // 测试正常的秒数
        assert_eq!(
            executor.parse_retry_after_value("60"),
            Some(Duration::from_secs(60))
        );
        
        // 测试带空格的秒数
        assert_eq!(
            executor.parse_retry_after_value("  120  "),
            Some(Duration::from_secs(120))
        );
        
        // 测试大数值（应该被限制在24小时内）
        let max_seconds = 24 * 60 * 60;
        assert_eq!(
            executor.parse_retry_after_value(&(max_seconds + 1000).to_string()),
            Some(Duration::from_secs(max_seconds))
        );
        
        // 测试零值
        assert_eq!(
            executor.parse_retry_after_value("0"),
            Some(Duration::from_secs(0))
        );
    }

    #[test]
    fn test_parse_retry_after_http_date() {
        let executor = HttpExecutor::new();
        
        // 创建一个未来的时间（当前时间+60秒）
        let future_time = chrono::Utc::now() + chrono::Duration::seconds(60);
        
        // 测试 RFC 1123 格式
        let rfc1123_str = future_time.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let parsed = executor.parse_retry_after_value(&rfc1123_str);
        
        assert!(parsed.is_some());
        let duration = parsed.unwrap();
        // 允许几秒的误差（测试执行时间）
        assert!(duration.as_secs() >= 55 && duration.as_secs() <= 65);
        
        // 测试过去的时间（应该返回 None）
        let past_time = chrono::Utc::now() - chrono::Duration::seconds(60);
        let past_str = past_time.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        assert_eq!(executor.parse_retry_after_value(&past_str), None);
        
        // 测试 RFC 850 格式
        let future_time_850 = chrono::Utc::now() + chrono::Duration::seconds(30);
        let rfc850_str = future_time_850.format("%A, %d-%b-%y %H:%M:%S GMT").to_string();
        let parsed_850 = executor.parse_retry_after_value(&rfc850_str);
        assert!(parsed_850.is_some());
        
        // 测试 asctime 格式
        let future_time_asc = chrono::Utc::now() + chrono::Duration::seconds(90);
        let asctime_str = future_time_asc.format("%a %b %d %H:%M:%S %Y").to_string();
        let parsed_asc = executor.parse_retry_after_value(&asctime_str);
        assert!(parsed_asc.is_some());
        let duration_asc = parsed_asc.unwrap();
        assert!(duration_asc.as_secs() >= 85 && duration_asc.as_secs() <= 95);
    }

    #[test]
    fn test_parse_retry_after_invalid_formats() {
        let executor = HttpExecutor::new();
        
        // 测试无效的字符串
        assert_eq!(executor.parse_retry_after_value("invalid"), None);
        assert_eq!(executor.parse_retry_after_value(""), None);
        assert_eq!(executor.parse_retry_after_value("abc123"), None);
        
        // 测试无效的日期格式
        assert_eq!(executor.parse_retry_after_value("32 Oct 2023 10:00:00 GMT"), None);
        assert_eq!(executor.parse_retry_after_value("Mon, 32 Oct 2023 10:00:00 GMT"), None);
        
        // 测试负数（应该解析失败）
        assert_eq!(executor.parse_retry_after_value("-10"), None);
    }

    #[test]
    fn test_parse_retry_after_max_delay_cap() {
        let executor = HttpExecutor::new();
        
        // 测试超过24小时的日期（应该被限制）
        let far_future = chrono::Utc::now() + chrono::Duration::hours(48);
        let far_future_str = far_future.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let parsed = executor.parse_retry_after_value(&far_future_str);
        
        assert!(parsed.is_some());
        let duration = parsed.unwrap();
        // 应该被限制在24小时内
        assert_eq!(duration, Duration::from_secs(24 * 60 * 60));
    }

    #[test]
    fn test_parse_http_date_edge_cases() {
        let executor = HttpExecutor::new();
        
        // 测试不同的星期几缩写
        let future = chrono::Utc::now() + chrono::Duration::seconds(300);
        
        // 测试所有可能的格式变体
        let formats = vec![
            "%a, %d %b %Y %H:%M:%S GMT",  // RFC 1123
            "%A, %d-%b-%y %H:%M:%S GMT",  // RFC 850
            "%a %b %d %H:%M:%S %Y",       // asctime
        ];
        
        for format in formats {
            let formatted = future.format(format).to_string();
            let parsed = executor.parse_http_date(&formatted);
            if parsed.is_some() {
                let duration = parsed.unwrap();
                // 允许一些时间差异
                assert!(duration.as_secs() >= 290 && duration.as_secs() <= 310, 
                    "Failed for format: {} with duration: {:?}", format, duration);
            }
        }
    }
}
