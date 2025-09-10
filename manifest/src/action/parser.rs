// Action parser implementation
// Parses OpenAPI specifications to extract Action definitions

use crate::spec::api_spec::*;
use crate::business::trn_generator::ActionTrnGenerator;
use crate::utils::error::{OpenApiToolError, Result};
use super::models::*;
use super::extensions::*;
use super::auth::AuthConfig;
use std::collections::HashMap;

/// Action parser for extracting actions from OpenAPI specifications
pub struct ActionParser {
    /// TRN generator for creating action identifiers
    trn_generator: ActionTrnGenerator,
    /// Parsing options
    options: ActionParsingOptions,
    /// Extension processor
    extension_processor: ExtensionProcessor,
}

impl ActionParser {
    /// Create a new action parser
    pub fn new(options: ActionParsingOptions) -> Self {
        Self {
            trn_generator: ActionTrnGenerator::new(),
            options,
            extension_processor: ExtensionRegistry::create_default_processor(),
        }
    }

    /// Create a new action parser with default options
    pub fn with_defaults() -> Self {
        Self::new(ActionParsingOptions::default())
    }

    /// Parse OpenAPI specification to extract actions
    pub fn parse_spec(&mut self, spec: &OpenApi30Spec) -> Result<ActionParsingResult> {
        let start_time = std::time::Instant::now();
        let mut actions = Vec::new();
        let mut errors = Vec::new();
        let mut total_operations = 0;
        let mut successful_actions = 0;
        let mut failed_operations = 0;
        let mut deprecated_skipped = 0;

        // Iterate through all paths and operations
        for (path, path_item) in &spec.paths.paths {
            // Process each HTTP method
            let operations = self.extract_operations_from_path_item(path, path_item);
            
            for operation_info in operations {
                total_operations += 1;

                // Skip deprecated operations if configured
                if operation_info.deprecated && !self.options.include_deprecated {
                    deprecated_skipped += 1;
                    continue;
                }

                // Parse the operation into an action
                match self.parse_operation(&operation_info, spec) {
                    Ok(action) => {
                        actions.push(action);
                        successful_actions += 1;
                    }
                    Err(e) => {
                        failed_operations += 1;
                        errors.push(ActionParsingError {
                            error_type: ActionParsingErrorType::Other,
                            message: e.to_string(),
                            path: Some(path.clone()),
                            operation_id: operation_info.operation_id.clone(),
                        });
                    }
                }
            }
        }

        let processing_time = start_time.elapsed().as_millis() as u64;

        Ok(ActionParsingResult {
            actions,
            errors,
            stats: ActionParsingStats {
                total_operations,
                successful_actions,
                failed_operations,
                deprecated_skipped,
                processing_time_ms: processing_time,
            },
        })
    }

    /// Extract operations from a path item
    fn extract_operations_from_path_item(&self, path: &str, path_item: &PathItem) -> Vec<OperationInfo> {
        let mut operations = Vec::new();

        // Helper closure to add operation
        let mut add_operation = |method: &str, operation: &Operation| {
            operations.push(OperationInfo {
                method: method.to_string(),
                path: path.to_string(),
                operation: operation.clone(),
                operation_id: operation.operation_id.clone(),
                deprecated: operation.deprecated,
            });
        };

        // Check each HTTP method
        if let Some(op) = &path_item.get {
            add_operation("GET", op);
        }
        if let Some(op) = &path_item.post {
            add_operation("POST", op);
        }
        if let Some(op) = &path_item.put {
            add_operation("PUT", op);
        }
        if let Some(op) = &path_item.delete {
            add_operation("DELETE", op);
        }
        if let Some(op) = &path_item.patch {
            add_operation("PATCH", op);
        }
        if let Some(op) = &path_item.head {
            add_operation("HEAD", op);
        }
        if let Some(op) = &path_item.options {
            add_operation("OPTIONS", op);
        }
        if let Some(op) = &path_item.trace {
            add_operation("TRACE", op);
        }

        operations
    }

    /// Parse a single operation into an action
    fn parse_operation(&mut self, operation_info: &OperationInfo, spec: &OpenApi30Spec) -> Result<Action> {
        let operation = &operation_info.operation;

        // Generate action name
        let action_name = self.generate_action_name(operation_info)?;

        // Generate TRN
        let trn = self.trn_generator.generate_action_trn(
            spec,
            &action_name,
            &self.options.default_provider,
            Some(&self.options.default_tenant),
        )?;

        // Create base action
        let mut action = Action::new(
            action_name,
            operation_info.method.clone(),
            operation_info.path.clone(),
            self.options.default_provider.clone(),
            self.options.default_tenant.clone(),
            trn.trn,
        );

        // Set description
        if let Some(description) = &operation.description {
            action.description = Some(description.clone());
        }

        // Set tags
        for tag in &operation.tags {
            action.add_tag(tag.clone());
        }

        // Parse parameters
        self.parse_parameters(&mut action, operation, spec)?;

        // Parse request body
        self.parse_request_body(&mut action, operation, spec)?;

        // Parse responses
        self.parse_responses(&mut action, operation, spec)?;

        // Parse security requirements
        self.parse_security(&mut action, operation, spec)?;

        // Parse extension fields
        self.parse_extensions(&mut action, operation)?;

        // Parse authentication configuration
        self.parse_auth_config(&mut action, operation)?;

        // Validate the action
        if self.options.validate_schemas {
            action.validate()
                .map_err(|e| OpenApiToolError::ValidationError(e))?;
        }

        Ok(action)
    }

    /// Parse extension fields
    fn parse_extensions(&self, action: &mut Action, operation: &Operation) -> Result<()> {
        // Process extensions using the extension processor
        let processed_extensions = self.extension_processor.process_extensions(&operation.extensions)?;
        
        // Add processed extensions to the action
        for processed_ext in processed_extensions {
            action.set_extension(processed_ext.key, processed_ext.value);
        }

        Ok(())
    }

    /// Parse authentication configuration from x-auth extension
    fn parse_auth_config(&self, action: &mut Action, operation: &Operation) -> Result<()> {
        if let Some(auth_extension) = operation.extensions.get("x-auth") {
            let auth_config = AuthConfig::from_extension(auth_extension)?;
            action.auth_config = Some(auth_config);
        }
        Ok(())
    }

    /// Generate action name from operation
    fn generate_action_name(&self, operation_info: &OperationInfo) -> Result<String> {
        // Use operationId if available
        if let Some(operation_id) = &operation_info.operation.operation_id {
            return Ok(operation_id.clone());
        }

        // Generate from path and method
        let path_segments: Vec<&str> = operation_info.path
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        let mut name_parts = Vec::new();
        name_parts.push(operation_info.method.to_lowercase());

        for segment in path_segments {
            // Remove path parameters (e.g., {id} -> id)
            let clean_segment = segment
                .trim_start_matches('{')
                .trim_end_matches('}')
                .to_lowercase();
            
            name_parts.push(clean_segment);
        }

        Ok(name_parts.join("."))
    }

    /// Parse operation parameters
    fn parse_parameters(
        &self,
        action: &mut Action,
        operation: &Operation,
        spec: &OpenApi30Spec,
    ) -> Result<()> {
        // Process operation-level parameters
        for param_or_ref in &operation.parameters {
            let parameter = self.resolve_parameter(param_or_ref, spec)?;
            let action_param = self.convert_parameter(parameter)?;
            action.add_parameter(action_param);
        }

        Ok(())
    }

    /// Resolve parameter reference
    fn resolve_parameter(
        &self,
        param_or_ref: &ParameterOrReference,
        spec: &OpenApi30Spec,
    ) -> Result<Parameter> {
        match param_or_ref {
            ParameterOrReference::Item(param) => Ok(param.clone()),
            ParameterOrReference::Reference(ref_ref) => {
                // Resolve reference
                let ref_path = &ref_ref.reference;
                if let Some(components) = &spec.components {
                    if let Some(param) = components.parameters.get(ref_path) {
                        match param {
                            ParameterOrReference::Item(param) => Ok(param.clone()),
                            ParameterOrReference::Reference(_) => {
                                Err(OpenApiToolError::ValidationError(
                                    format!("Nested parameter reference not supported: {}", ref_path)
                                ))
                            }
                        }
                    } else {
                        Err(OpenApiToolError::ValidationError(
                            format!("Parameter reference not found: {}", ref_path)
                        ))
                    }
                } else {
                    Err(OpenApiToolError::ValidationError(
                        format!("Components section not found for reference: {}", ref_path)
                    ))
                }
            }
        }
    }

    /// Convert OpenAPI parameter to action parameter
    fn convert_parameter(&self, param: Parameter) -> Result<ActionParameter> {
        let location = match param.location.as_str() {
            "path" => ParameterLocation::Path,
            "query" => ParameterLocation::Query,
            "header" => ParameterLocation::Header,
            "cookie" => ParameterLocation::Cookie,
            _ => {
                return Err(OpenApiToolError::ValidationError(
                    format!("Invalid parameter location: {}", param.location)
                ));
            }
        };

        let mut action_param = ActionParameter::new(param.name, location)
            .description(param.description.unwrap_or_default());

        if param.required {
            action_param = action_param.required();
        }

        if param.deprecated {
            action_param = action_param.deprecated();
        }

        if let Some(schema) = param.schema {
            action_param = action_param.schema(serde_json::to_value(schema)?);
        }

        if let Some(example) = param.example {
            action_param = action_param.example(example);
        }

        Ok(action_param)
    }

    /// Parse request body
    fn parse_request_body(
        &self,
        action: &mut Action,
        operation: &Operation,
        spec: &OpenApi30Spec,
    ) -> Result<()> {
        if let Some(request_body_or_ref) = &operation.request_body {
            let request_body = self.resolve_request_body(request_body_or_ref, spec)?;
            
            let mut content = HashMap::new();
            for (content_type, media_type) in &request_body.content {
                let action_content = ActionContent {
                    schema: media_type.schema.as_ref().map(|s| serde_json::to_value(s).unwrap_or_default()),
                    example: media_type.example.clone(),
                    encoding: if media_type.encoding.is_empty() {
                        None
                    } else {
                        Some(media_type.encoding.iter()
                            .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default()))
                            .collect())
                    },
                };
                content.insert(content_type.clone(), action_content);
            }

            action.request_body = Some(ActionRequestBody {
                description: request_body.description,
                required: request_body.required,
                content,
            });
        }

        Ok(())
    }

    /// Resolve request body reference
    fn resolve_request_body(
        &self,
        request_body_or_ref: &RequestBodyOrReference,
        spec: &OpenApi30Spec,
    ) -> Result<RequestBody> {
        match request_body_or_ref {
            RequestBodyOrReference::Item(request_body) => Ok(request_body.clone()),
            RequestBodyOrReference::Reference(ref_ref) => {
                let ref_path = &ref_ref.reference;
                if let Some(components) = &spec.components {
                    if let Some(request_body) = components.request_bodies.get(ref_path) {
                        match request_body {
                            RequestBodyOrReference::Item(request_body) => Ok(request_body.clone()),
                            RequestBodyOrReference::Reference(_) => {
                                Err(OpenApiToolError::ValidationError(
                                    format!("Nested request body reference not supported: {}", ref_path)
                                ))
                            }
                        }
                    } else {
                        Err(OpenApiToolError::ValidationError(
                            format!("Request body reference not found: {}", ref_path)
                        ))
                    }
                } else {
                    Err(OpenApiToolError::ValidationError(
                        format!("Components section not found for reference: {}", ref_path)
                    ))
                }
            }
        }
    }

    /// Parse responses
    fn parse_responses(
        &self,
        action: &mut Action,
        operation: &Operation,
        spec: &OpenApi30Spec,
    ) -> Result<()> {
        for (status_code, response_or_ref) in &operation.responses.responses {
            let response = self.resolve_response(response_or_ref, spec)?;
            
            let mut content = HashMap::new();
            for (content_type, media_type) in &response.content {
                let action_content = ActionContent {
                    schema: media_type.schema.as_ref().map(|s| serde_json::to_value(s).unwrap_or_default()),
                    example: media_type.example.clone(),
                    encoding: if media_type.encoding.is_empty() {
                        None
                    } else {
                        Some(media_type.encoding.iter()
                            .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default()))
                            .collect())
                    },
                };
                content.insert(content_type.clone(), action_content);
            }

            let action_response = ActionResponse {
                description: response.description.clone(),
                content,
                headers: response.headers.iter()
                    .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default()))
                    .collect(),
            };

            action.add_response(status_code.clone(), action_response);
        }

        // Handle default response
        if let Some(default_response_or_ref) = &operation.responses.default {
            let response = self.resolve_response(default_response_or_ref, spec)?;
            
            let mut content = HashMap::new();
            for (content_type, media_type) in &response.content {
                let action_content = ActionContent {
                    schema: media_type.schema.as_ref().map(|s| serde_json::to_value(s).unwrap_or_default()),
                    example: media_type.example.clone(),
                    encoding: if media_type.encoding.is_empty() {
                        None
                    } else {
                        Some(media_type.encoding.iter()
                            .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default()))
                            .collect())
                    },
                };
                content.insert(content_type.clone(), action_content);
            }

            let action_response = ActionResponse {
                description: response.description.clone(),
                content,
                headers: response.headers.iter()
                    .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default()))
                    .collect(),
            };

            action.add_response("default".to_string(), action_response);
        }

        Ok(())
    }

    /// Resolve response reference
    fn resolve_response(
        &self,
        response_or_ref: &ResponseOrReference,
        spec: &OpenApi30Spec,
    ) -> Result<Response> {
        match response_or_ref {
            ResponseOrReference::Item(response) => Ok(response.clone()),
            ResponseOrReference::Reference(ref_ref) => {
                let ref_path = &ref_ref.reference;
                if let Some(components) = &spec.components {
                    if let Some(response) = components.responses.get(ref_path) {
                        match response {
                            ResponseOrReference::Item(response) => Ok(response.clone()),
                            ResponseOrReference::Reference(_) => {
                                Err(OpenApiToolError::ValidationError(
                                    format!("Nested response reference not supported: {}", ref_path)
                                ))
                            }
                        }
                    } else {
                        Err(OpenApiToolError::ValidationError(
                            format!("Response reference not found: {}", ref_path)
                        ))
                    }
                } else {
                    Err(OpenApiToolError::ValidationError(
                        format!("Components section not found for reference: {}", ref_path)
                    ))
                }
            }
        }
    }

    /// Parse security requirements
    fn parse_security(
        &self,
        action: &mut Action,
        operation: &Operation,
        _spec: &OpenApi30Spec,
    ) -> Result<()> {
        for security_req in &operation.security {
            for (scheme_name, scopes) in &security_req.0 {
                let security = super::models::SecurityRequirement {
                    scheme_name: scheme_name.clone(),
                    scopes: scopes.clone(),
                };
                action.add_security(security);
            }
        }

        Ok(())
    }

}

/// Internal structure for operation information
#[derive(Debug, Clone)]
struct OperationInfo {
    method: String,
    path: String,
    operation: Operation,
    operation_id: Option<String>,
    deprecated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_spec() -> OpenApi30Spec {
        OpenApi30Spec {
            openapi: "3.0.0".to_string(),
            info: Info {
                title: "Test API".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                terms_of_service: None,
                contact: None,
                license: None,
                extensions: HashMap::new(),
            },
            external_docs: None,
            servers: vec![],
            security: vec![],
            tags: vec![],
            paths: Paths {
                paths: {
                    let mut paths = HashMap::new();
                    paths.insert("/users/{id}".to_string(), PathItem {
                        reference: None,
                        summary: None,
                        description: None,
                        get: Some(Operation {
                            tags: vec!["users".to_string()],
                            summary: Some("Get user by ID".to_string()),
                            description: Some("Retrieve a user by their ID".to_string()),
                            external_docs: None,
                            operation_id: Some("getUser".to_string()),
                            parameters: vec![],
                            request_body: None,
                            responses: Responses {
                                default: None,
                                responses: {
                                    let mut responses = HashMap::new();
                                    responses.insert("200".to_string(), ResponseOrReference::Item(Response {
                                        description: "User found".to_string(),
                                        headers: HashMap::new(),
                                        content: HashMap::new(),
                                        links: HashMap::new(),
                                        extensions: HashMap::new(),
                                    }));
                                    responses
                                },
                                extensions: HashMap::new(),
                            },
                            callbacks: HashMap::new(),
                            deprecated: false,
                            security: vec![],
                            servers: vec![],
                            extensions: HashMap::new(),
                        }),
                        put: None,
                        post: None,
                        delete: None,
                        options: None,
                        head: None,
                        patch: None,
                        trace: None,
                        servers: vec![],
                        parameters: vec![],
                        extensions: HashMap::new(),
                    });
                    paths
                },
                extensions: HashMap::new(),
            },
            components: None,
            extensions: HashMap::new(),
        }
    }

    #[test]
    fn test_action_parser_creation() {
        let options = ActionParsingOptions::default();
        let parser = ActionParser::new(options);
        assert_eq!(parser.options.default_provider, "unknown");
        assert_eq!(parser.options.default_tenant, "default");
    }

    #[test]
    fn test_parse_spec() {
        let mut parser = ActionParser::new(ActionParsingOptions {
            default_provider: "test".to_string(),
            default_tenant: "tenant123".to_string(),
            ..Default::default()
        });

        let spec = create_test_spec();
        let result = parser.parse_spec(&spec).unwrap();

        assert_eq!(result.stats.total_operations, 1);
        assert_eq!(result.stats.successful_actions, 1);
        assert_eq!(result.actions.len(), 1);

        let action = &result.actions[0];
        assert_eq!(action.name, "getUser");
        assert_eq!(action.method, "GET");
        assert_eq!(action.path, "/users/{id}");
        assert_eq!(action.provider, "test");
        assert_eq!(action.tenant, "tenant123");
        assert!(action.has_tag("users"));
    }

    #[test]
    fn test_generate_action_name_with_operation_id() {
        let parser = ActionParser::with_defaults();
        let operation_info = OperationInfo {
            method: "GET".to_string(),
            path: "/users/{id}".to_string(),
            operation: Operation {
                tags: vec![],
                summary: None,
                description: None,
                external_docs: None,
                operation_id: Some("getUser".to_string()),
                parameters: vec![],
                request_body: None,
                responses: Responses::new(),
                callbacks: HashMap::new(),
                deprecated: false,
                security: vec![],
                servers: vec![],
                extensions: HashMap::new(),
            },
            operation_id: Some("getUser".to_string()),
            deprecated: false,
        };

        let name = parser.generate_action_name(&operation_info).unwrap();
        assert_eq!(name, "getUser");
    }

    #[test]
    fn test_generate_action_name_without_operation_id() {
        let parser = ActionParser::with_defaults();
        let operation_info = OperationInfo {
            method: "GET".to_string(),
            path: "/users/{id}".to_string(),
            operation: Operation {
                tags: vec![],
                summary: None,
                description: None,
                external_docs: None,
                operation_id: None,
                parameters: vec![],
                request_body: None,
                responses: Responses::new(),
                callbacks: HashMap::new(),
                deprecated: false,
                security: vec![],
                servers: vec![],
                extensions: HashMap::new(),
            },
            operation_id: None,
            deprecated: false,
        };

        let name = parser.generate_action_name(&operation_info).unwrap();
        assert_eq!(name, "get.users.id");
    }
}
