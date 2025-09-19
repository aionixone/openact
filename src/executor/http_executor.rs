//! HTTP执行器
//! 
//! 处理直接HTTP调用：API Key、Basic Auth、OAuth2 Client Credentials

use crate::models::{ConnectionConfig, TaskConfig, AuthorizationType};
use crate::store::{ConnectionStore, MemoryConnectionStore};
#[cfg(feature = "workflow")]
use crate::authflow::actions::{OAuth2ClientCredentialsHandler, EnsureFreshTokenHandler};
#[cfg(feature = "workflow")]
use crate::authflow::engine::TaskHandler;
use super::auth_injector::create_auth_injector;
use super::parameter_merger::ParameterMerger;

use anyhow::{Result, anyhow, Context};
use reqwest::{Client, Response};
use reqwest::header::{HeaderValue, AUTHORIZATION};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use serde_json::json;
use tokio::sync::OnceCell;

// 全局HTTP客户端
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

// 全局ConnectionStore实例
static CONNECTION_STORE: OnceCell<Arc<dyn ConnectionStore>> = OnceCell::const_new();

/// HTTP执行器：处理直接HTTP调用
pub struct HttpExecutor {
    client: Client,
}

impl HttpExecutor {
    /// 创建新的HTTP执行器
    pub fn new() -> Self {
        Self {
            client: Self::get_http_client(),
        }
    }

    /// 获取全局ConnectionStore实例
    async fn get_connection_store() -> Arc<dyn ConnectionStore> {
        if let Some(store) = CONNECTION_STORE.get() {
            return store.clone();
        }

        // TODO: 这里应该从环境变量或配置中选择合适的ConnectionStore
        // 临时使用MemoryConnectionStore
        let store: Arc<dyn ConnectionStore> = Arc::new(MemoryConnectionStore::new());
        let _ = CONNECTION_STORE.set(store.clone());
        store
    }

    /// 获取共享的HTTP客户端
    fn get_http_client() -> Client {
        HTTP_CLIENT.get_or_init(|| {
            Client::builder()
                .user_agent("OpenAct/0.1.0")
                .build()
                .expect("Failed to create HTTP client")
        }).clone()
    }

    /// 执行HTTP请求
    pub async fn execute(
        &self,
        connection: &ConnectionConfig,
        task: &TaskConfig,
    ) -> Result<Response> {
        // 1. 合并参数（ConnectionWins策略）
        let mut merged = ParameterMerger::merge(connection, task)
            .context("Failed to merge parameters")?;

        // 2. 注入认证信息
        self.inject_authentication(&mut merged.headers, &mut merged.query_params, connection)
            .await
            .context("Failed to inject authentication")?;

        // 3. 构建完整URL
        let url = self.build_url(&merged.endpoint, &merged.query_params)?;

        // 4. 构建HTTP请求
        let mut request_builder = self.client
            .request(
                merged.method.parse()
                    .map_err(|e| anyhow!("Invalid HTTP method '{}': {}", merged.method, e))?,
                url
            )
            .headers(merged.headers);

        // 5. 添加请求体（如果有）
        if let Some(body) = merged.body {
            request_builder = request_builder.json(&body);
        }

        // 6. 执行请求
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
                self.handle_oauth2_client_credentials(headers, connection).await?;
            }
            AuthorizationType::OAuth2AuthorizationCode => {
                // OAuth2 Authorization Code: 从存储中获取token，必要时刷新，然后注入Bearer header
                self.handle_oauth2_authorization_code(headers, connection).await?;
            }
            _ => {
                // API Key和Basic Auth: 直接注入，无需token刷新
                let injector = create_auth_injector(&connection.authorization_type);
                injector.inject_auth(headers, query_params, connection)
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
        let oauth_params = connection
            .auth_parameters
            .oauth_parameters
            .as_ref()
            .ok_or_else(|| anyhow!("OAuth2 parameters missing for connection: {}", connection.trn))?;

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
        let result = handler.execute("oauth2.client_credentials", "", &request_params)
            .context("Failed to obtain OAuth2 Client Credentials token")?;

        // 提取access_token
        let access_token = result
            .get("access_token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("No access_token in OAuth2 response"))?;

        // 注入Bearer token到Authorization header
        let auth_value = format!("Bearer {}", access_token);
        let header_value = HeaderValue::from_str(&auth_value)
            .map_err(|_| anyhow!("Invalid access token format"))?;
        headers.insert(AUTHORIZATION, header_value);

        // TODO: 可选地缓存token到auth_connections表，用于后续的refresh
        // 这需要expires_in信息和可能的refresh_token

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
            .ok_or_else(|| anyhow!("OAuth2 parameters missing for connection: {}", connection.trn))?;

        // 临时使用MemoryConnectionStore，后续可以改为从配置选择
        // TODO: 集成真正的数据库存储
        let store = MemoryConnectionStore::new();
        
        // 使用现有的EnsureFreshTokenHandler来处理token刷新
        let ensure_handler = EnsureFreshTokenHandler { store };
        let request_params = json!({
            "connection_ref": connection.trn,
            "tokenUrl": oauth_params.token_url,
            "clientId": oauth_params.client_id,
            "clientSecret": oauth_params.client_secret,
            "skewSeconds": 120  // 2分钟的安全边际
        });

        let result = ensure_handler.execute("ensure.fresh_token", "", &request_params)
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
        let url = executor.build_url("https://api.example.com/users", &params).unwrap();
        assert_eq!(url, "https://api.example.com/users");
    }

    #[test]
    fn test_build_url_with_params() {
        let executor = HttpExecutor::new();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "10".to_string());
        params.insert("offset".to_string(), "20".to_string());
        
        let url = executor.build_url("https://api.example.com/users", &params).unwrap();
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
        
        let url = executor.build_url("https://api.example.com/users?existing=value", &params).unwrap();
        assert!(url.contains("existing=value"));
        assert!(url.contains("sort=name"));
        assert!(url.contains("&"));
    }

    // 注意：实际的HTTP请求测试需要mock服务器，这里只测试URL构建逻辑
}
