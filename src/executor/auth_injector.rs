//! 认证注入器
//!
//! 根据不同的认证类型注入相应的认证头和参数

use crate::models::{AuthorizationType, ConnectionConfig};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use std::collections::HashMap;
use thiserror::Error;

/// 认证注入错误
#[derive(Error, Debug)]
pub enum AuthInjectionError {
    #[error("Missing authentication parameters for {auth_type:?}")]
    MissingAuthParams { auth_type: AuthorizationType },

    #[error("Invalid header name")]
    InvalidHeaderName { name: String },

    #[error("Invalid header value (length: {value_len})")]
    InvalidHeaderValue { value_len: usize },

    #[error("OAuth2 token not available for connection: {connection_trn}")]
    OAuth2TokenNotAvailable { connection_trn: String },
}

/// 认证注入器接口
pub trait AuthInjector {
    /// 注入认证信息到headers和query参数中
    fn inject_auth(
        &self,
        headers: &mut HeaderMap,
        query_params: &mut HashMap<String, String>,
        connection: &ConnectionConfig,
    ) -> Result<(), AuthInjectionError>;
}

/// API Key 认证注入器
pub struct ApiKeyInjector;

impl AuthInjector for ApiKeyInjector {
    fn inject_auth(
        &self,
        headers: &mut HeaderMap,
        query_params: &mut HashMap<String, String>,
        connection: &ConnectionConfig,
    ) -> Result<(), AuthInjectionError> {
        let api_key_params = connection
            .auth_parameters
            .api_key_auth_parameters
            .as_ref()
            .ok_or(AuthInjectionError::MissingAuthParams {
                auth_type: connection.authorization_type.clone(),
            })?;

        // 根据API Key名称决定注入位置
        // 常见模式：
        // - "Authorization" -> Header: "Bearer {api_key}" 或 "ApiKey {api_key}"
        // - "X-API-Key" -> Header: "{api_key}"
        // - "api_key" -> Query: "{api_key}"

        let key_name = &api_key_params.api_key_name;
        let key_value = &api_key_params.api_key_value;

        if key_name.eq_ignore_ascii_case("authorization") {
            // Authorization header with Bearer prefix
            let auth_value = format!("Bearer {}", key_value);
            let header_value = HeaderValue::from_str(&auth_value)
                .map_err(|_| AuthInjectionError::InvalidHeaderValue { value_len: auth_value.len() })?;
            headers.insert(AUTHORIZATION, header_value);
        } else if key_name.starts_with("X-") || key_name.contains("-") {
            // Custom header
            let header_name = HeaderName::from_bytes(key_name.as_bytes()).map_err(|_| {
                AuthInjectionError::InvalidHeaderName {
                    name: key_name.clone(),
                }
            })?;
            let header_value = HeaderValue::from_str(key_value).map_err(|_| {
                AuthInjectionError::InvalidHeaderValue {
                    value_len: key_value.len(),
                }
            })?;
            headers.insert(header_name, header_value);
        } else {
            // Query parameter
            query_params.insert(key_name.clone(), key_value.clone());
        }

        Ok(())
    }
}

/// Basic Auth 认证注入器
pub struct BasicAuthInjector;

impl AuthInjector for BasicAuthInjector {
    fn inject_auth(
        &self,
        headers: &mut HeaderMap,
        _query_params: &mut HashMap<String, String>,
        connection: &ConnectionConfig,
    ) -> Result<(), AuthInjectionError> {
        let basic_params = connection
            .auth_parameters
            .basic_auth_parameters
            .as_ref()
            .ok_or(AuthInjectionError::MissingAuthParams {
                auth_type: connection.authorization_type.clone(),
            })?;

        // 创建 Basic Auth header: "Basic base64(username:password)"
        let credentials = format!("{}:{}", basic_params.username, basic_params.password);
        let encoded = STANDARD.encode(credentials.as_bytes());
        let auth_value = format!("Basic {}", encoded);

        let header_value = HeaderValue::from_str(&auth_value)
            .map_err(|_| AuthInjectionError::InvalidHeaderValue { value_len: auth_value.len() })?;

        headers.insert(AUTHORIZATION, header_value);
        Ok(())
    }
}

/// OAuth2 认证注入器
pub struct OAuth2Injector;

impl AuthInjector for OAuth2Injector {
    fn inject_auth(
        &self,
        _headers: &mut HeaderMap,
        _query_params: &mut HashMap<String, String>,
        connection: &ConnectionConfig,
    ) -> Result<(), AuthInjectionError> {
        // Phase 0: interface only. Runtime integration will be added in Phase 3.
        // For now, return a placeholder error to avoid accidental use.
        Err(AuthInjectionError::OAuth2TokenNotAvailable { connection_trn: connection.trn.clone() })
    }
}

/// 创建对应的认证注入器
pub fn create_auth_injector(auth_type: &AuthorizationType) -> Box<dyn AuthInjector> {
    match auth_type {
        AuthorizationType::ApiKey => Box::new(ApiKeyInjector),
        AuthorizationType::Basic => Box::new(BasicAuthInjector),
        AuthorizationType::OAuth2ClientCredentials => Box::new(OAuth2Injector),
        AuthorizationType::OAuth2AuthorizationCode => Box::new(OAuth2Injector),
    }
}
