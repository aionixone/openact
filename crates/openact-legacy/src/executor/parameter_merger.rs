//! Parameter Merger
//!
//! Implements the "ConnectionWins" merge strategy: Connection parameters override Task parameters with the same key

use crate::models::{ConnectionConfig, TaskConfig};
use anyhow::{Result, anyhow};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use std::collections::HashMap;

/// Merged parameters
#[derive(Debug, Clone)]
pub struct MergedParameters {
    pub headers: HeaderMap,
    pub query_params: HashMap<String, String>,
    pub body: Option<Value>,
    pub endpoint: String,
    pub method: String,
}

/// Parameter Merger
pub struct ParameterMerger;

impl ParameterMerger {
    /// Merges Connection and Task parameters, with Connection parameters taking precedence
    pub fn merge(connection: &ConnectionConfig, task: &TaskConfig) -> Result<MergedParameters> {
        let mut merged = MergedParameters {
            headers: HeaderMap::new(),
            query_params: HashMap::new(),
            body: None,
            endpoint: task.api_endpoint.clone(),
            method: task.method.clone(),
        };

        // 1. Add Task parameters as the base
        Self::merge_task_headers(&mut merged.headers, task)?;
        Self::merge_task_query_params(&mut merged.query_params, task)?;
        merged.body = task.request_body.clone();

        // 2. Add Connection default parameters (override same keys)
        Self::merge_connection_headers(&mut merged.headers, connection)?;
        Self::merge_connection_query_params(&mut merged.query_params, connection)?;
        Self::merge_connection_body(&mut merged.body, connection)?;

        // 3. Apply HttpPolicy (deny headers/reserve headers/append multi-values)
        Self::apply_http_policy(&mut merged.headers, connection, task)?;

        Ok(merged)
    }

    /// Merge Task headers
    fn merge_task_headers(headers: &mut HeaderMap, task: &TaskConfig) -> Result<()> {
        if let Some(task_headers) = &task.headers {
            for (key, multi_value) in task_headers {
                let header_name = HeaderName::from_bytes(key.as_bytes())
                    .map_err(|e| anyhow!("Invalid header name '{}': {}", key, e))?;

                // For MultiValue (Vec<String>), merge all values
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

    /// Merge Task query parameters
    fn merge_task_query_params(
        query_params: &mut HashMap<String, String>,
        task: &TaskConfig,
    ) -> Result<()> {
        if let Some(task_query) = &task.query_params {
            for (key, multi_value) in task_query {
                let value = if multi_value.len() == 1 {
                    multi_value[0].clone()
                } else {
                    multi_value.join(",") // Join multi-values with a comma
                };
                query_params.insert(key.clone(), value);
            }
        }
        Ok(())
    }

    /// Merge Connection headers (from invocation_http_parameters)
    fn merge_connection_headers(
        headers: &mut HeaderMap,
        connection: &ConnectionConfig,
    ) -> Result<()> {
        if let Some(invocation_params) = &connection.invocation_http_parameters {
            for header_param in &invocation_params.header_parameters {
                let header_name = HeaderName::from_bytes(header_param.key.as_bytes())
                    .map_err(|e| anyhow!("Invalid header name '{}': {}", header_param.key, e))?;
                let header_value = HeaderValue::from_str(&header_param.value)
                    .map_err(|e| anyhow!("Invalid header value '{}': {}", header_param.value, e))?;

                // ConnectionWins: Override existing header
                headers.insert(header_name, header_value);
            }
        }
        Ok(())
    }

    /// Merge Connection query parameters
    fn merge_connection_query_params(
        query_params: &mut HashMap<String, String>,
        connection: &ConnectionConfig,
    ) -> Result<()> {
        if let Some(invocation_params) = &connection.invocation_http_parameters {
            for query_param in &invocation_params.query_string_parameters {
                // ConnectionWins: Override existing query parameters
                query_params.insert(query_param.key.clone(), query_param.value.clone());
            }
        }
        Ok(())
    }

    /// Merge Connection body parameters
    fn merge_connection_body(
        body: &mut Option<Value>,
        connection: &ConnectionConfig,
    ) -> Result<()> {
        if let Some(invocation_params) = &connection.invocation_http_parameters {
            if !invocation_params.body_parameters.is_empty() {
                // Convert body_parameters to a JSON object
                let mut body_obj = serde_json::Map::new();
                for body_param in &invocation_params.body_parameters {
                    body_obj.insert(
                        body_param.key.clone(),
                        Value::String(body_param.value.clone()),
                    );
                }

                match body {
                    Some(existing_body) => {
                        // If Task already has a body, merge (ConnectionWins)
                        if let Some(existing_obj) = existing_body.as_object_mut() {
                            for (key, value) in body_obj {
                                existing_obj.insert(key, value); // Override existing key
                            }
                        } else {
                            // Task's body is not an object, directly replace
                            *body = Some(Value::Object(body_obj));
                        }
                    }
                    None => {
                        // Task has no body, directly use Connection's body
                        *body = Some(Value::Object(body_obj));
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_http_policy(
        headers: &mut HeaderMap,
        connection: &ConnectionConfig,
        task: &TaskConfig,
    ) -> Result<()> {
        // Select policy: task takes precedence over connection; default if neither
        let policy = task
            .http_policy
            .as_ref()
            .or(connection.http_policy.as_ref())
            .cloned()
            .unwrap_or_default();

        // 0) Validate total header count limit
        if headers.len() > policy.max_total_headers {
            return Err(anyhow!(
                "Too many headers: {} exceeds limit of {}",
                headers.len(),
                policy.max_total_headers
            ));
        }

        // 1) Remove denied headers
        let denied_headers_lower: Vec<String> = policy
            .denied_headers
            .iter()
            .map(|h| h.to_lowercase())
            .collect();
        let mut headers_to_remove = Vec::new();
        for name in headers.keys() {
            let name_str = name.as_str().to_lowercase();
            if denied_headers_lower.contains(&name_str) {
                headers_to_remove.push(name.clone());
            }
        }
        for name in headers_to_remove {
            headers.remove(name);
        }

        // 2) Validate header value length and content
        let mut invalid_headers = Vec::new();
        for (name, value) in headers.iter() {
            let value_str = value.to_str().unwrap_or("");

            // Check header value length
            if value_str.len() > policy.max_header_value_length {
                if policy.drop_forbidden_headers {
                    invalid_headers.push(name.clone());
                    continue;
                } else {
                    return Err(anyhow!(
                        "Header '{}' value too long: {} exceeds limit of {}",
                        name.as_str(),
                        value_str.len(),
                        policy.max_header_value_length
                    ));
                }
            }

            // Check if Content-Type is allowed
            if name.as_str().to_lowercase() == "content-type"
                && !policy.allowed_content_types.is_empty()
            {
                let content_type = value_str
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_lowercase();
                if !policy
                    .allowed_content_types
                    .iter()
                    .any(|ct| ct.to_lowercase() == content_type)
                {
                    if policy.drop_forbidden_headers {
                        invalid_headers.push(name.clone());
                        continue;
                    } else {
                        return Err(anyhow!("Content-Type '{}' not allowed", content_type));
                    }
                }
            }

            // Check for malicious header values
            if Self::is_malicious_header_value(value_str) {
                if policy.drop_forbidden_headers {
                    invalid_headers.push(name.clone());
                    continue;
                } else {
                    return Err(anyhow!(
                        "Malicious header value detected in '{}'",
                        name.as_str()
                    ));
                }
            }
        }

        // Remove invalid headers
        for name in invalid_headers {
            headers.remove(name);
        }

        // 3) Normalize header names
        if policy.normalize_header_names {
            Self::normalize_header_names(headers)?;
        }

        // 4) Reserved headers: if task explicitly provides reserved headers, use task's value, overriding ConnectionWins result
        if let Some(task_headers) = &task.headers {
            for rkey in policy.reserved_headers.iter() {
                let rkey_lc = rkey.to_lowercase();
                if let Some(task_vals) = task_headers
                    .get(&rkey_lc)
                    .or_else(|| task_headers.get(rkey))
                {
                    if let Ok(name) = HeaderName::from_bytes(rkey_lc.as_bytes()) {
                        let combined = if task_vals.len() == 1 {
                            task_vals[0].clone()
                        } else {
                            task_vals.join(", ")
                        };
                        if let Ok(val) = HeaderValue::from_str(&combined) {
                            headers.insert(name, val);
                        }
                    }
                }
            }
        }

        // 5) Multi-value append headers: if a header has multiple values, merge them into a comma-separated list
        for key in policy.multi_value_append_headers.iter() {
            if let Ok(name) = HeaderName::from_bytes(key.as_bytes()) {
                if let Some(val) = headers.get(&name) {
                    let s = val.to_str().unwrap_or("");
                    // Normalize: comma-separated, deduplicate, and sort
                    let mut values: Vec<&str> = s
                        .split(',')
                        .map(|v| v.trim())
                        .filter(|v| !v.is_empty())
                        .collect();
                    values.sort();
                    values.dedup();
                    let joined = values.join(", ");
                    if let Ok(new_val) = HeaderValue::from_str(&joined) {
                        headers.insert(name.clone(), new_val);
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if a header value contains malicious content
    fn is_malicious_header_value(value: &str) -> bool {
        // Check for CRLF injection
        if value.contains('\r') || value.contains('\n') {
            return true;
        }

        // Check for control characters
        if value.chars().any(|c| c.is_control() && c != '\t') {
            return true;
        }

        // Check for suspicious script tags
        let value_lower = value.to_lowercase();
        if value_lower.contains("<script")
            || value_lower.contains("javascript:")
            || value_lower.contains("data:")
        {
            return true;
        }

        false
    }

    /// Normalize header names (convert to lowercase)
    fn normalize_header_names(headers: &mut HeaderMap) -> Result<()> {
        let mut headers_to_update = Vec::new();

        // Collect headers that need normalization
        for (name, value) in headers.iter() {
            let name_str = name.as_str();
            let normalized = name_str.to_lowercase();
            if name_str != normalized {
                headers_to_update.push((name.clone(), value.clone(), normalized));
            }
        }

        // Update header names
        for (old_name, value, normalized) in headers_to_update {
            headers.remove(&old_name);
            if let Ok(new_name) = HeaderName::from_bytes(normalized.as_bytes()) {
                headers.insert(new_name, value);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AuthorizationType, HttpParameter, HttpPolicy, InvocationHttpParameters};

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
                    key: "Content-Type".to_string(), // This will override Task's
                    value: "application/json; charset=utf-8".to_string(),
                },
            ],
            query_string_parameters: vec![
                HttpParameter {
                    key: "limit".to_string(),
                    value: "100".to_string(), // This will override Task's
                },
                HttpParameter {
                    key: "format".to_string(),
                    value: "json".to_string(),
                },
            ],
            body_parameters: vec![HttpParameter {
                key: "source".to_string(),
                value: "connection".to_string(),
            }],
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
        headers.insert(
            "Content-Type".to_string(),
            vec!["application/json".to_string()],
        );
        headers.insert(
            "Accept".to_string(),
            vec!["application/json".to_string(), "text/plain".to_string()],
        );
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
    fn test_http_policy_security_validation() {
        let connection = create_test_connection();
        let mut task = create_test_task();

        // Test malicious header value detection
        task.headers = Some(std::collections::HashMap::from([(
            "test-header".to_string(),
            vec!["value\r\ninjected".to_string()],
        )]));

        let result = ParameterMerger::merge(&connection, &task);

        // Should either drop the header or return error based on policy
        if let Ok(merged) = result {
            assert!(!merged.headers.contains_key("test-header"));
        } else {
            // Error is also acceptable if drop_forbidden_headers is false
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_http_policy_header_limits() {
        let connection = create_test_connection();
        let mut task = create_test_task();

        // Test header value length limit
        let long_value = "x".repeat(10000); // Exceeds default 8KB limit
        task.headers = Some(std::collections::HashMap::from([(
            "test-header".to_string(),
            vec![long_value],
        )]));

        let result = ParameterMerger::merge(&connection, &task);

        if let Ok(merged) = result {
            assert!(!merged.headers.contains_key("test-header"));
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_http_policy_content_type_validation() {
        let mut connection = create_test_connection();
        let mut task = create_test_task();

        // Remove connection headers to avoid interference
        if let Some(ref mut params) = connection.invocation_http_parameters {
            params.header_parameters = vec![];
        }

        // Create a custom policy with strict content type validation
        let mut policy = HttpPolicy::default();
        policy.allowed_content_types = vec!["application/json".to_string()];
        task.http_policy = Some(policy);

        // Test forbidden content type
        task.headers = Some(std::collections::HashMap::from([(
            "content-type".to_string(),
            vec!["application/evil".to_string()],
        )]));

        let result = ParameterMerger::merge(&connection, &task);

        assert!(result.is_ok());
        let merged = result.unwrap();

        // Content-type should be removed because it's not in allowed list
        assert!(!merged.headers.contains_key("content-type"));
    }

    #[test]
    fn test_http_policy_header_normalization() {
        let connection = create_test_connection();
        let mut task = create_test_task();

        // Test header name normalization
        task.headers = Some(std::collections::HashMap::from([
            (
                "Content-Type".to_string(),
                vec!["application/json".to_string()],
            ),
            ("ACCEPT".to_string(), vec!["application/json".to_string()]),
        ]));

        let result = ParameterMerger::merge(&connection, &task);

        assert!(result.is_ok());
        let merged = result.unwrap();

        // Headers should be normalized to lowercase (if policy is enabled)
        if task
            .http_policy
            .as_ref()
            .map_or(true, |p| p.normalize_header_names)
        {
            assert!(merged.headers.contains_key("content-type"));
            assert!(merged.headers.contains_key("accept"));
        }

        // Verify that headers exist with some case
        let has_content_type = merged.headers.contains_key("content-type")
            || merged.headers.contains_key("Content-Type");
        let has_accept =
            merged.headers.contains_key("accept") || merged.headers.contains_key("ACCEPT");
        assert!(has_content_type);
        assert!(has_accept);
    }

    #[test]
    fn test_connection_wins_merge() {
        let connection = create_test_connection();
        let task = create_test_task();

        let merged = ParameterMerger::merge(&connection, &task).unwrap();

        // Verify headers: Connection overrides Task
        assert_eq!(
            merged
                .headers
                .get("Content-Type")
                .unwrap()
                .to_str()
                .unwrap(),
            "application/json; charset=utf-8" // Connection's value
        );
        // Multi-value append normalization: task provided two values → comma joined
        let accept = merged.headers.get("Accept").unwrap().to_str().unwrap();
        assert!(accept.contains("application/json"));
        assert!(accept.contains("text/plain"));
        assert_eq!(
            merged
                .headers
                .get("X-API-Version")
                .unwrap()
                .to_str()
                .unwrap(),
            "v2" // Connection's value
        );
        // Denied headers removed
        assert!(merged.headers.get("host").is_none());

        // Verify query parameters: Connection overrides Task
        assert_eq!(merged.query_params.get("limit").unwrap(), "100"); // Connection's value
        assert_eq!(merged.query_params.get("offset").unwrap(), "0"); // Task's value (no conflict)
        assert_eq!(merged.query_params.get("format").unwrap(), "json"); // Connection's value

        // Verify body: Connection parameters merged into Task's body
        let body = merged.body.unwrap();
        assert_eq!(body["existing"], "value"); // Task's value
        assert_eq!(body["source"], "connection"); // Connection's value
    }
}
