//! HTTP policy management for headers and query parameters

use crate::error::{ConnectorError, ConnectorResult};
use crate::http::connection::HttpPolicy;
use std::collections::HashMap;

/// Manager for applying HTTP policies to headers and query parameters
#[derive(Debug, Clone)]
pub struct PolicyManager {
    policy: HttpPolicy,
}

/// Result of header/query merging with policy applied
#[derive(Debug, Clone)]
pub struct MergeResult {
    pub headers: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
    pub warnings: Vec<String>,
}

impl PolicyManager {
    /// Create a new policy manager with the given policy
    pub fn new(policy: HttpPolicy) -> Self {
        Self { policy }
    }

    /// Merge headers with policy enforcement
    pub fn merge_headers(
        &self,
        connection_headers: &HashMap<String, String>,
        action_headers: &HashMap<String, String>,
    ) -> ConnectorResult<HashMap<String, String>> {
        let mut result = HashMap::new();
        let mut warnings = Vec::new();

        // Start with connection-level headers
        for (key, value) in connection_headers {
            let normalized_key = if self.policy.normalize_header_names {
                key.to_lowercase()
            } else {
                key.clone()
            };

            match self.apply_header_policy(&normalized_key, value) {
                Ok(Some((final_key, final_value))) => {
                    result.insert(final_key, final_value);
                }
                Ok(None) => {
                    warnings.push(format!("Header '{}' was filtered by policy", key));
                }
                Err(e) => {
                    if self.policy.drop_forbidden_headers {
                        warnings.push(format!("Header '{}' dropped: {}", key, e));
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        // Apply action-level headers with conflict resolution
        for (key, value) in action_headers {
            let normalized_key = if self.policy.normalize_header_names {
                key.to_lowercase()
            } else {
                key.clone()
            };

            match self.apply_header_policy(&normalized_key, value) {
                Ok(Some((final_key, final_value))) => {
                    // Check for conflicts and apply merge strategy
                    if let Some(existing_value) = result.get(&final_key) {
                        let merged_value = self.resolve_header_conflict(
                            &final_key, 
                            existing_value, 
                            &final_value
                        )?;
                        result.insert(final_key, merged_value);
                    } else {
                        result.insert(final_key, final_value);
                    }
                }
                Ok(None) => {
                    warnings.push(format!("Action header '{}' was filtered by policy", key));
                }
                Err(e) => {
                    if self.policy.drop_forbidden_headers {
                        warnings.push(format!("Action header '{}' dropped: {}", key, e));
                    } else {
                        return Err(e);
                    }
                }
            }
        }

        // Log warnings if any
        if !warnings.is_empty() {
            for warning in &warnings {
                eprintln!("Warning: {}", warning);
            }
        }

        Ok(result)
    }

    /// Apply header policy to a single header
    fn apply_header_policy(
        &self,
        key: &str,
        value: &str,
    ) -> ConnectorResult<Option<(String, String)>> {
        let normalized_key = key.to_lowercase();

        // Check if header is denied
        if self.policy.denied_headers.iter().any(|h| h.to_lowercase() == normalized_key) {
            return Err(ConnectorError::InvalidConfig(format!(
                "Header '{}' is denied by policy",
                key
            )));
        }

        // Validate header value length
        if value.len() > self.policy.max_header_value_length {
            return Err(ConnectorError::InvalidConfig(format!(
                "Header '{}' value exceeds maximum length ({} > {})",
                key,
                value.len(),
                self.policy.max_header_value_length
            )));
        }

        // Validate header value content (basic security checks)
        if value.contains('\n') || value.contains('\r') {
            return Err(ConnectorError::InvalidConfig(format!(
                "Header '{}' contains invalid characters (newline)",
                key
            )));
        }

        let final_key = if self.policy.normalize_header_names {
            normalized_key
        } else {
            key.to_string()
        };

        Ok(Some((final_key, value.to_string())))
    }

    /// Resolve conflicts between connection and action headers
    fn resolve_header_conflict(
        &self,
        key: &str,
        connection_value: &str,
        action_value: &str,
    ) -> ConnectorResult<String> {
        let normalized_key = key.to_lowercase();

        // Check if this is a reserved header (connection takes precedence)
        if self.policy.reserved_headers.iter().any(|h| h.to_lowercase() == normalized_key) {
            return Ok(connection_value.to_string());
        }

        // Check if this is a multi-value append header
        if self.policy.multi_value_append_headers.iter().any(|h| h.to_lowercase() == normalized_key) {
            // Append with comma separation (HTTP standard)
            return Ok(format!("{}, {}", connection_value, action_value));
        }

        // Default: action overrides connection
        Ok(action_value.to_string())
    }

    /// Merge query parameters (simpler than headers, but still policy-aware)
    pub fn merge_query_params(
        &self,
        connection_params: &HashMap<String, String>,
        action_params: &HashMap<String, Vec<String>>,
    ) -> ConnectorResult<HashMap<String, String>> {
        let mut result = connection_params.clone();

        // Add/override with action parameters
        for (key, values) in action_params {
            if let Some(first_value) = values.first() {
                // For query params, we use simple override strategy
                // Could be enhanced with multi-value support if needed
                result.insert(key.clone(), first_value.clone());
            }
        }

        // Validate total parameter count (prevent query bomb attacks)
        if result.len() > 100 {
            return Err(ConnectorError::InvalidConfig(format!(
                "Too many query parameters ({} > 100)",
                result.len()
            )));
        }

        Ok(result)
    }

    /// Validate final headers count against policy
    pub fn validate_headers_count(&self, headers: &HashMap<String, String>) -> ConnectorResult<()> {
        if headers.len() > self.policy.max_total_headers {
            return Err(ConnectorError::InvalidConfig(format!(
                "Too many headers ({} > {})",
                headers.len(),
                self.policy.max_total_headers
            )));
        }
        Ok(())
    }

    /// Check if a content type is allowed
    pub fn is_content_type_allowed(&self, content_type: &str) -> bool {
        if self.policy.allowed_content_types.is_empty() {
            return true; // No restrictions
        }

        // Extract main type (before semicolon for charset etc.)
        let main_type = content_type.split(';').next().unwrap_or(content_type).trim();
        
        self.policy.allowed_content_types.iter().any(|allowed| {
            allowed.to_lowercase() == main_type.to_lowercase()
        })
    }

    /// Apply all policies and return merge result with warnings
    pub fn apply_policies(
        &self,
        connection_headers: &HashMap<String, String>,
        action_headers: &HashMap<String, String>,
        connection_query: &HashMap<String, String>,
        action_query: &HashMap<String, Vec<String>>,
    ) -> ConnectorResult<MergeResult> {
        // Merge headers with policy
        let headers = self.merge_headers(connection_headers, action_headers)?;
        
        // Validate header count
        self.validate_headers_count(&headers)?;

        // Merge query parameters
        let query_params = self.merge_query_params(connection_query, action_query)?;

        Ok(MergeResult {
            headers,
            query_params,
            warnings: Vec::new(), // Warnings are already logged
        })
    }

    /// Get the current policy
    pub fn get_policy(&self) -> &HttpPolicy {
        &self.policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::connection::HttpPolicy;

    fn create_test_policy() -> HttpPolicy {
        HttpPolicy {
            denied_headers: vec!["x-forbidden".to_string()],
            reserved_headers: vec!["authorization".to_string()],
            multi_value_append_headers: vec!["accept".to_string()],
            drop_forbidden_headers: true,
            normalize_header_names: true,
            max_header_value_length: 1000,
            max_total_headers: 50,
            allowed_content_types: vec![
                "application/json".to_string(),
                "text/plain".to_string(),
            ],
        }
    }

    #[test]
    fn test_header_merging_basic() {
        let policy = create_test_policy();
        let manager = PolicyManager::new(policy);

        let mut connection_headers = HashMap::new();
        connection_headers.insert("Content-Type".to_string(), "application/json".to_string());
        connection_headers.insert("User-Agent".to_string(), "OpenAct/1.0".to_string());

        let mut action_headers = HashMap::new();
        action_headers.insert("X-Custom".to_string(), "test-value".to_string());

        let result = manager.merge_headers(&connection_headers, &action_headers).unwrap();

        // Should normalize header names
        assert_eq!(result.get("content-type"), Some(&"application/json".to_string()));
        assert_eq!(result.get("user-agent"), Some(&"OpenAct/1.0".to_string()));
        assert_eq!(result.get("x-custom"), Some(&"test-value".to_string()));
    }

    #[test]
    fn test_reserved_header_precedence() {
        let policy = create_test_policy();
        let manager = PolicyManager::new(policy);

        let mut connection_headers = HashMap::new();
        connection_headers.insert("Authorization".to_string(), "Bearer token123".to_string());

        let mut action_headers = HashMap::new();
        action_headers.insert("authorization".to_string(), "Bearer different".to_string());

        let result = manager.merge_headers(&connection_headers, &action_headers).unwrap();

        // Connection's authorization should take precedence
        assert_eq!(result.get("authorization"), Some(&"Bearer token123".to_string()));
    }

    #[test]
    fn test_multi_value_append() {
        let policy = create_test_policy();
        let manager = PolicyManager::new(policy);

        let mut connection_headers = HashMap::new();
        connection_headers.insert("Accept".to_string(), "application/json".to_string());

        let mut action_headers = HashMap::new();
        action_headers.insert("accept".to_string(), "text/plain".to_string());

        let result = manager.merge_headers(&connection_headers, &action_headers).unwrap();

        // Should append values
        assert_eq!(result.get("accept"), Some(&"application/json, text/plain".to_string()));
    }

    #[test]
    fn test_denied_header_filtering() {
        let policy = create_test_policy();
        let manager = PolicyManager::new(policy);

        let mut connection_headers = HashMap::new();
        connection_headers.insert("x-forbidden".to_string(), "should-be-filtered".to_string());

        let action_headers = HashMap::new();

        // Should not fail due to drop_forbidden_headers = true
        let result = manager.merge_headers(&connection_headers, &action_headers).unwrap();

        // Forbidden header should be filtered out
        assert!(!result.contains_key("x-forbidden"));
    }

    #[test]
    fn test_header_value_length_validation() {
        let policy = create_test_policy();
        let manager = PolicyManager::new(policy);

        let mut action_headers = HashMap::new();
        let long_value = "x".repeat(2000); // Exceeds max_header_value_length
        action_headers.insert("x-long".to_string(), long_value);

        let connection_headers = HashMap::new();

        // Should not fail due to drop_forbidden_headers = true
        let result = manager.merge_headers(&connection_headers, &action_headers).unwrap();

        // Long header should be filtered out
        assert!(!result.contains_key("x-long"));
    }

    #[test]
    fn test_query_param_merging() {
        let policy = create_test_policy();
        let manager = PolicyManager::new(policy);

        let mut connection_query = HashMap::new();
        connection_query.insert("page".to_string(), "1".to_string());

        let mut action_query = HashMap::new();
        action_query.insert("limit".to_string(), vec!["10".to_string()]);
        action_query.insert("page".to_string(), vec!["2".to_string()]); // Override

        let result = manager.merge_query_params(&connection_query, &action_query).unwrap();

        assert_eq!(result.get("page"), Some(&"2".to_string())); // Action overrides
        assert_eq!(result.get("limit"), Some(&"10".to_string()));
    }

    #[test]
    fn test_content_type_validation() {
        let policy = create_test_policy();
        let manager = PolicyManager::new(policy);

        assert!(manager.is_content_type_allowed("application/json"));
        assert!(manager.is_content_type_allowed("application/json; charset=utf-8"));
        assert!(manager.is_content_type_allowed("text/plain"));
        assert!(!manager.is_content_type_allowed("application/xml"));
        assert!(!manager.is_content_type_allowed("multipart/form-data"));
    }

    #[test]
    fn test_complete_policy_application() {
        let policy = create_test_policy();
        let manager = PolicyManager::new(policy);

        let mut connection_headers = HashMap::new();
        connection_headers.insert("Authorization".to_string(), "Bearer token".to_string());
        connection_headers.insert("Accept".to_string(), "application/json".to_string());

        let mut action_headers = HashMap::new();
        action_headers.insert("X-Custom".to_string(), "test".to_string());
        action_headers.insert("accept".to_string(), "text/plain".to_string());

        let mut connection_query = HashMap::new();
        connection_query.insert("version".to_string(), "v1".to_string());

        let mut action_query = HashMap::new();
        action_query.insert("page".to_string(), vec!["1".to_string()]);

        let result = manager.apply_policies(
            &connection_headers,
            &action_headers,
            &connection_query,
            &action_query,
        ).unwrap();

        // Verify merged results
        assert_eq!(result.headers.get("authorization"), Some(&"Bearer token".to_string()));
        assert_eq!(result.headers.get("accept"), Some(&"application/json, text/plain".to_string()));
        assert_eq!(result.headers.get("x-custom"), Some(&"test".to_string()));
        
        assert_eq!(result.query_params.get("version"), Some(&"v1".to_string()));
        assert_eq!(result.query_params.get("page"), Some(&"1".to_string()));
    }
}
