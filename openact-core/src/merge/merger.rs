//! 参数合并器 - 实现 Connection 和 Task 参数合并

use std::collections::HashMap;
use crate::config::{ConnectionConfig, TaskConfig};
use crate::config::types::{InvocationHttpParameters, MultiValue, Mapping};
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
    
    /// API 端点（支持 JSONata 表达式）
    pub api_endpoint: crate::config::types::Mapping,
    
    /// HTTP 方法（支持 JSONata 表达式）
    pub method: crate::config::types::Mapping,
    
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

        // 合并 Body Parameters（将 Mapping 转换为字符串）
        for param in &invocation_params.body_parameters {
            let value_str = match &param.value {
                Mapping::Static(json_val) => {
                    if let Some(s) = json_val.as_str() {
                        s.to_string()
                    } else {
                        json_val.to_string()
                    }
                }
                Mapping::Dynamic { expr } => {
                    // 动态值暂时转为表达式字符串，后续需要实际求值
                    format!("{{{{ {} }}}}", expr.as_str())
                }
            };
            merged.body_parameters.insert(param.key.clone(), value_str);
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

    /// 应用动态值替换 (JSONata 表达式处理)
    /// 
    /// TODO: 实现 JSONata 表达式处理
    pub fn apply_dynamic_values(
        merged: &mut MergedParameters,
        input: &serde_json::Value,
    ) -> Result<()> {
        // 处理 Headers 中的动态值
        for (_key, multi_value) in merged.headers.iter_mut() {
            for mapping in multi_value.values.iter_mut() {
                if let Some(processed) = Self::process_dynamic_mapping(mapping, input)? {
                    *mapping = processed;
                }
            }
        }

        // 处理 Query Parameters 中的动态值
        for (_key, multi_value) in merged.query_parameters.iter_mut() {
            for mapping in multi_value.values.iter_mut() {
                if let Some(processed) = Self::process_dynamic_mapping(mapping, input)? {
                    *mapping = processed;
                }
            }
        }

        // 处理 Body Parameters 中的动态值
        for (_key, value) in merged.body_parameters.iter_mut() {
            if let Some(processed) = Self::process_dynamic_value(value, input)? {
                *value = processed;
            }
        }

        // 处理 API 端点中的动态值（现为 Mapping 类型，由执行器阶段统一求值）

        // 处理 Request Body 中的动态值（JSON 结构）
        if let Some(ref mut body) = merged.request_body {
            Self::process_json_dynamic_values(body, input)?;
        }

        Ok(())
    }

    /// 处理单个动态映射
    fn process_dynamic_mapping(
        mapping: &Mapping,
        input: &serde_json::Value,
    ) -> Result<Option<Mapping>> {
        // 统一用执行器中的 JSONata 求值风格：
        // 1) Dynamic 表达式：直接用 jsonata 表达式求值
        // 2) Static 字符串以 "$" 开头：按表达式求值；否则返回 None
        match mapping {
            Mapping::Dynamic { expr } => {
                let evaluated = Self::eval_jsonata(expr.as_str(), input)?;
                Ok(Some(Mapping::static_value(evaluated)))
            }
            Mapping::Static(v) => {
                if let Some(s) = v.as_str() {
                    if s.trim_start().starts_with("$") {
                        let evaluated = Self::eval_jsonata(s, input)?;
                        return Ok(Some(Mapping::static_value(evaluated)));
                    }
                }
                Ok(None)
            }
        }
    }

    /// 处理单个动态值
    fn process_dynamic_value(
        value: &str,
        input: &serde_json::Value,
    ) -> Result<Option<String>> {
        if value.trim_start().starts_with("$") {
            let evaluated = Self::eval_jsonata(value, input)?;
            return Ok(Some(evaluated.to_string()));
        }
        Ok(None)
    }

    /// 处理 JSON 结构中的动态值
    fn process_json_dynamic_values(
        json: &mut serde_json::Value,
        input: &serde_json::Value,
    ) -> Result<()> {
        match json {
            serde_json::Value::Object(map) => {
                for (key, value) in map.iter_mut() {
                    // 处理以 ".$" 结尾的键名（JSONata 表达式）
                    if key.ends_with(".$") {
                        if let serde_json::Value::String(expr) = value {
                            let evaluated = Self::eval_jsonata(expr, input)?;
                            *value = evaluated;
                        }
                    } else {
                        // 递归处理嵌套对象
                        Self::process_json_dynamic_values(value, input)?;
                    }
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr.iter_mut() {
                    Self::process_json_dynamic_values(item, input)?;
                }
            }
            _ => {}
        }
        
        Ok(())
    }

    /// 使用 jsonata-rs 进行表达式求值，输入为 serde_json::Value，输出 serde_json::Value
    fn eval_jsonata(expr: &str, input: &serde_json::Value) -> Result<serde_json::Value> {
        let arena = bumpalo::Bump::new();
        // 将输入序列化为字符串以对接 jsonata-rs API
        let input_str = serde_json::to_string(input)
            .map_err(|e| OpenActError::jsonata_expr(format!("input serialize error: {}", e)))?;
        let ja = jsonata_rs::JsonAta::new(expr, &arena)
            .map_err(|e| OpenActError::jsonata_expr(format!("parse error: {}", e)))?;
        let out = ja
            .evaluate(Some(&input_str), None)
            .map_err(|e| OpenActError::jsonata_expr(format!("eval error: {}", e)))?;
        // jsonata_rs::Value -> json string -> serde_json::Value
        let json_str = out.serialize(false);
        let v = serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
        Ok(v)
    }

    /// 简单的 JSON 路径值获取（临时实现）
    fn get_json_path_value<'a>(json: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = json;
        
        for part in parts {
            match current {
                serde_json::Value::Object(map) => {
                    current = map.get(part)?;
                }
                _ => return None,
            }
        }
        
        Some(current)
    }
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
                    value: "OpenAct/1.0".to_string().into(),
                },
                crate::config::types::HttpParameter {
                    key: "Accept".to_string(),
                    value: "application/json".to_string().into(),
                },
            ],
            query_string_parameters: vec![
                crate::config::types::HttpParameter {
                    key: "per_page".to_string(),
                    value: "100".to_string().into(),
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
            api_endpoint: "https://api.test.com/data".to_string().into(),
            method: "GET".to_string().into(),
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
        assert_eq!(merged.headers.get("Accept").and_then(|mv| mv.first()).and_then(|m| m.as_static()).and_then(|v| v.as_str()), Some("application/json"));
        assert_eq!(merged.headers.get("User-Agent").and_then(|mv| mv.first()).and_then(|m| m.as_static()).and_then(|v| v.as_str()), Some("OpenAct/1.0"));
        assert_eq!(merged.headers.get("X-Custom").and_then(|mv| mv.first()).and_then(|m| m.as_static()).and_then(|v| v.as_str()), Some("task-header"));

        assert_eq!(merged.query_parameters.get("per_page").and_then(|mv| mv.first()).and_then(|m| m.as_static()).and_then(|v| v.as_str()), Some("100"));
        assert_eq!(merged.query_parameters.get("sort").and_then(|mv| mv.first()).and_then(|m| m.as_static()).and_then(|v| v.as_str()), Some("updated"));

        assert_eq!(merged.api_endpoint.as_string().unwrap_or_default(), "https://api.test.com/data");
        assert_eq!(merged.method.as_string().unwrap_or_default(), "GET");
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
                value: "b=2".to_string().into(),
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
        let values: Vec<String> = mv
            .values
            .iter()
            .map(|m| match m {
                crate::config::types::Mapping::Static(v) => v.as_str().unwrap_or(&v.to_string()).to_string(),
                crate::config::types::Mapping::Dynamic { expr } => expr.as_str().to_string(),
            })
            .collect();
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
        invalid_merged.api_endpoint = "invalid-url".to_string().into();
        assert!(ParameterMerger::validate(&invalid_merged).is_err());
    }

    #[test]
    fn test_dynamic_values() {
        let connection = create_test_connection_with_params();
        let mut task = create_test_task();
        
        // 添加动态值
        task.parameters.headers.insert("X-User-ID".to_string(), "$.user.id".to_string().into());
        
        let mut merged = ParameterMerger::merge(&connection, &task).unwrap();
        
        let input = serde_json::json!({
            "user": {
                "id": "12345",
                "name": "test_user"
            }
        });

        ParameterMerger::apply_dynamic_values(&mut merged, &input).unwrap();
        
        // 验证动态值被正确替换
        assert_eq!(merged.headers.get("X-User-ID").and_then(|mv| mv.first()).and_then(|m| m.as_static()).map(|v| v.to_string()), Some("\"12345\"".to_string()));
    }
}
