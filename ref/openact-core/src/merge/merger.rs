//! 参数合并器 - 实现 Connection 和 Task 参数合并

use std::collections::HashMap;
use crate::config::{ConnectionConfig, TaskConfig};
use crate::config::types::{InvocationHttpParameters, MultiValue};
use crate::error::{OpenActError, Result};

/// 合并后的参数
#[derive(Debug, Clone)]
pub struct MergedParameters {
    /// 最终的 Headers（支持多值）
    pub headers: HashMap<String, MultiValue>,
    
    /// 最终的 Query Parameters（支持多值）
    pub query_parameters: HashMap<String, MultiValue>,
    
    /// 最终的 Body Parameters
    pub body_parameters: HashMap<String, String>,
    
    /// API 端点（静态）
    pub api_endpoint: String,
    
    /// HTTP 方法（静态）
    pub method: String,
    
    /// Request Body（如果有）
    pub request_body: Option<serde_json::Value>,
}

/// 参数合并器
pub struct ParameterMerger;

impl ParameterMerger {
    /// 合并 Connection 和 Task 参数
    /// 
    /// 规则：Connection 的 InvocationHttpParameters 优先覆盖 Task 的参数
    pub fn merge(
        connection: &ConnectionConfig,
        task: &TaskConfig,
    ) -> Result<MergedParameters> {
        let mut merged = MergedParameters {
            headers: HashMap::new(),
            query_parameters: HashMap::new(),
            body_parameters: HashMap::new(),
            api_endpoint: task.parameters.api_endpoint.clone(),
            method: task.parameters.method.clone(),
            request_body: task.parameters.request_body.clone(),
        };

        // 1. 先添加 Task 的参数（已经是多值格式）
        merged.headers.extend(task.parameters.headers.clone());
        merged.query_parameters.extend(task.parameters.query_parameters.clone());

        // 2. 再添加 Connection 的参数（会覆盖 Task 的同名参数）
        if let Some(invocation_params) = connection.get_http_parameters() {
            Self::merge_invocation_parameters(&mut merged, invocation_params)?;
        }

        Ok(merged)
    }

    /// 合并 Connection 的 InvocationHttpParameters
    fn merge_invocation_parameters(
        merged: &mut MergedParameters,
        invocation_params: &InvocationHttpParameters,
    ) -> Result<()> {
        // 合并 Header Parameters（支持 multi_value_append_headers 追加）
        let policy = crate::config::types::HttpPolicy::default();
        for param in &invocation_params.header_parameters {
            let key_lower = param.key.to_ascii_lowercase();
            if policy
                .multi_value_append_headers
                .iter()
                .any(|h| h.eq_ignore_ascii_case(&key_lower))
            {
                // 允许追加
                merged
                    .headers
                    .entry(param.key.clone())
                    .and_modify(|mv| mv.add(param.value.clone()))
                    .or_insert_with(|| MultiValue::single(param.value.clone()));
            } else {
                // 覆盖（Connection 优先）
                merged.headers.insert(param.key.clone(), MultiValue::single(param.value.clone()));
            }
        }

        // 合并 Query String Parameters
        for param in &invocation_params.query_string_parameters {
            merged.query_parameters.insert(param.key.clone(), MultiValue::single(param.value.clone()));
        }

        // 合并 Body Parameters（字符串）
        for param in &invocation_params.body_parameters {
            merged.body_parameters.insert(param.key.clone(), param.value.clone());
        }

        Ok(())
    }

    /// 验证合并后的参数
    pub fn validate(merged: &MergedParameters) -> Result<()> {
        // 验证 API 端点
        if merged.api_endpoint.is_empty() {
            return Err(OpenActError::parameter_merge("API endpoint cannot be empty"));
        }

        // 验证 URL 格式
        if !merged.api_endpoint.starts_with("http://") && !merged.api_endpoint.starts_with("https://") {
            return Err(OpenActError::parameter_merge("API endpoint must be a valid HTTP(S) URL"));
        }

        // 验证 HTTP 方法
        match merged.method.to_uppercase().as_str() {
            "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS" => {}
            _ => return Err(OpenActError::parameter_merge("Invalid HTTP method")),
        }

        Ok(())
    }

    // 动态求值已下沉到上层解析层；core 不再处理动态表达式
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::types::*;
    use crate::config::task::TaskParameters;

    fn create_test_connection_with_params() -> ConnectionConfig {
        let invocation_params = InvocationHttpParameters {
            header_parameters: vec![
                crate::config::types::HttpParameter {
                    key: "User-Agent".to_string(),
                    value: "OpenAct/1.0".to_string(),
                },
                crate::config::types::HttpParameter {
                    key: "Accept".to_string(),
                    value: "application/json".to_string(),
                },
            ],
            query_string_parameters: vec![
                crate::config::types::HttpParameter {
                    key: "per_page".to_string(),
                    value: "100".to_string(),
                },
            ],
            body_parameters: vec![],
        };

        let auth_params = AuthParameters {
            api_key_auth_parameters: Some(ApiKeyAuthParameters {
                api_key_name: "X-API-Key".to_string(),
                api_key_value: crate::config::types::Credential::InlineEncrypted("test_key".to_string()),
            }),
            o_auth_parameters: None,
            basic_auth_parameters: None,
            invocation_http_parameters: Some(invocation_params),
        };

        ConnectionConfig::new(
            "trn:openact:tenant1:connection/test@v1".to_string(),
            "Test API".to_string(),
            AuthorizationType::ApiKey,
            auth_params,
        )
    }

    fn create_test_task() -> TaskConfig {
        let mut headers: HashMap<String, MultiValue> = HashMap::new();
        headers.insert("Accept".to_string(), "text/html".into()); // 会被 Connection 覆盖
        headers.insert("X-Custom".to_string(), "task-header".to_string().into());

        let mut query_params: HashMap<String, MultiValue> = HashMap::new();
        query_params.insert("per_page".to_string(), "50".into()); // 会被 Connection 覆盖
        query_params.insert("sort".to_string(), "updated".into());

        let parameters = TaskParameters {
            api_endpoint: "https://api.test.com/data".to_string(),
            method: "GET".to_string(),
            headers,
            query_parameters: query_params,
            request_body: None,
        };

        TaskConfig::new(
            "trn:openact:tenant1:task/get-data@v1".to_string(),
            "Get Data".to_string(),
            "trn:openact:tenant1:connection/test@v1".to_string(),
            parameters,
        )
    }

    #[test]
    fn test_parameter_merge() {
        let connection = create_test_connection_with_params();
        let task = create_test_task();

        let merged = ParameterMerger::merge(&connection, &task).unwrap();

        // 验证 Connection 参数覆盖了 Task 参数
        assert_eq!(merged.headers.get("Accept").and_then(|mv| mv.first()).cloned(), Some("application/json".to_string()));
        assert_eq!(merged.headers.get("User-Agent").and_then(|mv| mv.first()).cloned(), Some("OpenAct/1.0".to_string()));
        assert_eq!(merged.headers.get("X-Custom").and_then(|mv| mv.first()).cloned(), Some("task-header".to_string()));

        assert_eq!(merged.query_parameters.get("per_page").and_then(|mv| mv.first()).cloned(), Some("100".to_string()));
        assert_eq!(merged.query_parameters.get("sort").and_then(|mv| mv.first()).cloned(), Some("updated".to_string()));

        assert_eq!(merged.api_endpoint, "https://api.test.com/data");
        assert_eq!(merged.method, "GET");
    }

    #[test]
    fn test_cookie_multi_value_append() {
        let _connection = create_test_connection_with_params();
        let mut task = create_test_task();

        // 在 Task 中设置 cookie
        task.parameters
            .headers
            .insert("Cookie".to_string(), "a=1".to_string().into());

        // 在 Connection 中追加 cookie（通过 InvocationHttpParameters）
        let inv = InvocationHttpParameters {
            header_parameters: vec![crate::config::types::HttpParameter {
                key: "Cookie".to_string(),
                value: "b=2".to_string(),
            }],
            query_string_parameters: vec![],
            body_parameters: vec![],
        };

        // 构造新的 connection，注入 inv 以测试追加
        let auth_params = AuthParameters {
            api_key_auth_parameters: Some(ApiKeyAuthParameters {
                api_key_name: "X-API-Key".to_string(),
                api_key_value: crate::config::types::Credential::InlineEncrypted("test_key".to_string()),
            }),
            o_auth_parameters: None,
            basic_auth_parameters: None,
            invocation_http_parameters: Some(inv.clone()),
        };
        let connection_with_cookie = ConnectionConfig::new(
            "trn:openact:tenant1:connection/test@v1".to_string(),
            "Test API".to_string(),
            AuthorizationType::ApiKey,
            auth_params,
        );

        let merged = ParameterMerger::merge(&connection_with_cookie, &task).unwrap();

        // 断言 Cookie 被追加为多值：应包含 a=1 和 b=2
        let mv = merged.headers.get("Cookie").expect("cookie header missing");
        let values: Vec<String> = mv.values.clone();
        assert!(values.contains(&"a=1".to_string()));
        assert!(values.contains(&"b=2".to_string()));
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn test_validation() {
        let connection = create_test_connection_with_params();
        let task = create_test_task();

        let merged = ParameterMerger::merge(&connection, &task).unwrap();
        assert!(ParameterMerger::validate(&merged).is_ok());

        // 测试无效的 API 端点
        let mut invalid_merged = merged;
        invalid_merged.api_endpoint = "invalid-url".to_string();
        assert!(ParameterMerger::validate(&invalid_merged).is_err());
    }
}
