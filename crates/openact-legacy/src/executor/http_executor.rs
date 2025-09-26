//! HTTP Executor
//!
//! Handles direct HTTP calls: API Key, Basic Auth, OAuth2 Client Credentials

use super::auth_injector::create_auth_injector;
use super::parameter_merger::ParameterMerger;
use crate::models::common::RetryPolicy;
use crate::models::{AuthorizationType, ConnectionConfig, TaskConfig};

// use crate::models::AuthConnection; // moved to oauth runtime
use crate::observability::{logging, metrics, tracing_config};
use anyhow::{Context, Result, anyhow};
use reqwest::Response;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use std::collections::HashMap;
use std::time::Duration;
use tracing::instrument;

// HTTP Client pool has been moved to crate::executor::client_pool

/// HTTP Executor: Handles direct HTTP calls
pub struct HttpExecutor {
    /// Retry policy
    pub retry_policy: RetryPolicy,
}

impl HttpExecutor {
    /// Create a new HTTP Executor
    pub fn new() -> Self {
        Self {
            retry_policy: RetryPolicy::default(),
        }
    }

    /// Create an HTTP Executor with a custom retry policy
    pub fn with_retry_policy(retry_policy: RetryPolicy) -> Self {
        Self { retry_policy }
    }

    /// Execute an HTTP request
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
                tracing_config::enrich_span_with_response(
                    status,
                    start_time.elapsed().as_millis() as u64,
                );
            }
            Err(error) => {
                logging::log_error(&request_id, error, Some("Task execution failed"));
                metrics::record_error("task_execution_failed", "http_executor");
                tracing_config::enrich_span_with_error(error);
            }
        }

        result
    }

    /// Execute an HTTP request (with retry logic)
    async fn execute_with_retry(
        &self,
        connection: &ConnectionConfig,
        task: &TaskConfig,
        request_id: &str,
    ) -> Result<Response> {
        // Merge retry policies: Task-level > Connection-level > Default
        let effective_retry_policy = self.merge_retry_policies(connection, task);

        let mut last_error = None;
        let mut retry_after_delay: Option<Duration> = None;

        for attempt in 0..=effective_retry_policy.max_retries {
            // Delay (except for the first attempt)
            if attempt > 0 {
                let delay = self
                    .calculate_delay(&effective_retry_policy, attempt, retry_after_delay)
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
                    // Check if a retry is needed (based on status code)
                    if self.should_retry_response(&response, &effective_retry_policy)
                        && attempt < effective_retry_policy.max_retries
                    {
                        // Parse Retry-After header for the next retry
                        retry_after_delay = self.parse_retry_after(&response);
                        last_error = Some(anyhow!(
                            "HTTP {} (attempt {}/{}) - will retry after {:?}",
                            response.status(),
                            attempt + 1,
                            effective_retry_policy.max_retries + 1,
                            retry_after_delay
                        ));
                        continue;
                    }

                    // Request succeeded (attempt: {} retries)
                    return Ok(response);
                }
                Err(e) => {
                    last_error = Some(e);

                    if attempt < effective_retry_policy.max_retries {
                        // Request failed, will retry (attempt {}/{})
                        continue;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("HTTP request failed with no error details")))
    }

    /// Execute a single HTTP request
    async fn execute_single_request(
        &self,
        connection: &ConnectionConfig,
        task: &TaskConfig,
    ) -> Result<Response> {
        // 1. Merge parameters (ConnectionWins strategy)
        let mut merged =
            ParameterMerger::merge(connection, task).context("Failed to merge parameters")?;

        // 2. Inject authentication
        self.inject_authentication(&mut merged.headers, &mut merged.query_params, connection)
            .await
            .context("Failed to inject authentication")?;

        // 3. Build the complete URL
        let url = self.build_url(&merged.endpoint, &merged.query_params)?;

        // 4. Get the HTTP client for the configuration (delegated to client_pool)
        let client = crate::executor::client_pool::get_client_for(connection, task)?;

        // 5. Build the HTTP request
        let mut request_builder = client
            .request(
                merged
                    .method
                    .parse()
                    .map_err(|e| anyhow!("Invalid HTTP method '{}': {}", merged.method, e))?,
                url,
            )
            .headers(merged.headers);

        // 6. Add request body (if any)
        if let Some(body) = merged.body {
            request_builder = request_builder.json(&body);
        }

        // 7. Execute the request
        let response = request_builder
            .send()
            .await
            .context("Failed to send HTTP request")?;

        Ok(response)
    }

    /// Determine if a retry should be based on the response
    fn should_retry_response(&self, response: &Response, retry_policy: &RetryPolicy) -> bool {
        // Use status codes configured in the retry policy
        retry_policy.should_retry_status(response.status().as_u16())
    }

    /// Inject authentication (including OAuth2 token auto-refresh)
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
                // OAuth2 Client Credentials: Get or refresh token via AuthRuntime
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
                // OAuth2 Authorization Code: Refresh/get token via AuthRuntime, prefer using bound auth_ref
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
                // API Key and Basic Auth: Direct injection, no token refresh needed
                let injector = create_auth_injector(&connection.authorization_type);
                injector
                    .inject_auth(headers, query_params, connection)
                    .map_err(|e| anyhow!("Authentication injection failed: {}", e))?;
            }
        }

        Ok(())
    }

    // OAuth2 Client Credentials branch logic has been moved to oauth::runtime

    // client_key logic has been moved to crate::executor::client_pool

    // client construction has been extracted to client_pool module

    // OAuth2 Authorization Code branch logic has been moved to oauth::runtime

    /// Build the complete URL (including query parameters)
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

    /// Merge retry policies: Task-level > Connection-level > Default
    fn merge_retry_policies(
        &self,
        connection: &ConnectionConfig,
        task: &TaskConfig,
    ) -> RetryPolicy {
        // Task-level takes precedence
        if let Some(ref task_policy) = task.retry_policy {
            return task_policy.clone();
        }

        // Connection-level is next
        if let Some(ref conn_policy) = connection.retry_policy {
            return conn_policy.clone();
        }

        // Default policy
        self.retry_policy.clone()
    }

    /// Parse the Retry-After header, returning the suggested delay
    /// Supports two formats:
    /// 1. Seconds: 120
    /// 2. HTTP-date: Wed, 21 Oct 2015 07:28:00 GMT
    fn parse_retry_after(&self, response: &Response) -> Option<Duration> {
        response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|s| self.parse_retry_after_value(s))
    }

    /// Implementation of parsing the Retry-After value
    fn parse_retry_after_value(&self, value: &str) -> Option<Duration> {
        let trimmed = value.trim();

        // Try to parse as seconds
        if let Ok(seconds) = trimmed.parse::<u64>() {
            // Limit the maximum value to prevent excessive delay (24 hours)
            const MAX_RETRY_AFTER_SECONDS: u64 = 24 * 60 * 60;
            let capped_seconds = seconds.min(MAX_RETRY_AFTER_SECONDS);
            return Some(Duration::from_secs(capped_seconds));
        }

        // Try to parse as HTTP-date format
        self.parse_http_date(trimmed)
    }

    /// Parse HTTP-date format, returning the delay from the current time
    fn parse_http_date(&self, date_str: &str) -> Option<Duration> {
        use chrono::{DateTime, NaiveDateTime, Utc};

        // Support common HTTP-date formats:
        // RFC 1123: Wed, 21 Oct 2015 07:28:00 GMT
        // RFC 850: Wednesday, 21-Oct-15 07:28:00 GMT
        // asctime(): Wed Oct 21 07:28:00 2015

        // Try RFC 1123 format (most common)
        if date_str.ends_with(" GMT") {
            let date_part = &date_str[..date_str.len() - 4]; // Remove " GMT"
            if let Ok(naive_time) =
                NaiveDateTime::parse_from_str(date_part, "%a, %d %b %Y %H:%M:%S")
            {
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

        // Try RFC 850 format
        if date_str.ends_with(" GMT") && date_str.contains("-") {
            let date_part = &date_str[..date_str.len() - 4]; // Remove " GMT"
            if let Ok(naive_time) =
                NaiveDateTime::parse_from_str(date_part, "%A, %d-%b-%y %H:%M:%S")
            {
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

        // Try asctime format (no timezone, assume UTC)
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

    /// Calculate the delay time, considering the Retry-After header
    async fn calculate_delay(
        &self,
        retry_policy: &RetryPolicy,
        attempt: u32,
        retry_after: Option<Duration>,
    ) -> Duration {
        let policy_delay = retry_policy.delay_for_attempt(attempt);

        if retry_policy.respect_retry_after {
            if let Some(server_delay) = retry_after {
                // Use the server-suggested delay, but do not exceed the maximum delay
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
        // The order of URL parameters may vary, so check for containment
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

    // Note: Actual HTTP request tests require a mock server, here we only test URL construction logic

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

        // Test normal seconds
        assert_eq!(
            executor.parse_retry_after_value("60"),
            Some(Duration::from_secs(60))
        );

        // Test seconds with spaces
        assert_eq!(
            executor.parse_retry_after_value("  120  "),
            Some(Duration::from_secs(120))
        );

        // Test large value (should be capped at 24 hours)
        let max_seconds = 24 * 60 * 60;
        assert_eq!(
            executor.parse_retry_after_value(&(max_seconds + 1000).to_string()),
            Some(Duration::from_secs(max_seconds))
        );

        // Test zero value
        assert_eq!(
            executor.parse_retry_after_value("0"),
            Some(Duration::from_secs(0))
        );
    }

    #[test]
    fn test_parse_retry_after_http_date() {
        let executor = HttpExecutor::new();

        // Create a future time (current time + 60 seconds)
        let future_time = chrono::Utc::now() + chrono::Duration::seconds(60);

        // Test RFC 1123 format
        let rfc1123_str = future_time.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let parsed = executor.parse_retry_after_value(&rfc1123_str);

        assert!(parsed.is_some());
        let duration = parsed.unwrap();
        // Allow a few seconds of error (test execution time)
        assert!(duration.as_secs() >= 55 && duration.as_secs() <= 65);

        // Test past time (should return None)
        let past_time = chrono::Utc::now() - chrono::Duration::seconds(60);
        let past_str = past_time.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        assert_eq!(executor.parse_retry_after_value(&past_str), None);

        // Test RFC 850 format
        let future_time_850 = chrono::Utc::now() + chrono::Duration::seconds(30);
        let rfc850_str = future_time_850
            .format("%A, %d-%b-%y %H:%M:%S GMT")
            .to_string();
        let parsed_850 = executor.parse_retry_after_value(&rfc850_str);
        assert!(parsed_850.is_some());

        // Test asctime format
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

        // Test invalid strings
        assert_eq!(executor.parse_retry_after_value("invalid"), None);
        assert_eq!(executor.parse_retry_after_value(""), None);
        assert_eq!(executor.parse_retry_after_value("abc123"), None);

        // Test invalid date formats
        assert_eq!(
            executor.parse_retry_after_value("32 Oct 2023 10:00:00 GMT"),
            None
        );
        assert_eq!(
            executor.parse_retry_after_value("Mon, 32 Oct 2023 10:00:00 GMT"),
            None
        );

        // Test negative numbers (should fail to parse)
        assert_eq!(executor.parse_retry_after_value("-10"), None);
    }

    #[test]
    fn test_parse_retry_after_max_delay_cap() {
        let executor = HttpExecutor::new();

        // Test date beyond 24 hours (should be capped)
        let far_future = chrono::Utc::now() + chrono::Duration::hours(48);
        let far_future_str = far_future.format("%a, %d %b %Y %H:%M:%S GMT").to_string();
        let parsed = executor.parse_retry_after_value(&far_future_str);

        assert!(parsed.is_some());
        let duration = parsed.unwrap();
        // Should be capped at 24 hours
        assert_eq!(duration, Duration::from_secs(24 * 60 * 60));
    }

    #[test]
    fn test_parse_http_date_edge_cases() {
        let executor = HttpExecutor::new();

        // Test different day of the week abbreviations
        let future = chrono::Utc::now() + chrono::Duration::seconds(300);

        // Test all possible format variants
        let formats = vec![
            "%a, %d %b %Y %H:%M:%S GMT", // RFC 1123
            "%A, %d-%b-%y %H:%M:%S GMT", // RFC 850
            "%a %b %d %H:%M:%S %Y",      // asctime
        ];

        for format in formats {
            let formatted = future.format(format).to_string();
            let parsed = executor.parse_http_date(&formatted);
            if parsed.is_some() {
                let duration = parsed.unwrap();
                // Allow some time variance
                assert!(
                    duration.as_secs() >= 290 && duration.as_secs() <= 310,
                    "Failed for format: {} with duration: {:?}",
                    format,
                    duration
                );
            }
        }
    }

    #[tokio::test]
    async fn test_retry_after_integration() {
        use crate::models::RetryPolicy;

        let executor = HttpExecutor::new();

        // Test delay calculation logic (no actual network request needed)
        let retry_policy = RetryPolicy {
            max_retries: 3,
            base_delay_ms: 100,
            max_delay_ms: 5000,
            backoff_multiplier: 2.0,
            retry_status_codes: vec![429, 500, 502, 503, 504],
            respect_retry_after: true,
        };

        // Test delay without Retry-After
        let delay_no_retry_after = executor.calculate_delay(&retry_policy, 1, None).await;
        assert_eq!(delay_no_retry_after, Duration::from_millis(100)); // 100 * 2^0 = 100ms (attempt 1)

        // Test delay with Retry-After (server suggests shorter)
        let server_delay = Duration::from_millis(50);
        let delay_with_retry_after = executor
            .calculate_delay(&retry_policy, 1, Some(server_delay))
            .await;
        assert_eq!(delay_with_retry_after, server_delay);

        // Test Retry-After exceeding max delay (should be capped)
        let long_server_delay = Duration::from_millis(10000);
        let delay_capped = executor
            .calculate_delay(&retry_policy, 1, Some(long_server_delay))
            .await;
        assert_eq!(delay_capped, Duration::from_millis(5000)); // max_delay_ms

        // Test delay calculation for second retry
        let delay_attempt_2 = executor.calculate_delay(&retry_policy, 2, None).await;
        assert_eq!(delay_attempt_2, Duration::from_millis(200)); // 100 * 2^1 = 200ms (attempt 2)

        // Test with respect_retry_after disabled
        let retry_policy_no_respect = RetryPolicy {
            max_retries: 3,
            base_delay_ms: 100,
            max_delay_ms: 5000,
            backoff_multiplier: 2.0,
            retry_status_codes: vec![429, 500, 502, 503, 504],
            respect_retry_after: false,
        };
        let delay_ignored = executor
            .calculate_delay(&retry_policy_no_respect, 1, Some(Duration::from_millis(50)))
            .await;
        assert_eq!(delay_ignored, Duration::from_millis(100)); // Should ignore Retry-After, use policy delay
    }
}
