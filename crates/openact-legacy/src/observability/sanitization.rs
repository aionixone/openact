//! Log and data sanitization utilities
//!
//! Provides functions to sanitize sensitive data from logs, error messages,
//! and debug output to prevent credential leakage.

use serde_json::{Map, Value};
use std::collections::HashSet;

/// List of sensitive field names that should be redacted in logs
static SENSITIVE_FIELDS: &[&str] = &[
    "client_secret",
    "refresh_token",
    "access_token",
    "api_key_value",
    "password",
    "authorization", // for HTTP Authorization headers
    "token",         // generic token fields
    "secret",        // generic secret fields
    "key",           // generic key fields that might contain secrets
];

/// Create a sanitized string representation for logging
///
/// This function takes any serializable value and returns a sanitized JSON string
/// where sensitive fields are replaced with "[REDACTED]"
pub fn sanitize_for_logging<T: serde::Serialize>(data: &T) -> String {
    match serde_json::to_value(data) {
        Ok(value) => {
            let sanitized = sanitize_json_value(value);
            serde_json::to_string(&sanitized)
                .unwrap_or_else(|_| "[SERIALIZATION_ERROR]".to_string())
        }
        Err(_) => "[SERIALIZATION_ERROR]".to_string(),
    }
}

/// Sanitize a JSON value by redacting sensitive fields
pub fn sanitize_json_value(mut value: Value) -> Value {
    match &mut value {
        Value::Object(map) => {
            sanitize_json_object(map);
            Value::Object(map.clone())
        }
        Value::Array(arr) => {
            let sanitized: Vec<Value> =
                arr.iter().map(|v| sanitize_json_value(v.clone())).collect();
            Value::Array(sanitized)
        }
        _ => value,
    }
}

/// Sanitize a JSON object by redacting sensitive fields
fn sanitize_json_object(map: &mut Map<String, Value>) {
    let sensitive_set: HashSet<&str> = SENSITIVE_FIELDS.iter().cloned().collect();

    for (key, value) in map.iter_mut() {
        let key_lower = key.to_lowercase();

        // Check if this field should be redacted
        if sensitive_set.contains(key_lower.as_str())
            || sensitive_set.iter().any(|&field| key_lower.contains(field))
        {
            *value = Value::String("[REDACTED]".to_string());
        } else {
            // Recursively sanitize nested objects and arrays
            match value {
                Value::Object(nested_map) => {
                    sanitize_json_object(nested_map);
                }
                Value::Array(arr) => {
                    for item in arr.iter_mut() {
                        if let Value::Object(nested_map) = item {
                            sanitize_json_object(nested_map);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Sanitize a URL string by removing sensitive query parameters
pub fn sanitize_url(url: &str) -> String {
    // Simple URL sanitization without external dependencies
    // Look for common sensitive query parameters and redact them
    let mut sanitized = url.to_string();

    for &field in SENSITIVE_FIELDS {
        let patterns = [
            format!("{}=", field),
            format!("{} =", field),
            format!("{}:", field),
            format!("{} :", field),
        ];

        for pattern in &patterns {
            if let Some(start) = sanitized.find(pattern) {
                let after_pattern = start + pattern.len();
                if let Some(end) = sanitized[after_pattern..].find('&') {
                    let end_pos = after_pattern + end;
                    sanitized.replace_range(after_pattern..end_pos, "[REDACTED]");
                } else {
                    // Replace to end of string
                    sanitized.replace_range(after_pattern.., "[REDACTED]");
                }
            }
        }
    }

    sanitized
}

/// Sanitize an error message by removing sensitive information
pub fn sanitize_error_message(error_msg: &str) -> String {
    let mut sanitized = error_msg.to_string();

    // Simple pattern-based sanitization without regex
    for &field in SENSITIVE_FIELDS {
        // Look for patterns like "field":"value" or field=value
        let patterns = [
            format!("\"{}\":\"", field),
            format!("\"{}\": \"", field),
            format!("{}:\"", field),
            format!("{}: \"", field),
            format!("{}=\"", field),
            format!("{} =\"", field),
        ];

        for pattern in &patterns {
            if let Some(start) = sanitized.find(pattern) {
                let after_pattern = start + pattern.len();
                if let Some(end) = sanitized[after_pattern..].find('"') {
                    let end_pos = after_pattern + end;
                    sanitized.replace_range(after_pattern..end_pos, "[REDACTED]");
                }
            }
        }
    }

    // Handle Bearer and Basic auth patterns
    if let Some(start) = sanitized.find("Bearer ") {
        let after_bearer = start + 7; // length of "Bearer "
        if let Some(end) = sanitized[after_bearer..].find(' ') {
            let end_pos = after_bearer + end;
            sanitized.replace_range(after_bearer..end_pos, "[REDACTED]");
        } else {
            // Replace to end of string
            sanitized.replace_range(after_bearer.., "[REDACTED]");
        }
    }

    if let Some(start) = sanitized.find("Basic ") {
        let after_basic = start + 6; // length of "Basic "
        if let Some(end) = sanitized[after_basic..].find(' ') {
            let end_pos = after_basic + end;
            sanitized.replace_range(after_basic..end_pos, "[REDACTED]");
        } else {
            // Replace to end of string
            sanitized.replace_range(after_basic.., "[REDACTED]");
        }
    }

    sanitized
}

/// Macro for safe logging that automatically sanitizes the data
#[macro_export]
macro_rules! log_sanitized {
    ($level:ident, $($field:ident = $value:expr),*, $msg:expr) => {
        {
            use $crate::observability::sanitization::sanitize_for_logging;
            tracing::$level!(
                $(
                    $field = %sanitize_for_logging(&$value),
                )*
                $msg
            );
        }
    };
    ($level:ident, $msg:expr) => {
        tracing::$level!($msg);
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_sanitize_json_value() {
        let input = json!({
            "user_id": "123",
            "client_secret": "secret123",
            "access_token": "token456",
            "api_key_value": "key789",
            "password": "pass123",
            "normal_field": "normal_value",
            "nested": {
                "refresh_token": "refresh123",
                "safe_field": "safe_value"
            }
        });

        let sanitized = sanitize_json_value(input);

        assert_eq!(sanitized["user_id"], "123");
        assert_eq!(sanitized["client_secret"], "[REDACTED]");
        assert_eq!(sanitized["access_token"], "[REDACTED]");
        assert_eq!(sanitized["api_key_value"], "[REDACTED]");
        assert_eq!(sanitized["password"], "[REDACTED]");
        assert_eq!(sanitized["normal_field"], "normal_value");
        assert_eq!(sanitized["nested"]["refresh_token"], "[REDACTED]");
        assert_eq!(sanitized["nested"]["safe_field"], "safe_value");
    }

    #[test]
    fn test_sanitize_url() {
        let url = "https://oauth.example.com/token?client_id=123&client_secret=secret&scope=read";
        let sanitized = sanitize_url(url);

        assert!(sanitized.contains("client_id=123"));
        assert!(sanitized.contains("client_secret=[REDACTED]"));
        assert!(sanitized.contains("scope=read"));
    }

    #[test]
    fn test_sanitize_error_message() {
        let error_msg = r#"OAuth error: {"client_secret":"secret123","access_token":"token456"}"#;
        let sanitized = sanitize_error_message(error_msg);

        assert!(sanitized.contains("[REDACTED]"));
        assert!(!sanitized.contains("secret123"));
        assert!(!sanitized.contains("token456"));
    }

    #[test]
    fn test_sanitize_for_logging() {
        use crate::models::connection::OAuth2Parameters;

        let oauth = OAuth2Parameters {
            client_id: "test_client".to_string(),
            client_secret: "secret123".to_string(),
            token_url: "https://oauth.example.com/token".to_string(),
            scope: Some("read write".to_string()),
            redirect_uri: None,
            use_pkce: Some(false),
        };

        let sanitized = sanitize_for_logging(&oauth);

        assert!(sanitized.contains("test_client"));
        assert!(sanitized.contains("[REDACTED]"));
        assert!(!sanitized.contains("secret123"));
    }
}
