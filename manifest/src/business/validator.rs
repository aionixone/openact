use crate::utils::error::Result;
use crate::spec::OpenApi30Spec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub error_type: String,
    pub field: String,
    pub message: String,
    pub location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub warning_type: String,
    pub message: String,
    pub location: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub openapi_spec: Option<OpenApi30Spec>,
}

pub struct OpenApiValidator {
    strict_mode: bool,
}

impl OpenApiValidator {
    pub fn new() -> Self {
        Self { strict_mode: false }
    }

    pub fn with_strict_mode(mut self, strict: bool) -> Self {
        self.strict_mode = strict;
        self
    }

    /// Validate OpenAPI document
    pub fn validate(&self, content: &str) -> Result<ValidationResult> {
        // Detect format and parse
        if self.is_json_format(content) {
            self.validate_json(content)
        } else {
            self.validate_yaml(content)
        }
    }

    /// Validate JSON format OpenAPI document
    pub fn validate_json(&self, content: &str) -> Result<ValidationResult> {
        match serde_json::from_str::<OpenApi30Spec>(content) {
            Ok(spec) => self.validate_openapi_spec(spec),
            Err(e) => Ok(ValidationResult {
                valid: false,
                errors: vec![ValidationError {
                    error_type: "JSON_PARSE_ERROR".to_string(),
                    field: "root".to_string(),
                    message: format!("Failed to parse JSON: {}", e),
                    location: "document".to_string(),
                }],
                warnings: vec![],
                openapi_spec: None,
            }),
        }
    }

    /// Validate YAML format OpenAPI document
    pub fn validate_yaml(&self, content: &str) -> Result<ValidationResult> {
        match serde_yaml::from_str::<OpenApi30Spec>(content) {
            Ok(spec) => self.validate_openapi_spec(spec),
            Err(e) => Ok(ValidationResult {
                valid: false,
                errors: vec![ValidationError {
                    error_type: "YAML_PARSE_ERROR".to_string(),
                    field: "root".to_string(),
                    message: format!("Failed to parse YAML: {}", e),
                    location: "document".to_string(),
                }],
                warnings: vec![],
                openapi_spec: None,
            }),
        }
    }

    /// Validate parsed OpenAPI specification
    fn validate_openapi_spec(&self, spec: OpenApi30Spec) -> Result<ValidationResult> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Validate OpenAPI version
        if let Err(e) = spec.validate_version() {
            errors.push(ValidationError {
                error_type: "INVALID_VERSION".to_string(),
                field: "openapi".to_string(),
                message: e,
                location: "root".to_string(),
            });
        }

        // Validate Info object
        if let Err(e) = spec.info.validate() {
            errors.push(ValidationError {
                error_type: "INVALID_INFO".to_string(),
                field: "info".to_string(),
                message: e,
                location: "root".to_string(),
            });
        }

        // Validate Paths
        if spec.paths.paths.is_empty() {
            warnings.push(ValidationWarning {
                warning_type: "EMPTY_PATHS".to_string(),
                message: "No paths defined in the API".to_string(),
                location: "paths".to_string(),
                suggestion: Some("Add at least one path to your API".to_string()),
            });
        }

        // Validate Parameters
        for (path_name, path_item) in &spec.paths.paths {
            // Validate path-level parameters
            for param_ref in &path_item.parameters {
                if let crate::spec::OrReference::Item(param) = param_ref {
                    if let Err(e) = param.validate() {
                        errors.push(ValidationError {
                            error_type: "INVALID_PARAMETER".to_string(),
                            field: "parameters".to_string(),
                            message: e,
                            location: format!("paths.{}", path_name),
                        });
                    }
                }
            }

            // Validate operation-level parameters
            for (method, operation) in [
                ("get", &path_item.get),
                ("post", &path_item.post),
                ("put", &path_item.put),
                ("delete", &path_item.delete),
                ("options", &path_item.options),
                ("head", &path_item.head),
                ("patch", &path_item.patch),
                ("trace", &path_item.trace),
            ].iter().filter_map(|(m, op)| op.as_ref().map(|o| (*m, o))) {
                
                for param_ref in &operation.parameters {
                    if let crate::spec::OrReference::Item(param) = param_ref {
                        if let Err(e) = param.validate() {
                            errors.push(ValidationError {
                                error_type: "INVALID_PARAMETER".to_string(),
                                field: "parameters".to_string(),
                                message: e,
                                location: format!("paths.{}.{}", path_name, method),
                            });
                        }
                    }
                }

                // Validate responses
                if let Err(e) = operation.responses.validate() {
                    errors.push(ValidationError {
                        error_type: "INVALID_RESPONSES".to_string(),
                        field: "responses".to_string(),
                        message: e,
                        location: format!("paths.{}.{}", path_name, method),
                    });
                }

                // Check for missing operationId warning
                if operation.operation_id.is_none() && self.strict_mode {
                    warnings.push(ValidationWarning {
                        warning_type: "MISSING_OPERATION_ID".to_string(),
                        message: format!("Operation {} {} has no operationId", method.to_uppercase(), path_name),
                        location: format!("paths.{}.{}", path_name, method),
                        suggestion: Some("Add an operationId for better API tooling support".to_string()),
                    });
                }
            }
        }

        // Validate Tags
        for tag in &spec.tags {
            if let Err(e) = tag.validate() {
                errors.push(ValidationError {
                    error_type: "INVALID_TAG".to_string(),
                    field: "tags".to_string(),
                    message: e,
                    location: "root".to_string(),
                });
            }
        }

        // Validate External Documentation
        if let Some(ref ext_docs) = spec.external_docs {
            if let Err(e) = ext_docs.validate() {
                errors.push(ValidationError {
                    error_type: "INVALID_EXTERNAL_DOCS".to_string(),
                    field: "externalDocs".to_string(),
                    message: e,
                    location: "root".to_string(),
                });
            }
        }

        // Check for missing servers warning
        if spec.servers.is_empty() {
            warnings.push(ValidationWarning {
                warning_type: "MISSING_SERVERS".to_string(),
                message: "No servers defined".to_string(),
                location: "root".to_string(),
                suggestion: Some("Define at least one server for your API".to_string()),
            });
        }

        Ok(ValidationResult {
            valid: errors.is_empty(),
            errors,
            warnings,
            openapi_spec: Some(spec),
        })
    }

    /// Check if content is in JSON format
    fn is_json_format(&self, content: &str) -> bool {
        content.trim_start().starts_with('{')
    }

    /// Check if content is in YAML format
    #[allow(dead_code)]
    fn is_yaml_format(&self, content: &str) -> bool {
        !self.is_json_format(content)
    }
}

impl Default for OpenApiValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_creation() {
        let validator = OpenApiValidator::new();
        assert!(!validator.strict_mode);
    }

    #[test]
    fn test_validator_strict_mode() {
        let validator = OpenApiValidator::new().with_strict_mode(true);
        assert!(validator.strict_mode);
    }

    #[test]
    fn test_format_detection() {
        let validator = OpenApiValidator::new();
        
        assert!(validator.is_json_format(r#"{"openapi": "3.0.0"}"#));
        assert!(!validator.is_yaml_format(r#"{"openapi": "3.0.0"}"#));
        
        assert!(validator.is_yaml_format("openapi: 3.0.0"));
        assert!(!validator.is_json_format("openapi: 3.0.0"));
    }

    #[test]
    fn test_validate_invalid_json() {
        let validator = OpenApiValidator::new();
        let result = validator.validate("{ invalid json").unwrap();
        
        assert!(!result.valid);
        assert!(!result.errors.is_empty());
        assert_eq!(result.errors[0].error_type, "JSON_PARSE_ERROR");
    }

    #[test]
    fn test_validate_minimal_valid_spec() {
        let validator = OpenApiValidator::new();
        let minimal_spec = r#"{
            "openapi": "3.0.0",
            "info": {
                "title": "Test API",
                "version": "1.0.0"
            },
            "paths": {}
        }"#;
        
        let result = validator.validate(minimal_spec).unwrap();
        assert!(result.valid);
        assert!(result.errors.is_empty());
        assert!(!result.warnings.is_empty()); // Should warn about empty paths
    }
}
