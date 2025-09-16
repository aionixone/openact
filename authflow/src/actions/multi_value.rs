use anyhow::Result;
use serde_json::{Value, Map};
use std::collections::HashMap;

/// Multi-value header/query parameter support
/// Handles merging and appending logic for OpenAct v2 compatibility

/// Merge strategy for handling conflicts
#[derive(Debug, Clone)]
pub enum MergeStrategy {
    /// Override: later values replace earlier ones
    Override,
    /// Append: later values are appended to earlier ones
    Append,
    /// Prepend: later values are prepended to earlier ones
    Prepend,
}

/// HTTP policy configuration for header/query handling
#[derive(Debug, Clone)]
pub struct HttpPolicy {
    /// Headers that should be denied (e.g., "host", "content-length")
    pub denied_headers: Vec<String>,
    /// Headers reserved for system use (e.g., "authorization")
    pub reserved_headers: Vec<String>,
    /// Headers that should use append strategy instead of override
    pub multi_value_append_headers: Vec<String>,
    /// Whether to silently drop forbidden headers (true) or error (false)
    pub drop_forbidden_headers: bool,
}

impl Default for HttpPolicy {
    fn default() -> Self {
        Self {
            denied_headers: vec![
                "host".to_string(),
                "content-length".to_string(),
                "transfer-encoding".to_string(),
                "expect".to_string(),
            ],
            reserved_headers: vec![
                "authorization".to_string(),
            ],
            multi_value_append_headers: vec![
                "accept".to_string(),
                "cookie".to_string(),
                "set-cookie".to_string(),
                "cache-control".to_string(),
            ],
            drop_forbidden_headers: true,
        }
    }
}

/// Multi-value container for headers/query parameters
#[derive(Debug, Clone)]
pub struct MultiValue {
    pub values: Vec<String>,
}

impl MultiValue {
    pub fn new() -> Self {
        Self { values: Vec::new() }
    }

    pub fn from_value(value: String) -> Self {
        Self { values: vec![value] }
    }

    pub fn from_values(values: Vec<String>) -> Self {
        Self { values }
    }

    pub fn add_value(&mut self, value: String) {
        self.values.push(value);
    }

    pub fn prepend_value(&mut self, value: String) {
        self.values.insert(0, value);
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn first(&self) -> Option<&String> {
        self.values.first()
    }

    pub fn join(&self, separator: &str) -> String {
        self.values.join(separator)
    }

    pub fn join_comma(&self) -> String {
        self.join(", ")
    }
}

impl From<String> for MultiValue {
    fn from(value: String) -> Self {
        Self::from_value(value)
    }
}

impl From<Vec<String>> for MultiValue {
    fn from(values: Vec<String>) -> Self {
        Self::from_values(values)
    }
}

/// Multi-value parameter store
pub type MultiValueMap = HashMap<String, MultiValue>;

/// Utility functions for merging multi-value parameters
pub struct MultiValueMerger;

impl MultiValueMerger {
    /// Merge two multi-value maps according to policy
    pub fn merge_with_policy(
        base: &MultiValueMap,
        overlay: &MultiValueMap,
        policy: &HttpPolicy,
        is_headers: bool,
    ) -> Result<MultiValueMap> {
        let mut result = base.clone();

        for (key, new_values) in overlay {
            let key_lower = key.to_lowercase();

            // Check if header is denied
            if is_headers && policy.denied_headers.iter().any(|h| h.to_lowercase() == key_lower) {
                if !policy.drop_forbidden_headers {
                    return Err(anyhow::anyhow!("Forbidden header: {}", key));
                }
                // Skip this header (drop it)
                continue;
            }

            // Check if header is reserved (for system use only)
            if is_headers && policy.reserved_headers.iter().any(|h| h.to_lowercase() == key_lower) {
                if !policy.drop_forbidden_headers {
                    return Err(anyhow::anyhow!("Reserved header: {}", key));
                }
                // Skip this header (drop it)
                continue;
            }

            // Determine merge strategy
            let strategy = if is_headers && policy.multi_value_append_headers.iter().any(|h| h.to_lowercase() == key_lower) {
                MergeStrategy::Append
            } else {
                MergeStrategy::Override
            };

            // Apply merge strategy
            match strategy {
                MergeStrategy::Override => {
                    result.insert(key.clone(), new_values.clone());
                }
                MergeStrategy::Append => {
                    if let Some(existing) = result.get_mut(key) {
                        for value in &new_values.values {
                            existing.add_value(value.clone());
                        }
                    } else {
                        result.insert(key.clone(), new_values.clone());
                    }
                }
                MergeStrategy::Prepend => {
                    if let Some(existing) = result.get_mut(key) {
                        for value in new_values.values.iter().rev() {
                            existing.prepend_value(value.clone());
                        }
                    } else {
                        result.insert(key.clone(), new_values.clone());
                    }
                }
            }
        }

        Ok(result)
    }

    /// Merge Connection parameters with Task parameters (Connection wins)
    pub fn merge_connection_task(
        connection_params: &MultiValueMap,
        task_params: &MultiValueMap,
        policy: &HttpPolicy,
        is_headers: bool,
    ) -> Result<MultiValueMap> {
        // Task parameters as base, Connection parameters as overlay (Connection wins)
        Self::merge_with_policy(task_params, connection_params, policy, is_headers)
    }

    /// Convert MultiValueMap to flat HashMap for HTTP clients
    pub fn to_flat_map(multi_map: &MultiValueMap) -> HashMap<String, String> {
        multi_map
            .iter()
            .map(|(key, multi_value)| {
                let value = if multi_value.values.len() == 1 {
                    multi_value.values[0].clone()
                } else {
                    multi_value.join_comma()
                };
                (key.clone(), value)
            })
            .collect()
    }

    /// Convert JSON object to MultiValueMap
    pub fn from_json_object(obj: &Map<String, Value>) -> MultiValueMap {
        let mut result = MultiValueMap::new();

        for (key, value) in obj {
            let multi_value = match value {
                Value::String(s) => MultiValue::from_value(s.clone()),
                Value::Array(arr) => {
                    let values: Vec<String> = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    MultiValue::from_values(values)
                }
                _ => MultiValue::from_value(value.to_string()),
            };
            result.insert(key.clone(), multi_value);
        }

        result
    }

    /// Convert MultiValueMap to JSON object
    pub fn to_json_object(multi_map: &MultiValueMap) -> Map<String, Value> {
        let mut result = Map::new();

        for (key, multi_value) in multi_map {
            let json_value = if multi_value.values.len() == 1 {
                Value::String(multi_value.values[0].clone())
            } else {
                Value::Array(
                    multi_value
                        .values
                        .iter()
                        .map(|s| Value::String(s.clone()))
                        .collect(),
                )
            };
            result.insert(key.clone(), json_value);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_value_basic() {
        let mut mv = MultiValue::new();
        assert!(mv.is_empty());

        mv.add_value("value1".to_string());
        mv.add_value("value2".to_string());
        assert_eq!(mv.values.len(), 2);
        assert_eq!(mv.join_comma(), "value1, value2");
    }

    #[test]
    fn test_merge_override_strategy() {
        let mut base = MultiValueMap::new();
        base.insert("accept".to_string(), MultiValue::from_value("application/json".to_string()));

        let mut overlay = MultiValueMap::new();
        overlay.insert("accept".to_string(), MultiValue::from_value("text/html".to_string()));

        let policy = HttpPolicy {
            multi_value_append_headers: vec![], // No append headers
            ..Default::default()
        };

        let result = MultiValueMerger::merge_with_policy(&base, &overlay, &policy, true).unwrap();
        assert_eq!(result.get("accept").unwrap().values, vec!["text/html"]);
    }

    #[test]
    fn test_merge_append_strategy() {
        let mut base = MultiValueMap::new();
        base.insert("accept".to_string(), MultiValue::from_value("application/json".to_string()));

        let mut overlay = MultiValueMap::new();
        overlay.insert("accept".to_string(), MultiValue::from_value("text/html".to_string()));

        let policy = HttpPolicy::default(); // accept is in append list by default

        let result = MultiValueMerger::merge_with_policy(&base, &overlay, &policy, true).unwrap();
        assert_eq!(result.get("accept").unwrap().values, vec!["application/json", "text/html"]);
    }

    #[test]
    fn test_denied_headers() {
        let base = MultiValueMap::new();
        let mut overlay = MultiValueMap::new();
        overlay.insert("host".to_string(), MultiValue::from_value("evil.com".to_string()));

        let policy = HttpPolicy::default(); // host is denied by default

        // With drop_forbidden_headers = true (default)
        let result = MultiValueMerger::merge_with_policy(&base, &overlay, &policy, true).unwrap();
        assert!(!result.contains_key("host"));

        // With drop_forbidden_headers = false
        let strict_policy = HttpPolicy {
            drop_forbidden_headers: false,
            ..Default::default()
        };
        let result = MultiValueMerger::merge_with_policy(&base, &overlay, &strict_policy, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_connection_task_merge() {
        let mut connection_params = MultiValueMap::new();
        connection_params.insert("accept".to_string(), MultiValue::from_value("application/json".to_string()));
        connection_params.insert("user-agent".to_string(), MultiValue::from_value("OpenAct/1.0".to_string()));

        let mut task_params = MultiValueMap::new();
        task_params.insert("accept".to_string(), MultiValue::from_value("text/html".to_string()));
        task_params.insert("x-custom".to_string(), MultiValue::from_value("task-header".to_string()));

        let policy = HttpPolicy::default();
        let result = MultiValueMerger::merge_connection_task(&connection_params, &task_params, &policy, true).unwrap();

        // Connection should win for 'accept' (append strategy)
        assert_eq!(result.get("accept").unwrap().values, vec!["text/html", "application/json"]);
        // Connection value should be present
        assert_eq!(result.get("user-agent").unwrap().values, vec!["OpenAct/1.0"]);
        // Task value should be present
        assert_eq!(result.get("x-custom").unwrap().values, vec!["task-header"]);
    }

    #[test]
    fn test_json_conversion() {
        let mut obj = Map::new();
        obj.insert("single".to_string(), Value::String("value1".to_string()));
        obj.insert("multi".to_string(), Value::Array(vec![
            Value::String("value1".to_string()),
            Value::String("value2".to_string()),
        ]));

        let multi_map = MultiValueMerger::from_json_object(&obj);
        assert_eq!(multi_map.get("single").unwrap().values, vec!["value1"]);
        assert_eq!(multi_map.get("multi").unwrap().values, vec!["value1", "value2"]);

        let back_to_json = MultiValueMerger::to_json_object(&multi_map);
        assert_eq!(back_to_json.get("single").unwrap(), &Value::String("value1".to_string()));
        assert!(back_to_json.get("multi").unwrap().is_array());
    }
}
