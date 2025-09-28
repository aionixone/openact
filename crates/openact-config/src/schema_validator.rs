//! Schema validation for connector-specific configurations

use openact_core::ConnectorKind;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
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

/// Trait implemented by connector-specific validators.
pub trait ConnectorValidator {
    fn validate_connection(&self, config: &JsonValue) -> Result<(), SchemaValidationError>;
    fn validate_action(&self, config: &JsonValue) -> Result<(), SchemaValidationError>;
    fn connector_kind(&self) -> ConnectorKind;
}

/// Aggregates connector validators and exposes a simple interface.
pub struct SchemaValidator {
    validators: HashMap<ConnectorKind, Box<dyn ConnectorValidator + Send + Sync>>,
}

impl SchemaValidator {
    pub fn new() -> Self {
        Self {
            validators: HashMap::new(),
        }
    }

    pub fn register_validator(&mut self, validator: Box<dyn ConnectorValidator + Send + Sync>) {
        let kind = validator.connector_kind();
        self.validators.insert(kind, validator);
    }

    pub fn validate_connection(
        &self,
        connector_kind: &ConnectorKind,
        config: &JsonValue,
    ) -> Result<(), SchemaValidationError> {
        match self.validators.get(connector_kind) {
            Some(v) => v.validate_connection(config),
            None => Ok(()),
        }
    }

    pub fn validate_action(
        &self,
        connector_kind: &ConnectorKind,
        config: &JsonValue,
    ) -> Result<(), SchemaValidationError> {
        match self.validators.get(connector_kind) {
            Some(v) => v.validate_action(config),
            None => Ok(()),
        }
    }

    pub fn supported_connectors(&self) -> Vec<ConnectorKind> {
        self.validators.keys().cloned().collect()
    }
}
