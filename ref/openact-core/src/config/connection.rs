//! Connection 配置管理

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

use crate::error::{OpenActError, Result};
use super::types::*;

/// AWS EventBridge 兼容的 Connection 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    /// TRN 标识符
    pub trn: String,
    
    /// Connection 名称
    pub name: String,
    
    /// 认证类型
    #[serde(rename = "AuthorizationType")]
    pub authorization_type: AuthorizationType,
    
    /// 认证参数
    #[serde(rename = "AuthParameters")]
    pub auth_parameters: AuthParameters,
    
    /// 超时配置（可选）
    #[serde(default)]
    pub timeouts: Option<crate::config::types::TimeoutConfig>,
    
    /// 网络配置（可选）
    #[serde(default)]
    pub network: Option<crate::config::types::NetworkConfig>,
    
    /// 创建时间
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
    
    /// 更新时间
    #[serde(default = "Utc::now")]
    pub updated_at: DateTime<Utc>,
}

impl ConnectionConfig {
    /// 创建新的 Connection 配置
    pub fn new(
        trn: String,
        name: String,
        authorization_type: AuthorizationType,
        auth_parameters: AuthParameters,
    ) -> Self {
        let now = Utc::now();
        Self {
            trn,
            name,
            authorization_type,
            auth_parameters,
            timeouts: None,
            network: None,
            created_at: now,
            updated_at: now,
        }
    }

    /// 验证配置有效性
    pub fn validate(&self) -> Result<()> {
        // 验证 TRN 格式
        if !self.trn.starts_with("trn:openact:") {
            return Err(OpenActError::connection_config("Invalid TRN format"));
        }

        // 验证认证参数
        match &self.authorization_type {
            AuthorizationType::ApiKey => {
                if self.auth_parameters.api_key_auth_parameters.is_none() {
                    return Err(OpenActError::connection_config(
                        "API Key auth parameters required for API_KEY authorization type"
                    ));
                }
            }
            AuthorizationType::OAuth => {
                if self.auth_parameters.o_auth_parameters.is_none() {
                    return Err(OpenActError::connection_config(
                        "OAuth parameters required for OAUTH authorization type"
                    ));
                }
            }
            AuthorizationType::Basic => {
                if self.auth_parameters.basic_auth_parameters.is_none() {
                    return Err(OpenActError::connection_config(
                        "Basic auth parameters required for BASIC authorization type"
                    ));
                }
            }
        }

        Ok(())
    }

    /// 获取 HTTP 参数（Connection 级别）
    pub fn get_http_parameters(&self) -> Option<&InvocationHttpParameters> {
        self.auth_parameters.invocation_http_parameters.as_ref()
    }

    /// 获取认证信息，用于生成 openact TRN
    pub fn get_auth_info(&self) -> AuthInfo {
        match &self.authorization_type {
            AuthorizationType::ApiKey => {
                if let Some(params) = &self.auth_parameters.api_key_auth_parameters {
                    AuthInfo::ApiKey {
                        name: params.api_key_name.clone(),
                        value: params.api_key_value.clone(),
                    }
                } else {
                    AuthInfo::None
                }
            }
            AuthorizationType::OAuth => {
                if let Some(params) = &self.auth_parameters.o_auth_parameters {
                    AuthInfo::OAuth {
                        client_id: params.client_id.clone(),
                        client_secret: params.client_secret.clone(),
                        token_url: params.token_url.clone(),
                        scope: params.scope.clone(),
                        use_pkce: params.use_p_k_c_e,
                    }
                } else {
                    AuthInfo::None
                }
            }
            AuthorizationType::Basic => {
                if let Some(params) = &self.auth_parameters.basic_auth_parameters {
                    AuthInfo::Basic {
                        username: params.username.clone(),
                        password: params.password.clone(),
                    }
                } else {
                    AuthInfo::None
                }
            }
        }
    }
}

/// 认证信息摘要
#[derive(Debug, Clone)]
pub enum AuthInfo {
    None,
    ApiKey { name: String, value: Credential },
    OAuth { 
        client_id: Credential, 
        client_secret: Credential, 
        token_url: String, 
        scope: Option<String>,
        use_pkce: bool,
    },
    Basic { username: Credential, password: Credential },
}

/// Connection 配置存储
#[derive(Debug)]
pub struct ConnectionConfigStore {
    connections: HashMap<String, ConnectionConfig>,
}

impl ConnectionConfigStore {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    /// 存储 Connection 配置
    pub async fn store(&mut self, connection: ConnectionConfig) -> Result<()> {
        connection.validate()?;
        self.connections.insert(connection.trn.clone(), connection);
        Ok(())
    }

    /// 获取 Connection 配置
    pub async fn get(&self, trn: &str) -> Result<Option<ConnectionConfig>> {
        Ok(self.connections.get(trn).cloned())
    }

    /// 列出匹配模式的 Connection
    pub async fn list(&self, pattern: &str) -> Result<Vec<ConnectionConfig>> {
        let mut results = Vec::new();
        
        for connection in self.connections.values() {
            if pattern == "*" || connection.trn.contains(pattern) {
                results.push(connection.clone());
            }
        }
        
        Ok(results)
    }

    /// 删除 Connection 配置
    pub async fn delete(&mut self, trn: &str) -> Result<bool> {
        Ok(self.connections.remove(trn).is_some())
    }
}

impl Default for ConnectionConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_config_api_key() {
        let auth_params = AuthParameters {
            api_key_auth_parameters: Some(ApiKeyAuthParameters {
                api_key_name: "X-API-Key".to_string(),
                api_key_value: Credential::InlineEncrypted("test_key".to_string()),
            }),
            o_auth_parameters: None,
            basic_auth_parameters: None,
            invocation_http_parameters: None,
        };

        let connection = ConnectionConfig::new(
            "trn:openact:tenant1:connection/test@v1".to_string(),
            "Test API".to_string(),
            AuthorizationType::ApiKey,
            auth_params,
        );

        assert!(connection.validate().is_ok());
        
        match connection.get_auth_info() {
            AuthInfo::ApiKey { name, value } => {
                assert_eq!(name, "X-API-Key");
                match value {
                    Credential::InlineEncrypted(s) => assert_eq!(s, "test_key"),
                    _ => panic!("Expected InlineEncrypted credential"),
                }
            }
            _ => panic!("Expected API Key auth info"),
        }
    }

    #[tokio::test]
    async fn test_connection_store() {
        let mut store = ConnectionConfigStore::new();
        
        let auth_params = AuthParameters {
            api_key_auth_parameters: Some(ApiKeyAuthParameters {
                api_key_name: "Authorization".to_string(),
                api_key_value: Credential::InlineEncrypted("Bearer token".to_string()),
            }),
            o_auth_parameters: None,
            basic_auth_parameters: None,
            invocation_http_parameters: None,
        };

        let connection = ConnectionConfig::new(
            "trn:openact:tenant1:connection/test@v1".to_string(),
            "Test API".to_string(),
            AuthorizationType::ApiKey,
            auth_params,
        );

        // 存储
        store.store(connection.clone()).await.unwrap();

        // 获取
        let retrieved = store.get(&connection.trn).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test API");

        // 列表
        let list = store.list("*").await.unwrap();
        assert_eq!(list.len(), 1);
    }
}
