//! Utilities for sanitizing sensitive data in logs and error messages

use serde_json::{Value as JsonValue, Map};

/// Fields that should be sanitized (masked) in logs and error messages
const SENSITIVE_FIELDS: &[&str] = &[
    // Authentication related
    "password",
    "token", 
    "access_token",
    "refresh_token",
    "client_secret",
    "api_key",
    "api_key_value",
    "authorization",
    "auth_token",
    "bearer_token",
    "secret",
    "private_key",
    "key",
    // Common sensitive patterns
    "passwd",
    "pwd",
    "credential",
    "credentials",
];

/// Additional patterns to check (case-insensitive)
const SENSITIVE_PATTERNS: &[&str] = &[
    "_key",
    "_token", 
    "_secret",
    "_password",
    "_auth",
];

/// Sanitized placeholder for sensitive values
const SANITIZED_PLACEHOLDER: &str = "***REDACTED***";

/// Check if a field name indicates sensitive data
pub fn is_sensitive_field(field_name: &str) -> bool {
    let field_lower = field_name.to_lowercase();
    
    // Check exact matches
    if SENSITIVE_FIELDS.iter().any(|&sensitive| field_lower == sensitive) {
        return true;
    }
    
    // Check patterns
    SENSITIVE_PATTERNS.iter().any(|&pattern| field_lower.contains(pattern))
}

/// Sanitize a JSON value by replacing sensitive fields with placeholders
pub fn sanitize_json_value(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Object(map) => {
            let sanitized_map: Map<String, JsonValue> = map
                .iter()
                .map(|(key, val)| {
                    let sanitized_val = if is_sensitive_field(key) {
                        // Replace sensitive values with placeholder
                        match val {
                            JsonValue::String(_) => JsonValue::String(SANITIZED_PLACEHOLDER.to_string()),
                            JsonValue::Number(_) => JsonValue::String(SANITIZED_PLACEHOLDER.to_string()),
                            JsonValue::Bool(_) => JsonValue::String(SANITIZED_PLACEHOLDER.to_string()),
                            other => sanitize_json_value(other), // Recursively sanitize objects/arrays
                        }
                    } else {
                        // Recursively sanitize non-sensitive fields
                        sanitize_json_value(val)
                    };
                    (key.clone(), sanitized_val)
                })
                .collect();
            JsonValue::Object(sanitized_map)
        }
        JsonValue::Array(arr) => {
            let sanitized_arr: Vec<JsonValue> = arr
                .iter()
                .map(sanitize_json_value)
                .collect();
            JsonValue::Array(sanitized_arr)
        }
        // Primitive values are not sanitized unless they're in a sensitive field context
        other => other.clone(),
    }
}

/// Sanitize a string representation of JSON or other structured data
pub fn sanitize_string(input: &str) -> String {
    // Try to parse as JSON first
    if let Ok(json_val) = serde_json::from_str::<JsonValue>(input) {
        let sanitized = sanitize_json_value(&json_val);
        serde_json::to_string(&sanitized).unwrap_or_else(|_| input.to_string())
    } else {
        // For non-JSON strings, apply basic pattern matching
        let mut result = input.to_string();
        
        // Replace common patterns like "password=secret123" or "token: abc123"
        for &pattern in &["password=", "token=", "secret=", "key=", "auth="] {
            if let Some(start) = result.to_lowercase().find(pattern) {
                if let Some(end) = result[start..].find(&[' ', ',', '\n', '}', ']'][..]) {
                    let replacement = format!("{}{})", pattern, SANITIZED_PLACEHOLDER);
                    result.replace_range(start..start + end, &replacement);
                }
            }
        }
        
        result
    }
}

/// Create a sanitized display string for debugging
pub fn create_debug_string(prefix: &str, json: &JsonValue) -> String {
    let sanitized = sanitize_json_value(json);
    format!("{}: {}", prefix, sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_is_sensitive_field() {
        // Exact matches
        assert!(is_sensitive_field("password"));
        assert!(is_sensitive_field("Password"));
        assert!(is_sensitive_field("PASSWORD"));
        assert!(is_sensitive_field("api_key"));
        assert!(is_sensitive_field("client_secret"));
        
        // Pattern matches
        assert!(is_sensitive_field("my_secret"));
        assert!(is_sensitive_field("auth_token"));
        assert!(is_sensitive_field("user_key"));
        
        // Non-sensitive fields
        assert!(!is_sensitive_field("username"));
        assert!(!is_sensitive_field("email"));
        assert!(!is_sensitive_field("method"));
        assert!(!is_sensitive_field("path"));
    }

    #[test]
    fn test_sanitize_json_value() {
        let input = json!({
            "username": "john",
            "password": "secret123",
            "api_key": "key_abc123",
            "method": "POST",
            "nested": {
                "client_secret": "oauth_secret",
                "public_field": "visible"
            },
            "tokens": ["token1", "token2"]
        });

        let sanitized = sanitize_json_value(&input);
        
        // Check that sensitive fields are redacted
        assert_eq!(sanitized["password"], SANITIZED_PLACEHOLDER);
        assert_eq!(sanitized["api_key"], SANITIZED_PLACEHOLDER);
        assert_eq!(sanitized["nested"]["client_secret"], SANITIZED_PLACEHOLDER);
        
        // Check that non-sensitive fields are preserved
        assert_eq!(sanitized["username"], "john");
        assert_eq!(sanitized["method"], "POST");
        assert_eq!(sanitized["nested"]["public_field"], "visible");
        
        // Check that arrays are processed recursively
        assert!(sanitized["tokens"].is_array());
    }

    #[test]
    fn test_sanitize_string() {
        let input = r#"{"username": "john", "password": "secret"}"#;
        let result = sanitize_string(input);
        
        assert!(result.contains("john"));
        assert!(result.contains(SANITIZED_PLACEHOLDER));
        assert!(!result.contains("secret"));
    }

    #[test]
    fn test_create_debug_string() {
        let config = json!({
            "method": "GET",
            "api_key": "secret123"
        });
        
        let debug_str = create_debug_string("config_json", &config);
        
        assert!(debug_str.contains("config_json"));
        assert!(debug_str.contains("GET"));
        assert!(debug_str.contains(SANITIZED_PLACEHOLDER));
        assert!(!debug_str.contains("secret123"));
    }
}
