// Action data models and structures
// Defines the core Action types used throughout the system

use super::auth::AuthConfig;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub retry_on: Vec<String>,
    pub respect_retry_after: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PaginationConfig {
    pub mode: String,  // cursor | pageToken | link
    pub param: String, // param name for cursor/pageToken
    pub limit: u64,    // max pages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_expr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_expr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items_expr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_expr: Option<String>, // for mode=link, expression producing next URL
}

/// Represents an Action extracted from OpenAPI specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    /// Unique identifier for the action
    pub id: Option<i64>,
    /// TRN (Tenant Resource Name) identifier
    pub trn: String,
    /// Action name (derived from operationId or path+method)
    pub name: String,
    /// Action description
    pub description: Option<String>,
    /// HTTP method (GET, POST, PUT, DELETE, etc.)
    pub method: String,
    /// API path
    pub path: String,
    /// Provider identifier
    pub provider: String,
    /// Tenant identifier
    pub tenant: String,
    /// Whether the action is active
    pub active: bool,
    /// Action parameters
    pub parameters: Vec<ActionParameter>,
    /// Request body schema
    pub request_body: Option<ActionRequestBody>,
    /// Response schemas
    pub responses: HashMap<String, ActionResponse>,
    /// Security requirements
    pub security: Vec<SecurityRequirement>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Extension fields (x-* properties)
    pub extensions: HashMap<String, Value>,
    /// Authentication configuration
    pub auth_config: Option<AuthConfig>,
    /// Typed: timeout override (ms)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Typed: retry policy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    /// Typed: ok/error/output expressions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ok_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_pick: Option<String>,
    /// Typed: pagination config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<PaginationConfig>,
    /// Creation timestamp
    pub created_at: Option<DateTime<Utc>>,
    /// Last update timestamp
    pub updated_at: Option<DateTime<Utc>>,
}

/// Action parameter definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionParameter {
    /// Parameter name
    pub name: String,
    /// Parameter location (path, query, header, cookie)
    pub location: ParameterLocation,
    /// Parameter description
    pub description: Option<String>,
    /// Whether parameter is required
    pub required: bool,
    /// Parameter schema
    pub schema: Option<Value>,
    /// Parameter example value
    pub example: Option<Value>,
    /// Whether parameter is deprecated
    pub deprecated: bool,
}

/// Parameter location enum
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParameterLocation {
    Path,
    Query,
    Header,
    Cookie,
}

impl std::fmt::Display for ParameterLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParameterLocation::Path => write!(f, "path"),
            ParameterLocation::Query => write!(f, "query"),
            ParameterLocation::Header => write!(f, "header"),
            ParameterLocation::Cookie => write!(f, "cookie"),
        }
    }
}

/// Action request body definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionRequestBody {
    /// Request body description
    pub description: Option<String>,
    /// Whether request body is required
    pub required: bool,
    /// Content types and their schemas
    pub content: HashMap<String, ActionContent>,
}

/// Action content definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionContent {
    /// Content schema
    pub schema: Option<Value>,
    /// Content example
    pub example: Option<Value>,
    /// Content encoding information
    pub encoding: Option<HashMap<String, Value>>,
}

/// Action response definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionResponse {
    /// Response description
    pub description: String,
    /// Response content types and schemas
    pub content: HashMap<String, ActionContent>,
    /// Response headers
    pub headers: HashMap<String, Value>,
}

/// Security requirement for action
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecurityRequirement {
    /// Security scheme name
    pub scheme_name: String,
    /// Required scopes (for OAuth2)
    pub scopes: Vec<String>,
}

/// Action execution context
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionExecutionContext {
    /// Action TRN
    pub action_trn: String,
    /// Execution TRN
    pub execution_trn: String,
    /// Input parameters
    pub parameters: HashMap<String, Value>,
    /// Request body
    pub request_body: Option<Value>,
    /// Headers
    pub headers: HashMap<String, String>,
    /// Tenant context
    pub tenant: String,
    /// Provider context
    pub provider: String,
    /// Execution timestamp
    pub timestamp: DateTime<Utc>,
}

/// Action execution result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionExecutionResult {
    /// Execution TRN
    pub execution_trn: String,
    /// Execution status
    pub status: ExecutionStatus,
    /// Response data
    pub response_data: Option<Value>,
    /// Status code
    pub status_code: Option<i32>,
    /// Error message if any
    pub error_message: Option<String>,
    /// Execution duration in milliseconds
    pub duration_ms: Option<u64>,
}

/// Execution status enum
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Pending,
    Running,
    Success,
    Failed,
    Timeout,
    Cancelled,
}

impl std::fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionStatus::Pending => write!(f, "pending"),
            ExecutionStatus::Running => write!(f, "running"),
            ExecutionStatus::Success => write!(f, "success"),
            ExecutionStatus::Failed => write!(f, "failed"),
            ExecutionStatus::Timeout => write!(f, "timeout"),
            ExecutionStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Action parsing options
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionParsingOptions {
    /// Default provider name
    pub default_provider: String,
    /// Default tenant name
    pub default_tenant: String,
    /// Whether to include deprecated actions
    pub include_deprecated: bool,
    /// Whether to validate schemas
    pub validate_schemas: bool,
    /// Custom extension handlers
    pub extension_handlers: HashMap<String, String>,
    /// Config directory for provider defaults (e.g., "config/")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_dir: Option<String>,
    /// Provider host for defaults resolution (e.g., "api.github.com")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_host: Option<String>,
}

impl Default for ActionParsingOptions {
    fn default() -> Self {
        Self {
            default_provider: "unknown".to_string(),
            default_tenant: "default".to_string(),
            include_deprecated: false,
            validate_schemas: true,
            extension_handlers: HashMap::new(),
            config_dir: Some("config".to_string()),
            provider_host: None,
        }
    }
}

/// Action parsing result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionParsingResult {
    /// Successfully parsed actions
    pub actions: Vec<Action>,
    /// Parsing errors
    pub errors: Vec<ActionParsingError>,
    /// Parsing statistics
    pub stats: ActionParsingStats,
}

/// Action parsing error
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionParsingError {
    /// Error type
    pub error_type: ActionParsingErrorType,
    /// Error message
    pub message: String,
    /// Path where error occurred
    pub path: Option<String>,
    /// Operation ID where error occurred
    pub operation_id: Option<String>,
}

/// Action parsing error types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ActionParsingErrorType {
    InvalidOperation,
    MissingOperationId,
    InvalidParameter,
    InvalidSchema,
    InvalidSecurity,
    InvalidExtension,
    ValidationError,
    Other,
}

impl std::fmt::Display for ActionParsingErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionParsingErrorType::InvalidOperation => write!(f, "InvalidOperation"),
            ActionParsingErrorType::MissingOperationId => write!(f, "MissingOperationId"),
            ActionParsingErrorType::InvalidParameter => write!(f, "InvalidParameter"),
            ActionParsingErrorType::InvalidSchema => write!(f, "InvalidSchema"),
            ActionParsingErrorType::InvalidSecurity => write!(f, "InvalidSecurity"),
            ActionParsingErrorType::InvalidExtension => write!(f, "InvalidExtension"),
            ActionParsingErrorType::ValidationError => write!(f, "ValidationError"),
            ActionParsingErrorType::Other => write!(f, "Other"),
        }
    }
}

/// Action parsing statistics
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionParsingStats {
    /// Total operations processed
    pub total_operations: usize,
    /// Successfully parsed actions
    pub successful_actions: usize,
    /// Failed operations
    pub failed_operations: usize,
    /// Deprecated actions skipped
    pub deprecated_skipped: usize,
    /// Processing time in milliseconds
    pub processing_time_ms: u64,
}

impl Action {
    /// Create a new action with basic information
    pub fn new(
        name: String,
        method: String,
        path: String,
        provider: String,
        tenant: String,
        trn: String,
    ) -> Self {
        Self {
            id: None,
            trn,
            name,
            description: None,
            method,
            path,
            provider,
            tenant,
            active: true,
            parameters: Vec::new(),
            request_body: None,
            responses: HashMap::new(),
            security: Vec::new(),
            tags: Vec::new(),
            extensions: HashMap::new(),
            auth_config: None,
            timeout_ms: None,
            retry: None,
            ok_path: None,
            error_path: None,
            output_pick: None,
            pagination: None,
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        }
    }

    /// Add a parameter to the action
    pub fn add_parameter(&mut self, parameter: ActionParameter) {
        self.parameters.push(parameter);
    }

    /// Add a response to the action
    pub fn add_response(&mut self, status_code: String, response: ActionResponse) {
        self.responses.insert(status_code, response);
    }

    /// Add a security requirement to the action
    pub fn add_security(&mut self, security: SecurityRequirement) {
        self.security.push(security);
    }

    /// Add a tag to the action
    pub fn add_tag(&mut self, tag: String) {
        if !self.tags.contains(&tag) {
            self.tags.push(tag);
        }
    }

    /// Set an extension field
    pub fn set_extension(&mut self, key: String, value: Value) {
        self.extensions.insert(key, value);
    }

    /// Get an extension field
    pub fn get_extension(&self, key: &str) -> Option<&Value> {
        self.extensions.get(key)
    }

    /// Check if action has a specific tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(&tag.to_string())
    }

    /// Get path parameters
    pub fn get_path_parameters(&self) -> Vec<&ActionParameter> {
        self.parameters
            .iter()
            .filter(|p| matches!(p.location, ParameterLocation::Path))
            .collect()
    }

    /// Get query parameters
    pub fn get_query_parameters(&self) -> Vec<&ActionParameter> {
        self.parameters
            .iter()
            .filter(|p| matches!(p.location, ParameterLocation::Query))
            .collect()
    }

    /// Get header parameters
    pub fn get_header_parameters(&self) -> Vec<&ActionParameter> {
        self.parameters
            .iter()
            .filter(|p| matches!(p.location, ParameterLocation::Header))
            .collect()
    }

    /// Validate the action
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("Action name cannot be empty".to_string());
        }

        if self.method.trim().is_empty() {
            return Err("Action method cannot be empty".to_string());
        }

        if self.path.trim().is_empty() {
            return Err("Action path cannot be empty".to_string());
        }

        if self.provider.trim().is_empty() {
            return Err("Action provider cannot be empty".to_string());
        }

        if self.tenant.trim().is_empty() {
            return Err("Action tenant cannot be empty".to_string());
        }

        if self.trn.trim().is_empty() {
            return Err("Action TRN cannot be empty".to_string());
        }

        // Validate parameters
        for (i, param) in self.parameters.iter().enumerate() {
            if param.name.trim().is_empty() {
                return Err(format!("Parameter {} name cannot be empty", i));
            }
        }

        // Validate that all path parameters in the path are defined
        let path_params: Vec<String> = self
            .path
            .split('/')
            .filter(|segment| segment.starts_with('{') && segment.ends_with('}'))
            .map(|segment| segment[1..segment.len() - 1].to_string())
            .collect();

        let defined_path_params: Vec<String> = self
            .get_path_parameters()
            .iter()
            .map(|p| p.name.clone())
            .collect();

        for path_param in path_params {
            if !defined_path_params.contains(&path_param) {
                return Err(format!(
                    "Path parameter '{}' is not defined in parameters",
                    path_param
                ));
            }
        }

        Ok(())
    }
}

impl ActionParameter {
    /// Create a new action parameter
    pub fn new(name: String, location: ParameterLocation) -> Self {
        Self {
            name,
            location,
            description: None,
            required: false,
            schema: None,
            example: None,
            deprecated: false,
        }
    }

    /// Set parameter as required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set parameter description
    pub fn description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }

    /// Set parameter schema
    pub fn schema(mut self, schema: Value) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Set parameter example
    pub fn example(mut self, example: Value) -> Self {
        self.example = Some(example);
        self
    }

    /// Set parameter as deprecated
    pub fn deprecated(mut self) -> Self {
        self.deprecated = true;
        self
    }
}

impl ActionExecutionContext {
    /// Create a new execution context
    pub fn new(
        action_trn: String,
        execution_trn: String,
        tenant: String,
        provider: String,
    ) -> Self {
        Self {
            action_trn,
            execution_trn,
            parameters: HashMap::new(),
            request_body: None,
            headers: HashMap::new(),
            tenant,
            provider,
            timestamp: Utc::now(),
        }
    }

    /// Add a parameter to the context
    pub fn add_parameter(&mut self, name: String, value: Value) {
        self.parameters.insert(name, value);
    }

    /// Set request body
    pub fn set_request_body(&mut self, body: Value) {
        self.request_body = Some(body);
    }

    /// Add a header
    pub fn add_header(&mut self, name: String, value: String) {
        self.headers.insert(name, value);
    }
}

impl ActionExecutionResult {
    /// Create a new execution result
    pub fn new(execution_trn: String, status: ExecutionStatus) -> Self {
        Self {
            execution_trn,
            status,
            response_data: None,
            status_code: None,
            error_message: None,
            duration_ms: None,
        }
    }

    /// Set response data
    pub fn set_response_data(mut self, data: Value) -> Self {
        self.response_data = Some(data);
        self.status = ExecutionStatus::Success;
        self
    }

    /// Set status code
    pub fn set_status_code(mut self, code: u16) -> Self {
        self.status_code = Some(i32::from(code));
        self
    }

    /// Set error message
    pub fn set_error_message(mut self, message: String) -> Self {
        self.error_message = Some(message);
        self.status = ExecutionStatus::Failed;
        self
    }

    /// Set duration
    pub fn set_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// Mark result as success
    pub fn mark_success(mut self) -> Self {
        self.status = ExecutionStatus::Success;
        self
    }

    // response header storage removed in this minimal model
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_creation() {
        let action = Action::new(
            "get_user".to_string(),
            "GET".to_string(),
            "/users/{id}".to_string(),
            "example".to_string(),
            "tenant123".to_string(),
            "trn:openact:tenant123:action/get_user:provider/example".to_string(),
        );

        assert_eq!(action.name, "get_user");
        assert_eq!(action.method, "GET");
        assert_eq!(action.path, "/users/{id}");
        assert_eq!(action.provider, "example");
        assert_eq!(action.tenant, "tenant123");
        assert!(action.active);
    }

    #[test]
    fn test_action_validation() {
        let mut action = Action::new(
            "get_user".to_string(),
            "GET".to_string(),
            "/users/{id}".to_string(),
            "example".to_string(),
            "tenant123".to_string(),
            "trn:openact:tenant123:action/get_user:provider/example".to_string(),
        );

        // Add path parameter
        action.add_parameter(
            ActionParameter::new("id".to_string(), ParameterLocation::Path).required(),
        );

        // Should validate successfully
        assert!(action.validate().is_ok());

        // Test missing path parameter
        let mut invalid_action = action.clone();
        invalid_action.parameters.clear();
        assert!(invalid_action.validate().is_err());
    }

    #[test]
    fn test_parameter_creation() {
        let param = ActionParameter::new("id".to_string(), ParameterLocation::Path)
            .required()
            .description("User ID".to_string());

        assert_eq!(param.name, "id");
        assert!(matches!(param.location, ParameterLocation::Path));
        assert!(param.required);
        assert_eq!(param.description, Some("User ID".to_string()));
    }

    #[test]
    fn test_execution_context() {
        let mut context = ActionExecutionContext::new(
            "trn:openact:tenant123:action/get_user:provider/example".to_string(),
            "trn:stepflow:tenant123:execution:action-execution:exec-123".to_string(),
            "tenant123".to_string(),
            "example".to_string(),
        );

        context.add_parameter("id".to_string(), serde_json::json!("123"));
        context.add_header("Authorization".to_string(), "Bearer token".to_string());

        assert_eq!(context.parameters.len(), 1);
        assert_eq!(context.headers.len(), 1);
    }

    #[test]
    fn test_execution_result() {
        let result = ActionExecutionResult::new(
            "trn:stepflow:tenant123:execution:action-execution:exec-123".to_string(),
            ExecutionStatus::Success,
        )
        .set_response_data(serde_json::json!({"id": "123", "name": "John"}))
        .set_status_code(200)
        .set_duration(150);

        assert!(matches!(result.status, ExecutionStatus::Success));
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.duration_ms, Some(150));
    }
}
