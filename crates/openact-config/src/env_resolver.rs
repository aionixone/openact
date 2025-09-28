//! Environment variable resolution with whitelist and default value support

use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::env;
use thiserror::Error;

/// Errors that can occur during environment variable resolution
#[derive(Debug, Error)]
pub enum EnvResolverError {
    #[error("Environment variable '{0}' not found and no default provided")]
    VarNotFound(String),
    #[error("Environment variable '{0}' is not in whitelist. Allowed prefixes: {1:?}")]
    VarNotWhitelisted(String, Vec<String>),
    #[error("Invalid variable syntax: '{0}'. Expected ${{VAR}} or ${{VAR:default}}")]
    InvalidSyntax(String),
    #[error("Recursive variable reference detected in '{0}'")]
    RecursiveReference(String),
    #[error("JSON serialization error: {0}")]
    JsonSerialization(String),
}

/// Environment variable resolver with whitelist support
#[derive(Debug, Clone)]
pub struct EnvResolver {
    /// Allowed prefixes for environment variables (e.g., ["OPENACT_", "APP_"])
    /// Empty means no restrictions
    allowed_prefixes: Vec<String>,
    /// Maximum recursion depth to prevent infinite loops
    max_depth: usize,
}

impl Default for EnvResolver {
    fn default() -> Self {
        Self {
            allowed_prefixes: vec![
                "OPENACT_".to_string(),
                "APP_".to_string(),
                "HTTP_".to_string(),
                "DB_".to_string(),
                "PG_".to_string(),
                "LOG_".to_string(),
            ],
            max_depth: 10,
        }
    }
}

impl EnvResolver {
    /// Create a new resolver with specified allowed prefixes
    pub fn new(allowed_prefixes: Vec<String>) -> Self {
        Self {
            allowed_prefixes,
            max_depth: 10,
        }
    }

    /// Create a resolver with no restrictions (allow all variables)
    pub fn unrestricted() -> Self {
        Self {
            allowed_prefixes: vec![],
            max_depth: 10,
        }
    }

    /// Set maximum recursion depth
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Resolve environment variables in a JSON value
    /// Supports ${VAR} and ${VAR:default} syntax
    pub fn resolve(&self, value: &JsonValue) -> Result<JsonValue, EnvResolverError> {
        self.resolve_recursive(value, 0, &mut HashSet::new())
    }

    /// Recursive resolution with depth tracking and cycle detection
    fn resolve_recursive(
        &self,
        value: &JsonValue,
        depth: usize,
        visited: &mut HashSet<String>,
    ) -> Result<JsonValue, EnvResolverError> {
        if depth > self.max_depth {
            return Err(EnvResolverError::RecursiveReference(
                "Maximum recursion depth exceeded".to_string(),
            ));
        }

        match value {
            JsonValue::String(s) => self.resolve_string(s, depth, visited),
            JsonValue::Object(obj) => {
                let mut resolved_obj = serde_json::Map::new();
                for (key, val) in obj {
                    let resolved_val = self.resolve_recursive(val, depth + 1, visited)?;
                    resolved_obj.insert(key.clone(), resolved_val);
                }
                Ok(JsonValue::Object(resolved_obj))
            }
            JsonValue::Array(arr) => {
                let mut resolved_arr = Vec::new();
                for item in arr {
                    let resolved_item = self.resolve_recursive(item, depth + 1, visited)?;
                    resolved_arr.push(resolved_item);
                }
                Ok(JsonValue::Array(resolved_arr))
            }
            // Numbers, booleans, and null pass through unchanged
            other => Ok(other.clone()),
        }
    }

    /// Resolve environment variables in a string
    fn resolve_string(
        &self,
        input: &str,
        depth: usize,
        visited: &mut HashSet<String>,
    ) -> Result<JsonValue, EnvResolverError> {
        // Pattern matches ${VAR} or ${VAR:default}
        let re = Regex::new(r"\$\{([^}:]+)(?::([^}]*))?\}").unwrap();

        // Check for simple cases first
        if !input.contains("${") {
            return Ok(JsonValue::String(input.to_string()));
        }

        // Track this string to detect cycles
        if visited.contains(input) {
            return Err(EnvResolverError::RecursiveReference(input.to_string()));
        }
        visited.insert(input.to_string());

        let mut result = input.to_string();
        let mut changed = true;

        // Keep resolving until no more changes (handles nested variables)
        while changed {
            changed = false;
            let mut new_result = result.clone();

            for caps in re.captures_iter(&result) {
                let full_match = &caps[0];
                let var_name = &caps[1];
                let default_value = caps.get(2).map(|m| m.as_str());

                // Validate variable name against whitelist
                self.validate_var_name(var_name)?;

                // Get environment variable value
                let env_value = match env::var(var_name) {
                    Ok(value) => value,
                    Err(_) => match default_value {
                        Some(default) => default.to_string(),
                        None => {
                            return Err(EnvResolverError::VarNotFound(var_name.to_string()));
                        }
                    },
                };

                // Replace the variable reference
                new_result = new_result.replace(full_match, &env_value);
                changed = true;
            }

            result = new_result;

            // Prevent infinite loops
            if depth > self.max_depth {
                return Err(EnvResolverError::RecursiveReference(input.to_string()));
            }
        }

        visited.remove(input);

        // Try to parse as JSON if it looks like structured data
        if result.starts_with('{') || result.starts_with('[') || result.starts_with('"') {
            match serde_json::from_str(&result) {
                Ok(parsed) => Ok(parsed),
                Err(_) => Ok(JsonValue::String(result)), // Fallback to string if parsing fails
            }
        } else {
            // Try to parse as primitive types
            if let Ok(bool_val) = result.parse::<bool>() {
                Ok(JsonValue::Bool(bool_val))
            } else if let Ok(int_val) = result.parse::<i64>() {
                Ok(JsonValue::Number(serde_json::Number::from(int_val)))
            } else if let Ok(float_val) = result.parse::<f64>() {
                if let Some(num) = serde_json::Number::from_f64(float_val) {
                    Ok(JsonValue::Number(num))
                } else {
                    Ok(JsonValue::String(result))
                }
            } else {
                Ok(JsonValue::String(result))
            }
        }
    }

    /// Validate variable name against whitelist
    fn validate_var_name(&self, var_name: &str) -> Result<(), EnvResolverError> {
        // If no prefixes specified, allow all
        if self.allowed_prefixes.is_empty() {
            return Ok(());
        }

        // Check if variable name starts with any allowed prefix
        for prefix in &self.allowed_prefixes {
            if var_name.starts_with(prefix) {
                return Ok(());
            }
        }

        Err(EnvResolverError::VarNotWhitelisted(
            var_name.to_string(),
            self.allowed_prefixes.clone(),
        ))
    }

    /// Validate that all variables in a JSON value are allowed
    pub fn validate_all_vars(&self, value: &JsonValue) -> Result<(), EnvResolverError> {
        match value {
            JsonValue::String(s) => self.validate_string_vars(s),
            JsonValue::Object(obj) => {
                for val in obj.values() {
                    self.validate_all_vars(val)?;
                }
                Ok(())
            }
            JsonValue::Array(arr) => {
                for item in arr {
                    self.validate_all_vars(item)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Validate variables in a string without resolving them
    fn validate_string_vars(&self, input: &str) -> Result<(), EnvResolverError> {
        let re = Regex::new(r"\$\{([^}:]+)(?::([^}]*))?\}").unwrap();

        for caps in re.captures_iter(input) {
            let var_name = &caps[1];
            self.validate_var_name(var_name)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::env;

    #[test]
    fn test_basic_variable_resolution() {
        env::set_var("OPENACT_TEST_VAR", "test_value");

        let resolver = EnvResolver::default();
        let input = json!("${OPENACT_TEST_VAR}");
        let result = resolver.resolve(&input).unwrap();

        assert_eq!(result, json!("test_value"));

        env::remove_var("OPENACT_TEST_VAR");
    }

    #[test]
    fn test_default_value() {
        env::remove_var("OPENACT_NONEXISTENT");

        let resolver = EnvResolver::default();
        let input = json!("${OPENACT_NONEXISTENT:default_value}");
        let result = resolver.resolve(&input).unwrap();

        assert_eq!(result, json!("default_value"));
    }

    #[test]
    fn test_missing_variable_error() {
        env::remove_var("OPENACT_MISSING");

        let resolver = EnvResolver::default();
        let input = json!("${OPENACT_MISSING}");
        let result = resolver.resolve(&input);

        assert!(matches!(result, Err(EnvResolverError::VarNotFound(_))));
    }

    #[test]
    fn test_whitelist_validation() {
        let resolver = EnvResolver::new(vec!["ALLOWED_".to_string()]);
        let input = json!("${FORBIDDEN_VAR}");
        let result = resolver.resolve(&input);

        assert!(matches!(
            result,
            Err(EnvResolverError::VarNotWhitelisted(_, _))
        ));
    }

    #[test]
    fn test_complex_object_resolution() {
        env::set_var("OPENACT_HOST", "localhost");
        env::set_var("OPENACT_PORT", "8080");

        let resolver = EnvResolver::default();
        let input = json!({
            "connection": {
                "host": "${OPENACT_HOST}",
                "port": "${OPENACT_PORT}",
                "url": "http://${OPENACT_HOST}:${OPENACT_PORT}/api"
            },
            "timeout": "${OPENACT_TIMEOUT:30}"
        });

        let result = resolver.resolve(&input).unwrap();
        let expected = json!({
            "connection": {
                "host": "localhost",
                "port": 8080,
                "url": "http://localhost:8080/api"
            },
            "timeout": 30
        });

        assert_eq!(result, expected);

        env::remove_var("OPENACT_HOST");
        env::remove_var("OPENACT_PORT");
    }

    #[test]
    fn test_type_parsing() {
        env::set_var("OPENACT_BOOL", "true");
        env::set_var("OPENACT_INT", "42");
        env::set_var("OPENACT_FLOAT", "3.14");

        let resolver = EnvResolver::default();

        // Test boolean parsing
        let bool_result = resolver.resolve(&json!("${OPENACT_BOOL}")).unwrap();
        assert_eq!(bool_result, json!(true));

        // Test integer parsing
        let int_result = resolver.resolve(&json!("${OPENACT_INT}")).unwrap();
        assert_eq!(int_result, json!(42));

        // Test float parsing
        let float_result = resolver.resolve(&json!("${OPENACT_FLOAT}")).unwrap();
        assert_eq!(float_result, json!(3.14));

        env::remove_var("OPENACT_BOOL");
        env::remove_var("OPENACT_INT");
        env::remove_var("OPENACT_FLOAT");
    }

    #[test]
    fn test_json_object_parsing() {
        env::set_var("OPENACT_JSON", r#"{"key": "value", "count": 10}"#);

        let resolver = EnvResolver::default();
        let result = resolver.resolve(&json!("${OPENACT_JSON}")).unwrap();

        assert_eq!(result, json!({"key": "value", "count": 10}));

        env::remove_var("OPENACT_JSON");
    }

    #[test]
    fn test_unrestricted_resolver() {
        env::set_var("ANY_VAR_NAME", "test_value");

        let resolver = EnvResolver::unrestricted();
        let result = resolver.resolve(&json!("${ANY_VAR_NAME}")).unwrap();

        assert_eq!(result, json!("test_value"));

        env::remove_var("ANY_VAR_NAME");
    }

    #[test]
    fn test_validation_without_resolution() {
        let resolver = EnvResolver::new(vec!["ALLOWED_".to_string()]);

        // Valid variables
        let valid_input = json!({
            "var1": "${ALLOWED_VAR1}",
            "var2": "${ALLOWED_VAR2:default}"
        });
        assert!(resolver.validate_all_vars(&valid_input).is_ok());

        // Invalid variable
        let invalid_input = json!("${FORBIDDEN_VAR}");
        assert!(resolver.validate_all_vars(&invalid_input).is_err());
    }
}
