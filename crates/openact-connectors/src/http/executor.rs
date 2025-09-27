use super::actions::HttpAction;
use super::client_cache::ClientCache;
use super::connection::HttpConnection;
use super::oauth::OAuth2TokenManager;
use super::timeout_manager::TimeoutManager;
use super::retry_manager::{RetryManager, RetryDecision};
use super::url_builder::UrlBuilder;
use super::body_builder::{BodyBuilder, RequestBodyType, HttpRequestBody};
// PolicyManager will be used for header/query merging - temporarily commented out
// use super::policy_manager::PolicyManager;
use crate::auth::AuthConnectionStore;
use crate::error::{ConnectorError, ConnectorResult};
use reqwest::{Method, Request, Response};
use serde_json::Value as JsonValue;
use std::str::FromStr;
// Duration is used in tokio::time::sleep calls

/// HTTP executor that processes HTTP actions using connection configurations
pub struct HttpExecutor<S = ()> {
    client_cache: ClientCache,
    auth_store: Option<S>,
}

/// Execution result containing response data
#[derive(Debug, Clone)]
pub struct HttpExecutionResult {
    pub status_code: u16,
    pub headers: std::collections::HashMap<String, String>,
    pub body: JsonValue,
    pub execution_time_ms: u64,
}

impl HttpExecutor<()> {
    /// Create a new HTTP executor with default settings (no auth store)
    pub fn new() -> ConnectorResult<Self> {
        Ok(Self {
            client_cache: ClientCache::new(),
            auth_store: None,
        })
    }
}

impl<S: AuthConnectionStore> HttpExecutor<S> {
    /// Create a new HTTP executor with auth store
    pub fn new_with_auth_store(auth_store: S) -> ConnectorResult<Self> {
        Ok(Self {
            client_cache: ClientCache::new(),
            auth_store: Some(auth_store),
        })
    }

    /// Execute an HTTP action using the provided connection configuration
    pub async fn execute(
        &self,
        connection: &HttpConnection,
        action: &HttpAction,
        input: Option<JsonValue>,
    ) -> ConnectorResult<HttpExecutionResult> {
        let start_time = std::time::Instant::now();

        // Merge configurations (connection < action < input)
        let merged_config = self.merge_configs(connection, action, input.as_ref())?;

        // Get cached client for this connection
        let client = self.client_cache.get_client(connection)?;

        // Build the request
        let request = self.build_request(connection, &merged_config).await?;

        // Create timeout manager for this request
        let timeout_manager = TimeoutManager::new(merged_config.timeout.clone());
        timeout_manager.validate()?;
        
        // Create retry manager for this request
        let retry_manager = RetryManager::new(merged_config.retry_policy.clone());
        
        // Execute the request with retry logic
        let result = self
            .execute_with_retry(client, &timeout_manager, &retry_manager, request, start_time)
            .await?;

        Ok(result)
    }

    /// Merge connection and action configurations with input override
    fn merge_configs(
        &self,
        connection: &HttpConnection,
        action: &HttpAction,
        input: Option<&JsonValue>,
    ) -> ConnectorResult<MergedConfig> {
        // Start with connection defaults
        let timeout = action
            .timeout_config
            .as_ref()
            .or(connection.timeout_config.as_ref())
            .cloned()
            .unwrap_or_default();

        let retry_policy = action
            .retry_policy
            .as_ref()
            .or(connection.retry_policy.as_ref())
            .cloned()
            .unwrap_or_default();

        // Merge headers: connection < action < input (with null deletion)
        let headers = self.merge_headers_with_input(connection, action, input)?;

        // Merge query parameters: connection < action < input (with null deletion)
        let query_params = self.merge_query_with_input(connection, action, input)?;

        // Build URL using proper URL joining
        let url = UrlBuilder::join(&connection.base_url, &action.path)?;

        Ok(MergedConfig {
            method: action.method.clone(),
            url,
            headers,
            query_params,
            body: action.request_body.clone(),
            typed_body: action.body.clone(),
            timeout,
            retry_policy,
            auth: connection.authorization.clone(),
            auth_parameters: connection.auth_parameters.clone(),
            auth_ref: connection.auth_ref.clone(),
        })
    }

    /// Build the HTTP request
    async fn build_request(
        &self,
        connection: &HttpConnection,
        config: &MergedConfig,
    ) -> ConnectorResult<Request> {
        // Parse HTTP method
        let method = Method::from_str(&config.method).map_err(|_| {
            ConnectorError::InvalidConfig(format!("Invalid HTTP method: {}", config.method))
        })?;

        // Build URL with query parameters
        let mut url = reqwest::Url::parse(&config.url)
            .map_err(|e| ConnectorError::InvalidConfig(format!("Invalid URL: {}", e)))?;

        for (key, value) in &config.query_params {
            url.query_pairs_mut().append_pair(key, value);
        }

        // Get cached client for this connection
        let client = self.client_cache.get_client(connection)?;

        // Start building the request
        let mut request_builder = client.request(method, url);

        // Add headers
        for (key, value) in &config.headers {
            request_builder = request_builder.header(key, value);
        }

        // Add authentication
        request_builder = self
            .apply_authentication(
                request_builder,
                &config.auth,
                &config.auth_parameters,
                config.auth_ref.as_deref(),
            )
            .await?;

        // Add body if present - prioritize typed_body over legacy body
        if let Some(typed_body) = &config.typed_body {
            // Use new typed body system
            let http_body = BodyBuilder::build(typed_body).await
                .map_err(|e| ConnectorError::InvalidConfig(format!("Body build failed: {}", e)))?;
            
            match http_body {
                HttpRequestBody::Body { body, content_type, content_length } => {
                    // Set content type header if not already set
                    if !config.headers.iter().any(|(k, _)| k.to_lowercase() == "content-type") {
                        request_builder = request_builder.header("content-type", &content_type);
                    }
                    
                    // Set content length if known
                    if let Some(length) = content_length {
                        request_builder = request_builder.header("content-length", length.to_string());
                    }
                    
                    request_builder = request_builder.body(body);
                }
                HttpRequestBody::Multipart { form } => {
                    // For multipart, reqwest handles content-type and boundary automatically
                    request_builder = request_builder.multipart(form);
                }
            }
        } else if let Some(body) = &config.body {
            // Legacy JSON body support
            request_builder = request_builder.json(body);
        }

        // NOTE: Timeout is now handled by TimeoutManager, not set on individual requests
        // Connection timeout is set on the client level in ClientCache
        // Request timeout will be applied by TimeoutManager.execute_with_timeout()

        // Build the final request
        let request = request_builder.build()?;

        Ok(request)
    }

    /// Apply authentication to the request
    async fn apply_authentication(
        &self,
        mut builder: reqwest::RequestBuilder,
        auth_type: &super::connection::AuthorizationType,
        auth_params: &super::connection::AuthParameters,
        auth_ref: Option<&str>,
    ) -> ConnectorResult<reqwest::RequestBuilder> {
        use super::connection::AuthorizationType;

        match auth_type {
            AuthorizationType::ApiKey => {
                if let Some(api_key_params) = &auth_params.api_key_auth_parameters {
                    // Support both header and query parameter API keys
                    if api_key_params
                        .api_key_name
                        .to_lowercase()
                        .contains("authorization")
                    {
                        // If the key name suggests it's an Authorization header
                        builder = builder.header(
                            "Authorization",
                            format!("Bearer {}", api_key_params.api_key_value),
                        );
                    } else if api_key_params.api_key_name.to_lowercase().starts_with("x-")
                        || api_key_params.api_key_name.to_lowercase().contains("key")
                    {
                        // Custom header (X-API-Key, etc.)
                        builder = builder
                            .header(&api_key_params.api_key_name, &api_key_params.api_key_value);
                    } else {
                        // Query parameter
                        builder = builder.query(&[(
                            api_key_params.api_key_name.as_str(),
                            api_key_params.api_key_value.as_str(),
                        )]);
                    }
                }
            }
            AuthorizationType::Basic => {
                if let Some(basic_params) = &auth_params.basic_auth_parameters {
                    builder =
                        builder.basic_auth(&basic_params.username, Some(&basic_params.password));
                }
            }
            AuthorizationType::OAuth2ClientCredentials
            | AuthorizationType::OAuth2AuthorizationCode => {
                // For OAuth2, we need to get the actual access token
                if let Some(oauth_params) = &auth_params.oauth_parameters {
                    if let Some(auth_ref) = auth_ref {
                        // Use OAuth2TokenManager to get access token from auth_connections
                        if let Some(auth_store) = &self.auth_store {
                            let token_info = self
                                .get_oauth_token(auth_store, auth_ref, oauth_params, auth_type)
                                .await?;
                            builder = builder.bearer_auth(&token_info.access_token);
                        } else {
                            return Err(ConnectorError::InvalidConfig(
                                "OAuth2 authentication requires auth store to be configured"
                                    .to_string(),
                            ));
                        }
                    } else {
                        // Fallback for client credentials flow without auth_ref
                        if matches!(auth_type, AuthorizationType::OAuth2ClientCredentials) {
                            // For client credentials, we can fetch token directly without auth_ref
                            if let Some(auth_store) = &self.auth_store {
                                let token_info = self
                                    .client_credentials_flow(auth_store, oauth_params)
                                    .await?;
                                builder = builder.bearer_auth(&token_info.access_token);
                            } else {
                                // If no auth store, try to use OAuth parameters directly as bearer token
                                if oauth_params.client_secret.starts_with("ghp_")
                                    || oauth_params.client_secret.starts_with("gho_")
                                {
                                    // GitHub personal access token pattern
                                    builder = builder.bearer_auth(&oauth_params.client_secret);
                                } else {
                                    return Err(ConnectorError::InvalidConfig(
                                        "OAuth2 client credentials requires either auth store or a valid access token in client_secret field".to_string()
                                    ));
                                }
                            }
                        } else {
                            return Err(ConnectorError::InvalidConfig(
                                "OAuth2 authorization code flow requires auth_ref".to_string(),
                            ));
                        }
                    }
                } else {
                    return Err(ConnectorError::InvalidConfig(
                        "OAuth2 authentication requires OAuth parameters".to_string(),
                    ));
                }
            }
        }

        Ok(builder)
    }

    /// Get OAuth2 access token using OAuth2TokenManager
    async fn get_oauth_token(
        &self,
        auth_store: &S,
        auth_ref: &str,
        oauth_params: &super::connection::OAuth2Parameters,
        auth_type: &super::connection::AuthorizationType,
    ) -> ConnectorResult<crate::auth::TokenInfo> {
        let oauth_manager = OAuth2TokenManager::new(auth_store as &dyn AuthConnectionStore);
        oauth_manager
            .get_access_token(auth_ref, oauth_params, auth_type)
            .await
    }

    /// Perform OAuth2 client credentials flow
    async fn client_credentials_flow(
        &self,
        auth_store: &S,
        oauth_params: &super::connection::OAuth2Parameters,
    ) -> ConnectorResult<crate::auth::TokenInfo> {
        let oauth_manager = OAuth2TokenManager::new(auth_store as &dyn AuthConnectionStore);
        oauth_manager.client_credentials_flow(oauth_params).await
    }

    /// Execute request with advanced retry logic, timeout handling, and error classification
    async fn execute_with_retry(
        &self,
        client: std::sync::Arc<reqwest::Client>,
        timeout_manager: &TimeoutManager,
        retry_manager: &RetryManager,
        request: Request,
        start_time: std::time::Instant,
    ) -> ConnectorResult<HttpExecutionResult> {
        let mut last_error: Option<ConnectorError> = None;
        let mut attempt = 0;

        loop {
            // Clone the request for this attempt (reqwest Request can only be used once)
            let request_clone = match request.try_clone() {
                Some(req) => req,
                None => {
                    // If we can't clone the request (e.g., it has a body stream),
                    // we can only make one attempt
                    if attempt == 0 {
                        return self
                            .execute_single_request(&client, timeout_manager, request, start_time)
                            .await;
                    } else {
                        return Err(last_error.unwrap_or_else(|| {
                            ConnectorError::ExecutionFailed(
                                "Cannot retry request with streaming body".to_string(),
                            )
                        }));
                    }
                }
            };

            match self
                .execute_single_request(&client, timeout_manager, request_clone, start_time)
                .await
            {
                Ok(result) => {
                    // Check if this is a retryable status code
                    if retry_manager.get_policy()
                        .retry_on_status_codes
                        .contains(&result.status_code)
                    {
                        // Create a status code error for retry decision
                        let status_error = ConnectorError::ExecutionFailed(format!(
                            "HTTP {} (retryable status code)",
                            result.status_code
                        ));
                        last_error = Some(status_error);
                    } else {
                        // Success or non-retryable status code
                        return Ok(result);
                    }
                }
                Err(err) => {
                    last_error = Some(err);
                }
            }

            attempt += 1;

            // Use RetryManager to decide if we should retry
            let current_error = last_error.as_ref().unwrap();
            match retry_manager.should_retry(current_error, attempt, start_time) {
                RetryDecision::Retry { delay, attempt_info } => {
                    println!(
                        "Retrying request (attempt {}) after {}ms due to: {}", 
                        attempt_info.attempt_number, 
                        delay.as_millis(),
                        current_error
                    );
                    attempt = attempt_info.attempt_number;
                    tokio::time::sleep(delay).await;
                    continue;
                }
                RetryDecision::Stop { reason, .. } => {
                    return Err(ConnectorError::ExecutionFailed(format!(
                        "Retry stopped: {} (error: {})",
                        reason, current_error
                    )));
                }
            }
        }
    }

    /// Execute a single HTTP request without retry, with proper timeout handling
    async fn execute_single_request(
        &self,
        client: &reqwest::Client,
        timeout_manager: &TimeoutManager,
        request: Request,
        start_time: std::time::Instant,
    ) -> ConnectorResult<HttpExecutionResult> {
        // Use TimeoutManager for proper timeout handling
        let response = timeout_manager
            .execute_with_timeout(client.execute(request))
            .await?;
        
        self.process_response(response, start_time).await
    }

    /// Process the HTTP response
    async fn process_response(
        &self,
        response: Response,
        start_time: std::time::Instant,
    ) -> ConnectorResult<HttpExecutionResult> {
        let status_code = response.status().as_u16();

        // Extract headers
        let mut headers = std::collections::HashMap::new();
        for (name, value) in response.headers() {
            if let Ok(value_str) = value.to_str() {
                headers.insert(name.to_string(), value_str.to_string());
            }
        }

        // Extract body as JSON
        let body_text = response.text().await?;
        let body: JsonValue = if body_text.is_empty() {
            JsonValue::Null
        } else {
            serde_json::from_str(&body_text).unwrap_or_else(|_| JsonValue::String(body_text))
        };

        let execution_time_ms = start_time.elapsed().as_millis() as u64;

        Ok(HttpExecutionResult {
            status_code,
            headers,
            body,
            execution_time_ms,
        })
    }

    /// Merge headers across connection < action < input with null deletion support
    fn merge_headers_with_input(
        &self,
        connection: &HttpConnection,
        action: &HttpAction,
        input: Option<&JsonValue>,
    ) -> ConnectorResult<std::collections::HashMap<String, String>> {
        let mut headers = std::collections::HashMap::new();

        // Layer 1: Connection default headers
        if let Some(invocation_params) = &connection.invocation_http_parameters {
            for param in &invocation_params.header_parameters {
                headers.insert(param.key.clone(), param.value.clone());
            }
        }

        // Layer 2: Action headers (with policy checks)
        if let Some(action_headers) = &action.headers {
            for (key, values) in action_headers {
                if let Some(first_value) = values.first() {
                    // Apply basic policy checks
                    if let Some(http_policy) = &connection.http_policy {
                        let normalized_key = key.to_lowercase();
                        
                        // Check if header is denied
                        if http_policy.denied_headers.iter().any(|h| h.to_lowercase() == normalized_key) {
                            continue; // Skip denied headers
                        }
                        
                        // Check if this is a reserved header (connection takes precedence)
                        if http_policy.reserved_headers.iter().any(|h| h.to_lowercase() == normalized_key) {
                            if headers.contains_key(key) {
                                continue; // Keep connection value
                            }
                        }
                        
                        // Check if this is a multi-value append header
                        if http_policy.multi_value_append_headers.iter().any(|h| h.to_lowercase() == normalized_key) {
                            if let Some(existing_value) = headers.get(key) {
                                headers.insert(key.clone(), format!("{}, {}", existing_value, first_value));
                                continue;
                            }
                        }
                    }
                    
                    headers.insert(key.clone(), first_value.clone());
                }
            }
        }

        // Layer 3: Input headers (highest priority, supports null deletion)
        if let Some(input_obj) = input {
            if let Some(input_headers) = input_obj.get("headers") {
                if let Some(headers_map) = input_headers.as_object() {
                    for (key, value) in headers_map {
                        if value.is_null() {
                            // null means delete the key
                            headers.remove(key);
                        } else if let Some(str_value) = value.as_str() {
                            headers.insert(key.clone(), str_value.to_string());
                        }
                    }
                }
            }
        }

        Ok(headers)
    }

    /// Merge query parameters across connection < action < input with null deletion support
    fn merge_query_with_input(
        &self,
        connection: &HttpConnection,
        action: &HttpAction,
        input: Option<&JsonValue>,
    ) -> ConnectorResult<std::collections::HashMap<String, String>> {
        let mut query_params = std::collections::HashMap::new();

        // Layer 1: Connection default query params
        if let Some(invocation_params) = &connection.invocation_http_parameters {
            for param in &invocation_params.query_string_parameters {
                query_params.insert(param.key.clone(), param.value.clone());
            }
        }

        // Layer 2: Action query params
        if let Some(action_query) = &action.query_params {
            for (key, values) in action_query {
                if let Some(first_value) = values.first() {
                    query_params.insert(key.clone(), first_value.clone());
                }
            }
        }

        // Layer 3: Input query params (highest priority, supports null deletion)
        if let Some(input_obj) = input {
            if let Some(input_query) = input_obj.get("query") {
                if let Some(query_map) = input_query.as_object() {
                    for (key, value) in query_map {
                        if value.is_null() {
                            // null means delete the key
                            query_params.remove(key);
                        } else if let Some(str_value) = value.as_str() {
                            query_params.insert(key.clone(), str_value.to_string());
                        }
                    }
                }
            }
        }

        Ok(query_params)
    }
}

impl Default for HttpExecutor {
    fn default() -> Self {
        Self::new().expect("Failed to create default HTTP executor")
    }
}

/// Internal merged configuration structure
#[derive(Debug, Clone)]
struct MergedConfig {
    method: String,
    url: String,
    headers: std::collections::HashMap<String, String>,
    query_params: std::collections::HashMap<String, String>,
    body: Option<JsonValue>, // Legacy body support
    typed_body: Option<RequestBodyType>, // New typed body support
    timeout: super::connection::TimeoutConfig,
    retry_policy: super::connection::RetryPolicy,
    auth: super::connection::AuthorizationType,
    auth_parameters: super::connection::AuthParameters,
    auth_ref: Option<String>,
}
