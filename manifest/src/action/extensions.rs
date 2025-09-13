// Extension field handlers for OpenAPI x-* fields
// Provides standardized processing of custom extension fields

use serde::{Serialize, Deserialize};
use serde_json::Value;
use std::collections::HashMap;
use crate::utils::error::{OpenApiToolError, Result};

/// Extension field handler trait
pub trait ExtensionHandler {
    /// Handle an extension field
    fn handle(&self, key: &str, value: &Value) -> Result<ProcessedExtension>;
    
    /// Get the extension field name this handler processes
    fn get_field_name(&self) -> &str;
    
    /// Validate the extension field value
    fn validate(&self, value: &Value) -> Result<()>;
}

/// Processed extension field result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessedExtension {
    /// The processed key
    pub key: String,
    /// The processed value
    pub value: Value,
    /// Additional metadata
    pub metadata: ExtensionMetadata,
}

/// Extension field metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtensionMetadata {
    /// Extension field type
    pub field_type: ExtensionFieldType,
    /// Whether this field is required
    pub required: bool,
    /// Field description
    pub description: Option<String>,
    /// Validation rules
    pub validation_rules: Vec<ValidationRule>,
}

/// Extension field types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExtensionFieldType {
    String,
    Number,
    Boolean,
    Object,
    Array,
    Custom(String),
}

/// Validation rule for extension fields
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValidationRule {
    MinLength(usize),
    MaxLength(usize),
    MinValue(f64),
    MaxValue(f64),
    Pattern(String),
    Enum(Vec<Value>),
    Required,
    Custom(String),
}

/// Extension field processor
pub struct ExtensionProcessor {
    /// Registered handlers
    handlers: HashMap<String, Box<dyn ExtensionHandler>>,
    /// Default handler for unknown extensions
    default_handler: Option<Box<dyn ExtensionHandler>>,
}

impl ExtensionProcessor {
    /// Create a new extension processor
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            default_handler: None,
        }
    }
    
    /// Register an extension handler
    pub fn register_handler(&mut self, handler: Box<dyn ExtensionHandler>) {
        let field_name = handler.get_field_name().to_string();
        self.handlers.insert(field_name, handler);
    }
    
    /// Set default handler for unknown extensions
    pub fn set_default_handler(&mut self, handler: Box<dyn ExtensionHandler>) {
        self.default_handler = Some(handler);
    }
    
    /// Process extension fields
    pub fn process_extensions(&self, extensions: &HashMap<String, Value>) -> Result<Vec<ProcessedExtension>> {
        let mut processed = Vec::new();
        
        for (key, value) in extensions {
            if key.starts_with("x-") {
                match self.process_extension(key, value) {
                    Ok(processed_ext) => processed.push(processed_ext),
                    Err(e) => {
                        // Log warning but continue processing other extensions
                        eprintln!("Warning: Failed to process extension {}: {}", key, e);
                    }
                }
            }
        }
        
        Ok(processed)
    }
    
    /// Process a single extension field
    fn process_extension(&self, key: &str, value: &Value) -> Result<ProcessedExtension> {
        // Try to find a specific handler
        if let Some(handler) = self.handlers.get(key) {
            return handler.handle(key, value);
        }
        
        // Use default handler if available
        if let Some(default_handler) = &self.default_handler {
            return default_handler.handle(key, value);
        }
        
        // Use generic handler as fallback
        self.generic_handle(key, value)
    }
    
    /// Generic handler for unknown extensions
    fn generic_handle(&self, key: &str, value: &Value) -> Result<ProcessedExtension> {
        let field_type = self.infer_field_type(value);
        
        Ok(ProcessedExtension {
            key: key.to_string(),
            value: value.clone(),
            metadata: ExtensionMetadata {
                field_type,
                required: false,
                description: None,
                validation_rules: vec![],
            },
        })
    }
    
    /// Infer field type from value
    fn infer_field_type(&self, value: &Value) -> ExtensionFieldType {
        match value {
            Value::String(_) => ExtensionFieldType::String,
            Value::Number(_) => ExtensionFieldType::Number,
            Value::Bool(_) => ExtensionFieldType::Boolean,
            Value::Object(_) => ExtensionFieldType::Object,
            Value::Array(_) => ExtensionFieldType::Array,
            Value::Null => ExtensionFieldType::String, // Default to string for null
        }
    }
}

impl Default for ExtensionProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// Standard extension handlers

/// x-action-type handler
pub struct ActionTypeHandler;

impl ExtensionHandler for ActionTypeHandler {
    fn handle(&self, key: &str, value: &Value) -> Result<ProcessedExtension> {
        self.validate(value)?;
        
        let _action_type = value.as_str()
            .ok_or_else(|| OpenApiToolError::ValidationError(
                "x-action-type must be a string".to_string()
            ))?;
        
        Ok(ProcessedExtension {
            key: key.to_string(),
            value: value.clone(),
            metadata: ExtensionMetadata {
                field_type: ExtensionFieldType::Custom("enum".to_string()),
                required: false,
                description: Some("Specifies the type of action (read, write, create, update, delete)".to_string()),
                validation_rules: vec![
                    ValidationRule::Enum(vec![
                        Value::String("read".to_string()),
                        Value::String("write".to_string()),
                        Value::String("create".to_string()),
                        Value::String("update".to_string()),
                        Value::String("delete".to_string()),
                    ]),
                ],
            },
        })
    }
    
    fn get_field_name(&self) -> &str {
        "x-action-type"
    }
    
    fn validate(&self, value: &Value) -> Result<()> {
        if let Some(action_type) = value.as_str() {
            match action_type {
                "read" | "write" | "create" | "update" | "delete" => Ok(()),
                _ => Err(OpenApiToolError::ValidationError(
                    format!("Invalid action type: {}. Must be one of: read, write, create, update, delete", action_type)
                )),
            }
        } else {
            Err(OpenApiToolError::ValidationError(
                "x-action-type must be a string".to_string()
            ))
        }
    }
}

/// x-rate-limit handler
pub struct RateLimitHandler;

impl ExtensionHandler for RateLimitHandler {
    fn handle(&self, key: &str, value: &Value) -> Result<ProcessedExtension> {
        self.validate(value)?;
        
        Ok(ProcessedExtension {
            key: key.to_string(),
            value: value.clone(),
            metadata: ExtensionMetadata {
                field_type: ExtensionFieldType::Number,
                required: false,
                description: Some("Rate limit for the action (requests per minute)".to_string()),
                validation_rules: vec![
                    ValidationRule::MinValue(1.0),
                    ValidationRule::MaxValue(1000000.0),
                ],
            },
        })
    }
    
    fn get_field_name(&self) -> &str {
        "x-rate-limit"
    }
    
    fn validate(&self, value: &Value) -> Result<()> {
        if let Some(rate_limit) = value.as_f64() {
            if rate_limit < 1.0 || rate_limit > 1000000.0 {
                return Err(OpenApiToolError::ValidationError(
                    "x-rate-limit must be between 1 and 1000000".to_string()
                ));
            }
            Ok(())
        } else {
            Err(OpenApiToolError::ValidationError(
                "x-rate-limit must be a number".to_string()
            ))
        }
    }
}

/// x-timeout handler
pub struct TimeoutHandler;

impl ExtensionHandler for TimeoutHandler {
    fn handle(&self, key: &str, value: &Value) -> Result<ProcessedExtension> {
        self.validate(value)?;
        
        Ok(ProcessedExtension {
            key: key.to_string(),
            value: value.clone(),
            metadata: ExtensionMetadata {
                field_type: ExtensionFieldType::Number,
                required: false,
                description: Some("Timeout for the action in milliseconds".to_string()),
                validation_rules: vec![
                    ValidationRule::MinValue(100.0),
                    ValidationRule::MaxValue(300000.0), // 5 minutes max
                ],
            },
        })
    }
    
    fn get_field_name(&self) -> &str {
        "x-timeout"
    }
    
    fn validate(&self, value: &Value) -> Result<()> {
        if let Some(timeout) = value.as_f64() {
            if timeout < 100.0 || timeout > 300000.0 {
                return Err(OpenApiToolError::ValidationError(
                    "x-timeout must be between 100 and 300000 milliseconds".to_string()
                ));
            }
            Ok(())
        } else {
            Err(OpenApiToolError::ValidationError(
                "x-timeout must be a number".to_string()
            ))
        }
    }
}

/// x-retry handler
pub struct RetryHandler;

impl ExtensionHandler for RetryHandler {
    fn handle(&self, key: &str, value: &Value) -> Result<ProcessedExtension> {
        self.validate(value)?;
        
        Ok(ProcessedExtension {
            key: key.to_string(),
            value: value.clone(),
            metadata: ExtensionMetadata {
                field_type: ExtensionFieldType::Object,
                required: false,
                description: Some("Retry configuration for the action".to_string()),
                validation_rules: vec![],
            },
        })
    }
    
    fn get_field_name(&self) -> &str {
        "x-retry"
    }
    
    fn validate(&self, value: &Value) -> Result<()> {
        if let Some(obj) = value.as_object() {
            // Validate retry configuration structure
            if let Some(max_attempts) = obj.get("max_attempts") {
                if let Some(attempts) = max_attempts.as_u64() {
                    if attempts < 1 || attempts > 10 {
                        return Err(OpenApiToolError::ValidationError(
                            "x-retry.max_attempts must be between 1 and 10".to_string()
                        ));
                    }
                } else {
                    return Err(OpenApiToolError::ValidationError(
                        "x-retry.max_attempts must be a number".to_string()
                    ));
                }
            }
            
            if let Some(delay) = obj.get("delay") {
                if let Some(delay_ms) = delay.as_f64() {
                    if delay_ms < 100.0 || delay_ms > 60000.0 {
                        return Err(OpenApiToolError::ValidationError(
                            "x-retry.delay must be between 100 and 60000 milliseconds".to_string()
                        ));
                    }
                } else {
                    return Err(OpenApiToolError::ValidationError(
                        "x-retry.delay must be a number".to_string()
                    ));
                }
            }
            
            Ok(())
        } else {
            Err(OpenApiToolError::ValidationError(
                "x-retry must be an object".to_string()
            ))
        }
    }
}

/// x-auth handler
pub struct AuthHandler;

impl ExtensionHandler for AuthHandler {
    fn handle(&self, key: &str, value: &Value) -> Result<ProcessedExtension> {
        self.validate(value)?;
        
        Ok(ProcessedExtension {
            key: key.to_string(),
            value: value.clone(),
            metadata: ExtensionMetadata {
                field_type: ExtensionFieldType::Object,
                required: false,
                description: Some("Authentication configuration for the action".to_string()),
                validation_rules: vec![],
            },
        })
    }
    
    fn get_field_name(&self) -> &str {
        "x-auth"
    }
    
    fn validate(&self, value: &Value) -> Result<()> {
        if value.is_object() {
            // Accept spec-compliant structure; detailed validation will be done during parsing into AuthConfig
            Ok(())
        } else {
            Err(OpenApiToolError::ValidationError(
                "x-auth must be an object".to_string()
            ))
        }
    }
}

/// Extension field registry
pub struct ExtensionRegistry {
    /// Registered processors
    processors: HashMap<String, ExtensionProcessor>,
}

impl ExtensionRegistry {
    /// Create a new extension registry
    pub fn new() -> Self {
        Self {
            processors: HashMap::new(),
        }
    }
    
    /// Register a processor for a specific context
    pub fn register_processor(&mut self, context: String, processor: ExtensionProcessor) {
        self.processors.insert(context, processor);
    }
    
    /// Get processor for a context
    pub fn get_processor(&self, context: &str) -> Option<&ExtensionProcessor> {
        self.processors.get(context)
    }
    
    /// Create default processor with standard handlers
    pub fn create_default_processor() -> ExtensionProcessor {
        let mut processor = ExtensionProcessor::new();
        
        // Register standard handlers
        processor.register_handler(Box::new(ActionTypeHandler));
        processor.register_handler(Box::new(RateLimitHandler));
        processor.register_handler(Box::new(TimeoutHandler));
        processor.register_handler(Box::new(RetryHandler));
        processor.register_handler(Box::new(AuthHandler));
        
        processor
    }
}

impl Default for ExtensionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_action_type_handler() {
        let handler = ActionTypeHandler;
        
        // Valid action type
        let result = handler.handle("x-action-type", &json!("read"));
        assert!(result.is_ok());
        
        // Invalid action type
        let result = handler.handle("x-action-type", &json!("invalid"));
        assert!(result.is_err());
        
        // Non-string value
        let result = handler.handle("x-action-type", &json!(123));
        assert!(result.is_err());
    }

    #[test]
    fn test_rate_limit_handler() {
        let handler = RateLimitHandler;
        
        // Valid rate limit
        let result = handler.handle("x-rate-limit", &json!(1000));
        assert!(result.is_ok());
        
        // Invalid rate limit (too low)
        let result = handler.handle("x-rate-limit", &json!(0));
        assert!(result.is_err());
        
        // Invalid rate limit (too high)
        let result = handler.handle("x-rate-limit", &json!(2000000));
        assert!(result.is_err());
    }

    #[test]
    fn test_extension_processor() {
        let mut processor = ExtensionProcessor::new();
        processor.register_handler(Box::new(ActionTypeHandler));
        
        let mut extensions = HashMap::new();
        extensions.insert("x-action-type".to_string(), json!("read"));
        extensions.insert("x-unknown".to_string(), json!("test"));
        
        let result = processor.process_extensions(&extensions);
        assert!(result.is_ok());
        
        let processed = result.unwrap();
        assert_eq!(processed.len(), 2);
    }

    #[test]
    fn test_extension_registry() {
        let mut registry = ExtensionRegistry::new();
        let processor = ExtensionRegistry::create_default_processor();
        
        registry.register_processor("action".to_string(), processor);
        
        let processor = registry.get_processor("action");
        assert!(processor.is_some());
        
        let processor = registry.get_processor("unknown");
        assert!(processor.is_none());
    }
}
