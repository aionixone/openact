//! 参数合并器
//! 
//! 实现"ConnectionWins"合并策略：Connection参数覆盖Task相同参数

use crate::models::{ConnectionConfig, TaskConfig};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::HashMap;
use serde_json::Value;
use anyhow::{Result, anyhow};

/// 合并后的参数
#[derive(Debug, Clone)]
pub struct MergedParameters {
    pub headers: HeaderMap,
    pub query_params: HashMap<String, String>,
    pub body: Option<Value>,
    pub endpoint: String,
    pub method: String,
}

/// 参数合并器
pub struct ParameterMerger;

impl ParameterMerger {
    /// 合并Connection和Task参数，Connection参数优先
    pub fn merge(
        connection: &ConnectionConfig,
        task: &TaskConfig,
    ) -> Result<MergedParameters> {
        let mut merged = MergedParameters {
            headers: HeaderMap::new(),
            query_params: HashMap::new(),
            body: None,
            endpoint: task.api_endpoint.clone(),
            method: task.method.clone(),
        };

        // 1. 先添加Task的参数作为基础
        Self::merge_task_headers(&mut merged.headers, task)?;
        Self::merge_task_query_params(&mut merged.query_params, task)?;
        merged.body = task.request_body.clone();

        // 2. 再添加Connection的默认参数（覆盖相同key）
        Self::merge_connection_headers(&mut merged.headers, connection)?;
        Self::merge_connection_query_params(&mut merged.query_params, connection)?;
        Self::merge_connection_body(&mut merged.body, connection)?;

        // 3. 应用 HttpPolicy（禁止头/保留头/追加多值）
        Self::apply_http_policy(&mut merged.headers, connection, task)?;

        Ok(merged)
    }

    /// 合并Task的headers
    fn merge_task_headers(headers: &mut HeaderMap, task: &TaskConfig) -> Result<()> {
        if let Some(task_headers) = &task.headers {
            for (key, multi_value) in task_headers {
                let header_name = HeaderName::from_bytes(key.as_bytes())
                    .map_err(|e| anyhow!("Invalid header name '{}': {}", key, e))?;
                
                // 对于MultiValue (Vec<String>)，我们合并所有值
                let header_value = if multi_value.len() == 1 {
                    HeaderValue::from_str(&multi_value[0])
                        .map_err(|e| anyhow!("Invalid header value '{}': {}", multi_value[0], e))?
                } else {
                    let combined = multi_value.join(", ");
                    HeaderValue::from_str(&combined)
                        .map_err(|e| anyhow!("Invalid header value '{}': {}", combined, e))?
                };
                
                headers.insert(header_name, header_value);
            }
        }
        Ok(())
    }

    /// 合并Task的query参数
    fn merge_task_query_params(query_params: &mut HashMap<String, String>, task: &TaskConfig) -> Result<()> {
        if let Some(task_query) = &task.query_params {
            for (key, multi_value) in task_query {
                let value = if multi_value.len() == 1 {
                    multi_value[0].clone()
                } else {
                    multi_value.join(",") // 用逗号分隔多值
                };
                query_params.insert(key.clone(), value);
            }
        }
        Ok(())
    }

    /// 合并Connection的headers（来自invocation_http_parameters）
    fn merge_connection_headers(headers: &mut HeaderMap, connection: &ConnectionConfig) -> Result<()> {
        if let Some(invocation_params) = &connection.invocation_http_parameters {
            for header_param in &invocation_params.header_parameters {
                let header_name = HeaderName::from_bytes(header_param.key.as_bytes())
                    .map_err(|e| anyhow!("Invalid header name '{}': {}", header_param.key, e))?;
                let header_value = HeaderValue::from_str(&header_param.value)
                    .map_err(|e| anyhow!("Invalid header value '{}': {}", header_param.value, e))?;
                
                // ConnectionWins: 覆盖已存在的header
                headers.insert(header_name, header_value);
            }
        }
        Ok(())
    }

    /// 合并Connection的query参数
    fn merge_connection_query_params(query_params: &mut HashMap<String, String>, connection: &ConnectionConfig) -> Result<()> {
        if let Some(invocation_params) = &connection.invocation_http_parameters {
            for query_param in &invocation_params.query_string_parameters {
                // ConnectionWins: 覆盖已存在的query参数
                query_params.insert(query_param.key.clone(), query_param.value.clone());
            }
        }
        Ok(())
    }

    /// 合并Connection的body参数
    fn merge_connection_body(body: &mut Option<Value>, connection: &ConnectionConfig) -> Result<()> {
        if let Some(invocation_params) = &connection.invocation_http_parameters {
            if !invocation_params.body_parameters.is_empty() {
                // 将body_parameters转换为JSON对象
                let mut body_obj = serde_json::Map::new();
                for body_param in &invocation_params.body_parameters {
                    body_obj.insert(body_param.key.clone(), Value::String(body_param.value.clone()));
                }
                
                match body {
                    Some(existing_body) => {
                        // 如果Task已有body，合并（ConnectionWins）
                        if let Some(existing_obj) = existing_body.as_object_mut() {
                            for (key, value) in body_obj {
                                existing_obj.insert(key, value); // 覆盖已存在的key
                            }
                        } else {
                            // Task的body不是对象，直接替换
                            *body = Some(Value::Object(body_obj));
                        }
                    }
                    None => {
                        // Task没有body，直接使用Connection的body
                        *body = Some(Value::Object(body_obj));
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_http_policy(headers: &mut HeaderMap, connection: &ConnectionConfig, task: &TaskConfig) -> Result<()> {
        // 选择策略：task优先于connection；若都无则默认
        let policy = task.http_policy.as_ref().or(connection.http_policy.as_ref()).cloned().unwrap_or_default();

        // 1) 删除禁止头
        for key in policy.denied_headers.iter() {
            if let Ok(name) = HeaderName::from_bytes(key.as_bytes()) {
                headers.remove(name);
            }
        }

        // 2) 保留头名单：若 task 显式提供了保留头，则以 task 的值为准，覆盖 ConnectionWins 结果
        if let Some(task_headers) = &task.headers {
            for rkey in policy.reserved_headers.iter() {
                let rkey_lc = rkey.to_lowercase();
                if let Some(task_vals) = task_headers.get(&rkey_lc).or_else(|| task_headers.get(rkey)) {
                    if let Ok(name) = HeaderName::from_bytes(rkey_lc.as_bytes()) {
                        let combined = if task_vals.len() == 1 { task_vals[0].clone() } else { task_vals.join(", ") };
                        if let Ok(val) = HeaderValue::from_str(&combined) {
                            headers.insert(name, val);
                        }
                    }
                }
            }
        }

        // 3) 多值追加头：如果头存在多个值，合并为逗号分隔
        for key in policy.multi_value_append_headers.iter() {
            if let Ok(name) = HeaderName::from_bytes(key.as_bytes()) {
                if let Some(val) = headers.get(&name) {
                    let s = val.to_str().unwrap_or("");
                    // 标准化：用逗号分隔；去重简单略过
                    let joined = s.split(',').map(|v| v.trim()).collect::<Vec<_>>().join(", ");
                    if let Ok(new_val) = HeaderValue::from_str(&joined) {
                        headers.insert(name.clone(), new_val);
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AuthorizationType, InvocationHttpParameters, HttpParameter, HttpPolicy};

    fn create_test_connection() -> ConnectionConfig {
        let mut connection = ConnectionConfig::new(
            "trn:openact:default:connection/test".to_string(),
            "Test Connection".to_string(),
            AuthorizationType::ApiKey,
        );
        
        connection.invocation_http_parameters = Some(InvocationHttpParameters {
            header_parameters: vec![
                HttpParameter {
                    key: "X-API-Version".to_string(),
                    value: "v2".to_string(),
                },
                HttpParameter {
                    key: "Content-Type".to_string(), // 这个会覆盖Task的
                    value: "application/json; charset=utf-8".to_string(),
                },
            ],
            query_string_parameters: vec![
                HttpParameter {
                    key: "limit".to_string(),
                    value: "100".to_string(), // 这个会覆盖Task的
                },
                HttpParameter {
                    key: "format".to_string(),
                    value: "json".to_string(),
                },
            ],
            body_parameters: vec![
                HttpParameter {
                    key: "source".to_string(),
                    value: "connection".to_string(),
                },
            ],
        });
        
        connection
    }

    fn create_test_task() -> TaskConfig {
        let mut task = TaskConfig::new(
            "trn:openact:default:task/test".to_string(),
            "Test Task".to_string(),
            "trn:openact:default:connection/test".to_string(),
            "/api/users".to_string(),
            "GET".to_string(),
        );
        
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), vec!["application/json".to_string()]);
        headers.insert("Accept".to_string(), vec!["application/json".to_string(), "text/plain".to_string()]);
        headers.insert("host".to_string(), vec!["example.com".to_string()]);
        task.headers = Some(headers);
        
        let mut query_params = HashMap::new();
        query_params.insert("limit".to_string(), vec!["50".to_string()]);
        query_params.insert("offset".to_string(), vec!["0".to_string()]);
        task.query_params = Some(query_params);
        
        task.request_body = Some(serde_json::json!({
            "existing": "value"
        }));
        
        // Attach default policy (denies host; multi-append includes accept)
        task.http_policy = Some(HttpPolicy::default());
        task
    }

    #[test]
    fn test_connection_wins_merge() {
        let connection = create_test_connection();
        let task = create_test_task();
        
        let merged = ParameterMerger::merge(&connection, &task).unwrap();
        
        // 验证headers：Connection覆盖Task
        assert_eq!(
            merged.headers.get("Content-Type").unwrap().to_str().unwrap(),
            "application/json; charset=utf-8" // Connection的值
        );
        // Multi-value append normalization: task provided two values → comma joined
        let accept = merged.headers.get("Accept").unwrap().to_str().unwrap();
        assert!(accept.contains("application/json"));
        assert!(accept.contains("text/plain"));
        assert_eq!(
            merged.headers.get("X-API-Version").unwrap().to_str().unwrap(),
            "v2" // Connection的值
        );
        // Denied headers removed
        assert!(merged.headers.get("host").is_none());
        
        // 验证query参数：Connection覆盖Task
        assert_eq!(merged.query_params.get("limit").unwrap(), "100"); // Connection的值
        assert_eq!(merged.query_params.get("offset").unwrap(), "0"); // Task的值（没有冲突）
        assert_eq!(merged.query_params.get("format").unwrap(), "json"); // Connection的值
        
        // 验证body：Connection参数合并到Task的body中
        let body = merged.body.unwrap();
        assert_eq!(body["existing"], "value"); // Task的值
        assert_eq!(body["source"], "connection"); // Connection的值
    }
}
