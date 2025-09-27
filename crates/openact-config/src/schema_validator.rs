//! Schema validation for connector-specific configurations

use openact_core::ConnectorKind;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Errors that can occur during schema validation
#[derive(Debug, Error)]
pub enum SchemaValidationError {
    #[error("Missing required field: '{0}'")]
    MissingRequiredField(String),
    #[error("Invalid field type for '{field}': expected {expected}, got {actual}")]
    InvalidFieldType {
        field: String,
        expected: String,
        actual: String,
    },
    #[error("Invalid enum value for '{field}': '{value}'. Valid values: {valid_values:?}")]
    InvalidEnumValue {
        field: String,
        value: String,
        valid_values: Vec<String>,
    },
    #[error("Field '{field}' value '{value}' is out of range: {constraint}")]
    ValueOutOfRange {
        field: String,
        value: String,
        constraint: String,
    },
    #[error("Unsupported connector type: '{0}'")]
    UnsupportedConnectorType(String),
    #[error("Invalid URL format in field '{field}': '{value}'")]
    InvalidUrl { field: String, value: String },
    #[error("Missing connection reference in action: '{0}'")]
    MissingConnectionReference(String),
    #[error("Duplicate resource name: '{0}'")]
    DuplicateResourceName(String),
    #[error("Invalid TRN format: '{0}'")]
    InvalidTrnFormat(String),
    #[error("Custom validation error: {0}")]
    CustomValidation(String),
}

/// Configuration schema validator
pub struct SchemaValidator {
    /// Registered validators by connector type
    validators: HashMap<ConnectorKind, Box<dyn ConnectorValidator + Send + Sync>>,
}

/// Trait for connector-specific validation
pub trait ConnectorValidator {
    /// Validate a connection configuration
    fn validate_connection(&self, config: &JsonValue) -> Result<(), SchemaValidationError>;

    /// Validate an action configuration
    fn validate_action(&self, config: &JsonValue) -> Result<(), SchemaValidationError>;

    /// Get supported connector kind
    fn connector_kind(&self) -> ConnectorKind;
}

impl Default for SchemaValidator {
    fn default() -> Self {
        let mut validator = Self {
            validators: HashMap::new(),
        };

        // Register HTTP validator by default
        validator.register_validator(Box::new(HttpValidator::new()));

        validator
    }
}

impl SchemaValidator {
    /// Create a new schema validator
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a connector-specific validator
    pub fn register_validator(&mut self, validator: Box<dyn ConnectorValidator + Send + Sync>) {
        let kind = validator.connector_kind();
        self.validators.insert(kind, validator);
    }

    /// Validate a connection configuration for a specific connector
    pub fn validate_connection(
        &self,
        connector_kind: &ConnectorKind,
        config: &JsonValue,
    ) -> Result<(), SchemaValidationError> {
        match self.validators.get(connector_kind) {
            Some(validator) => validator.validate_connection(config),
            None => Err(SchemaValidationError::UnsupportedConnectorType(
                connector_kind.to_string(),
            )),
        }
    }

    /// Validate an action configuration for a specific connector
    pub fn validate_action(
        &self,
        connector_kind: &ConnectorKind,
        config: &JsonValue,
    ) -> Result<(), SchemaValidationError> {
        match self.validators.get(connector_kind) {
            Some(validator) => validator.validate_action(config),
            None => Err(SchemaValidationError::UnsupportedConnectorType(
                connector_kind.to_string(),
            )),
        }
    }

    /// Get list of supported connector types
    pub fn supported_connectors(&self) -> Vec<ConnectorKind> {
        self.validators.keys().cloned().collect()
    }
}

/// HTTP connector validator
#[derive(Debug)]
pub struct HttpValidator {
    valid_methods: HashSet<String>,
    valid_auth_types: HashSet<String>,
}

impl HttpValidator {
    /// Create a new HTTP validator
    pub fn new() -> Self {
        let mut valid_methods = HashSet::new();
        valid_methods.insert("GET".to_string());
        valid_methods.insert("POST".to_string());
        valid_methods.insert("PUT".to_string());
        valid_methods.insert("PATCH".to_string());
        valid_methods.insert("DELETE".to_string());
        valid_methods.insert("HEAD".to_string());
        valid_methods.insert("OPTIONS".to_string());

        let mut valid_auth_types = HashSet::new();
        valid_auth_types.insert("none".to_string());
        valid_auth_types.insert("basic".to_string());
        valid_auth_types.insert("bearer".to_string());
        valid_auth_types.insert("api_key".to_string());
        valid_auth_types.insert("oauth2".to_string());

        Self {
            valid_methods,
            valid_auth_types,
        }
    }

    /// Validate URL format
    fn validate_url(&self, field: &str, url: &str) -> Result<(), SchemaValidationError> {
        // Basic URL validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(SchemaValidationError::InvalidUrl {
                field: field.to_string(),
                value: url.to_string(),
            });
        }
        Ok(())
    }

    /// Validate timeout value (in milliseconds)
    fn validate_timeout(
        &self,
        field: &str,
        value: &JsonValue,
    ) -> Result<(), SchemaValidationError> {
        match value {
            JsonValue::Number(n) => {
                if let Some(val) = n.as_u64() {
                    // Timeout values are in milliseconds, so reasonable range is 1ms to 300000ms (5 minutes)
                    if val == 0 || val > 300_000 {
                        return Err(SchemaValidationError::ValueOutOfRange {
                            field: field.to_string(),
                            value: val.to_string(),
                            constraint: "must be between 1 and 300000 milliseconds".to_string(),
                        });
                    }
                } else {
                    return Err(SchemaValidationError::InvalidFieldType {
                        field: field.to_string(),
                        expected: "positive integer".to_string(),
                        actual: "invalid number".to_string(),
                    });
                }
            }
            _ => {
                return Err(SchemaValidationError::InvalidFieldType {
                    field: field.to_string(),
                    expected: "number".to_string(),
                    actual: self.get_json_type(value).to_string(),
                });
            }
        }
        Ok(())
    }

    /// Get JSON value type as string
    fn get_json_type(&self, value: &JsonValue) -> &'static str {
        match value {
            JsonValue::Null => "null",
            JsonValue::Bool(_) => "boolean",
            JsonValue::Number(_) => "number",
            JsonValue::String(_) => "string",
            JsonValue::Array(_) => "array",
            JsonValue::Object(_) => "object",
        }
    }

    /// Validate required string field
    fn validate_required_string(
        &self,
        obj: &serde_json::Map<String, JsonValue>,
        field: &str,
    ) -> Result<String, SchemaValidationError> {
        match obj.get(field) {
            Some(JsonValue::String(s)) => {
                if s.is_empty() {
                    Err(SchemaValidationError::InvalidFieldType {
                        field: field.to_string(),
                        expected: "non-empty string".to_string(),
                        actual: "empty string".to_string(),
                    })
                } else {
                    Ok(s.clone())
                }
            }
            Some(other) => Err(SchemaValidationError::InvalidFieldType {
                field: field.to_string(),
                expected: "string".to_string(),
                actual: self.get_json_type(other).to_string(),
            }),
            None => Err(SchemaValidationError::MissingRequiredField(
                field.to_string(),
            )),
        }
    }

    /// Validate optional object field
    fn validate_optional_object<'a>(
        &self,
        obj: &'a serde_json::Map<String, JsonValue>,
        field: &str,
    ) -> Result<Option<&'a serde_json::Map<String, JsonValue>>, SchemaValidationError> {
        match obj.get(field) {
            Some(JsonValue::Object(o)) => Ok(Some(o)),
            Some(other) => Err(SchemaValidationError::InvalidFieldType {
                field: field.to_string(),
                expected: "object".to_string(),
                actual: self.get_json_type(other).to_string(),
            }),
            None => Ok(None),
        }
    }
}

impl ConnectorValidator for HttpValidator {
    fn validate_connection(&self, config: &JsonValue) -> Result<(), SchemaValidationError> {
        let obj = match config.as_object() {
            Some(obj) => obj,
            None => {
                return Err(SchemaValidationError::InvalidFieldType {
                    field: "root".to_string(),
                    expected: "object".to_string(),
                    actual: self.get_json_type(config).to_string(),
                });
            }
        };

        // Validate required base_url
        let base_url = self.validate_required_string(obj, "base_url")?;
        self.validate_url("base_url", &base_url)?;

        // Validate optional auth configuration
        if let Some(auth_obj) = self.validate_optional_object(obj, "auth")? {
            if let Some(JsonValue::String(auth_type)) = auth_obj.get("type") {
                if !self.valid_auth_types.contains(auth_type) {
                    return Err(SchemaValidationError::InvalidEnumValue {
                        field: "auth.type".to_string(),
                        value: auth_type.clone(),
                        valid_values: self.valid_auth_types.iter().cloned().collect(),
                    });
                }

                // Validate auth-specific fields
                match auth_type.as_str() {
                    "basic" => {
                        self.validate_required_string(auth_obj, "username")?;
                        self.validate_required_string(auth_obj, "password")?;
                    }
                    "bearer" => {
                        self.validate_required_string(auth_obj, "token")?;
                    }
                    "api_key" => {
                        self.validate_required_string(auth_obj, "key")?;
                        self.validate_required_string(auth_obj, "value")?;
                    }
                    "oauth2" => {
                        self.validate_required_string(auth_obj, "client_id")?;
                        self.validate_required_string(auth_obj, "client_secret")?;
                        if let Some(JsonValue::String(grant_type)) = auth_obj.get("grant_type") {
                            if !["client_credentials", "authorization_code"]
                                .contains(&grant_type.as_str())
                            {
                                return Err(SchemaValidationError::InvalidEnumValue {
                                    field: "auth.grant_type".to_string(),
                                    value: grant_type.clone(),
                                    valid_values: vec![
                                        "client_credentials".to_string(),
                                        "authorization_code".to_string(),
                                    ],
                                });
                            }
                        }
                    }
                    _ => {} // "none" or other types don't require additional fields
                }
            }
        }

        // Validate optional timeout configuration
        if let Some(timeout_obj) = self.validate_optional_object(obj, "timeout_config")? {
            if let Some(connect_timeout) = timeout_obj.get("connect_timeout_ms") {
                self.validate_timeout("timeout_config.connect_timeout_ms", connect_timeout)?;
            }
            if let Some(read_timeout) = timeout_obj.get("read_timeout_ms") {
                self.validate_timeout("timeout_config.read_timeout_ms", read_timeout)?;
            }
            if let Some(total_timeout) = timeout_obj.get("total_timeout_ms") {
                self.validate_timeout("timeout_config.total_timeout_ms", total_timeout)?;
            }
        }

        Ok(())
    }

    fn validate_action(&self, config: &JsonValue) -> Result<(), SchemaValidationError> {
        let obj = match config.as_object() {
            Some(obj) => obj,
            None => {
                return Err(SchemaValidationError::InvalidFieldType {
                    field: "root".to_string(),
                    expected: "object".to_string(),
                    actual: self.get_json_type(config).to_string(),
                });
            }
        };

        // Validate required method
        let method = self.validate_required_string(obj, "method")?;
        if !self.valid_methods.contains(&method.to_uppercase()) {
            return Err(SchemaValidationError::InvalidEnumValue {
                field: "method".to_string(),
                value: method,
                valid_values: self.valid_methods.iter().cloned().collect(),
            });
        }

        // Validate required path
        let path = self.validate_required_string(obj, "path")?;
        if !path.starts_with('/') {
            return Err(SchemaValidationError::InvalidFieldType {
                field: "path".to_string(),
                expected: "path starting with '/'".to_string(),
                actual: format!("path '{}'", path),
            });
        }

        // Validate optional headers (should be an object)
        self.validate_optional_object(obj, "headers")?;

        // Validate optional query_params (should be an object)
        self.validate_optional_object(obj, "query_params")?;

        // Validate optional request_body (can be any type)
        // No specific validation needed for request_body as it's flexible

        // Validate optional body (should be an object with specific structure)
        if let Some(body_obj) = self.validate_optional_object(obj, "body")? {
            if let Some(JsonValue::String(body_type)) = body_obj.get("type") {
                let valid_body_types = vec!["json", "form", "multipart", "raw", "text"];
                if !valid_body_types.contains(&body_type.as_str()) {
                    return Err(SchemaValidationError::InvalidEnumValue {
                        field: "body.type".to_string(),
                        value: body_type.clone(),
                        valid_values: valid_body_types.iter().map(|s| s.to_string()).collect(),
                    });
                }
            }
        }

        Ok(())
    }

    fn connector_kind(&self) -> ConnectorKind {
        ConnectorKind::new("http")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_http_connection_validation_success() {
        let validator = SchemaValidator::new();
        let config = json!({
            "base_url": "https://api.example.com",
            "auth": {
                "type": "bearer",
                "token": "test-token"
            },
            "timeout_config": {
                "connect_timeout_ms": 5000,
                "read_timeout_ms": 30000
            }
        });

        let result = validator.validate_connection(&ConnectorKind::new("http"), &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_http_connection_validation_missing_base_url() {
        let validator = SchemaValidator::new();
        let config = json!({
            "auth": {
                "type": "none"
            }
        });

        let result = validator.validate_connection(&ConnectorKind::new("http"), &config);
        assert!(matches!(
            result,
            Err(SchemaValidationError::MissingRequiredField(_))
        ));
    }

    #[test]
    fn test_http_connection_validation_invalid_url() {
        let validator = SchemaValidator::new();
        let config = json!({
            "base_url": "invalid-url"
        });

        let result = validator.validate_connection(&ConnectorKind::new("http"), &config);
        assert!(matches!(
            result,
            Err(SchemaValidationError::InvalidUrl { .. })
        ));
    }

    #[test]
    fn test_http_action_validation_success() {
        let validator = SchemaValidator::new();
        let config = json!({
            "method": "POST",
            "path": "/api/users",
            "headers": {
                "Content-Type": "application/json"
            },
            "body": {
                "type": "json",
                "data": {"name": "test"}
            }
        });

        let result = validator.validate_action(&ConnectorKind::new("http"), &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_http_action_validation_invalid_method() {
        let validator = SchemaValidator::new();
        let config = json!({
            "method": "INVALID",
            "path": "/api/test"
        });

        let result = validator.validate_action(&ConnectorKind::new("http"), &config);
        assert!(matches!(
            result,
            Err(SchemaValidationError::InvalidEnumValue { .. })
        ));
    }

    #[test]
    fn test_http_action_validation_invalid_path() {
        let validator = SchemaValidator::new();
        let config = json!({
            "method": "GET",
            "path": "invalid-path-without-slash"
        });

        let result = validator.validate_action(&ConnectorKind::new("http"), &config);
        assert!(matches!(
            result,
            Err(SchemaValidationError::InvalidFieldType { .. })
        ));
    }

    #[test]
    fn test_unsupported_connector_type() {
        let validator = SchemaValidator::new();
        let config = json!({});

        let result = validator.validate_connection(&ConnectorKind::new("unsupported"), &config);
        assert!(matches!(
            result,
            Err(SchemaValidationError::UnsupportedConnectorType(_))
        ));
    }

    #[test]
    fn test_auth_type_validation() {
        let validator = SchemaValidator::new();

        // Test basic auth
        let basic_config = json!({
            "base_url": "https://api.example.com",
            "auth": {
                "type": "basic",
                "username": "user",
                "password": "pass"
            }
        });
        assert!(validator
            .validate_connection(&ConnectorKind::new("http"), &basic_config)
            .is_ok());

        // Test OAuth2
        let oauth_config = json!({
            "base_url": "https://api.example.com",
            "auth": {
                "type": "oauth2",
                "client_id": "client123",
                "client_secret": "secret456",
                "grant_type": "client_credentials"
            }
        });
        assert!(validator
            .validate_connection(&ConnectorKind::new("http"), &oauth_config)
            .is_ok());

        // Test invalid auth type
        let invalid_config = json!({
            "base_url": "https://api.example.com",
            "auth": {
                "type": "invalid_auth"
            }
        });
        assert!(validator
            .validate_connection(&ConnectorKind::new("http"), &invalid_config)
            .is_err());
    }

    #[test]
    fn test_supported_connectors() {
        let validator = SchemaValidator::new();
        let connectors = validator.supported_connectors();
        assert!(connectors.contains(&ConnectorKind::new("http")));
        assert_eq!(connectors.len(), 1);
    }
}
