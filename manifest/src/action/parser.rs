// Action parser implementation
// Parses OpenAPI specifications to extract Action definitions

use super::auth::AuthConfig;
use super::extensions::*;
use super::models::*;
use crate::business::trn_generator::ActionTrnGenerator;
use crate::config::registry::ConfigRegistry;
use crate::spec::api_spec::*;
use crate::utils::error::{OpenApiToolError, Result};
use serde_json::json;
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

        // Load config registry
        let config_dir = self
            .options
            .config_dir
            .clone()
            .unwrap_or_else(|| "config".to_string());
        let registry = match ConfigRegistry::load_from_dir(&config_dir) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "[parser] warn: failed to load config registry from {}: {}",
                    config_dir, e
                );
                ConfigRegistry::empty()
            }
        };

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
                match self.parse_operation_with_registry(&registry, &operation_info, spec) {
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
    fn extract_operations_from_path_item(
        &self,
        path: &str,
        path_item: &PathItem,
    ) -> Vec<OperationInfo> {
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
    fn parse_operation_with_registry(
        &mut self,
        registry: &ConfigRegistry,
        operation_info: &OperationInfo,
        spec: &OpenApi30Spec,
    ) -> Result<Action> {
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

        // Merge provider defaults → action → sidecar into extensions
        let provider_host = if let Some(host) = &self.options.provider_host {
            host.clone()
        } else {
            spec.servers
                .get(0)
                .and_then(|s| url::Url::parse(&s.url).ok())
                .and_then(|u| Some(u.host_str()?.to_string()))
                .unwrap_or_else(|| self.options.default_provider.clone())
        };
        let merged = registry.merged_for(
            &provider_host,
            operation.operation_id.as_deref().unwrap_or(&action.name),
            &json!(action.extensions),
        );
        if let Some(obj) = merged.as_object() {
            action.extensions = obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        }

        // Normalize well-known x-* extensions (types and defaults)
        self.normalize_extensions(&mut action);
        // Fill typed fields from extensions
        self.fill_typed_fields(&mut action);

        // Parse authentication configuration
        if let Some(xauth) = action.extensions.get("x-auth").cloned() {
            let auth_config = AuthConfig::from_extension(&xauth)?;
            action.auth_config = Some(auth_config);
        } else {
            self.parse_auth_config(&mut action, operation)?;
        }

        // Validate the action
        if self.options.validate_schemas {
            action
                .validate()
                .map_err(|e| OpenApiToolError::ValidationError(e))?;
        }

        Ok(action)
    }

    /// Normalize well-known x-* extensions into canonical shapes
    fn normalize_extensions(&self, action: &mut Action) {
        // Merge provider-level auth defaults (scheme/injection/expiry/refresh/failure)
        // into x-auth, with operation-level x-auth taking precedence.
        let mut provider_auth_obj = serde_json::Map::new();
        for key in ["scheme", "injection", "expiry", "refresh", "failure"] {
            if let Some(val) = action.extensions.get(key).cloned() {
                provider_auth_obj.insert(key.to_string(), val);
            }
        }
        if !provider_auth_obj.is_empty() {
            let mut merged_auth = serde_json::Value::Object(provider_auth_obj);
            if let Some(existing_xauth) = action.extensions.get("x-auth").cloned() {
                if existing_xauth.is_object() {
                    // action x-auth overrides provider defaults
                    crate::config::merger::deep_merge(&mut merged_auth, &existing_xauth);
                }
            }
            action
                .extensions
                .insert("x-auth".to_string(), merged_auth);
            // Clean up top-level auth default keys to avoid duplication
            for key in ["scheme", "injection", "expiry", "refresh", "failure"] {
                action.extensions.remove(key);
            }
        }

        // x-timeout-ms: coerce to number
        if let Some(v) = action.extensions.get("x-timeout-ms").cloned() {
            let num = match v {
                serde_json::Value::Number(n) => Some(n),
                serde_json::Value::String(s) => {
                    s.parse::<u64>().ok().map(|u| serde_json::Number::from(u))
                }
                _ => None,
            };
            if let Some(n) = num {
                action
                    .extensions
                    .insert("x-timeout-ms".to_string(), serde_json::Value::Number(n));
            }
        }

        // x-retry: ensure defaults and proper types
        if let Some(mut v) = action.extensions.get("x-retry").cloned() {
            if let Some(obj) = v.as_object_mut() {
                let max_retries = obj.get("max_retries").and_then(|x| x.as_u64()).unwrap_or(3);
                obj.insert(
                    "max_retries".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(max_retries)),
                );
                let base_delay_ms = obj
                    .get("base_delay_ms")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(500);
                obj.insert(
                    "base_delay_ms".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(base_delay_ms)),
                );
                let max_delay_ms = obj
                    .get("max_delay_ms")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(10_000);
                obj.insert(
                    "max_delay_ms".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(max_delay_ms)),
                );
                let respect = obj
                    .get("respect_retry_after")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(true);
                obj.insert(
                    "respect_retry_after".to_string(),
                    serde_json::Value::Bool(respect),
                );
                // retry_on as array of strings
                let retry_on = obj
                    .get("retry_on")
                    .and_then(|x| x.as_array())
                    .cloned()
                    .unwrap_or_else(|| {
                        vec![
                            serde_json::Value::String("5xx".to_string()),
                            serde_json::Value::String("429".to_string()),
                        ]
                    });
                let retry_on_strs: Vec<serde_json::Value> = retry_on
                    .into_iter()
                    .filter_map(|e| match e {
                        serde_json::Value::String(s) => Some(serde_json::Value::String(s)),
                        other => Some(serde_json::Value::String(other.to_string())),
                    })
                    .collect();
                obj.insert(
                    "retry_on".to_string(),
                    serde_json::Value::Array(retry_on_strs),
                );
                action.extensions.insert(
                    "x-retry".to_string(),
                    serde_json::Value::Object(obj.clone()),
                );
            }
        }

        // x-ok-path/x-error-path/x-output-pick: ensure strings
        for key in ["x-ok-path", "x-error-path", "x-output-pick"] {
            if let Some(v) = action.extensions.get(key).cloned() {
                let s = match v {
                    serde_json::Value::String(s) => Some(s),
                    other => Some(other.to_string()),
                };
                if let Some(sv) = s {
                    action
                        .extensions
                        .insert(key.to_string(), serde_json::Value::String(sv));
                }
            }
        }
    }

    fn fill_typed_fields(&self, action: &mut Action) {
        if let Some(v) = action
            .extensions
            .get("x-timeout-ms")
            .and_then(|v| v.as_u64())
        {
            action.timeout_ms = Some(v);
        }
        if let Some(obj) = action.extensions.get("x-retry").and_then(|v| v.as_object()) {
            action.retry = Some(crate::action::models::RetryPolicy {
                max_retries: obj.get("max_retries").and_then(|x| x.as_u64()).unwrap_or(3) as u32,
                base_delay_ms: obj
                    .get("base_delay_ms")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(500),
                max_delay_ms: obj
                    .get("max_delay_ms")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(10_000),
                retry_on: obj
                    .get("retry_on")
                    .and_then(|x| x.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_else(|| vec!["5xx".to_string(), "429".to_string()]),
                respect_retry_after: obj
                    .get("respect_retry_after")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(true),
            });
        }
        action.ok_path = action
            .extensions
            .get("x-ok-path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        action.error_path = action
            .extensions
            .get("x-error-path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        action.output_pick = action
            .extensions
            .get("x-output-pick")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(obj) = action
            .extensions
            .get("x-pagination")
            .and_then(|v| v.as_object())
        {
            action.pagination = Some(crate::action::models::PaginationConfig {
                mode: obj
                    .get("mode")
                    .and_then(|v| v.as_str())
                    .unwrap_or("cursor")
                    .to_string(),
                param: obj
                    .get("param")
                    .and_then(|v| v.as_str())
                    .unwrap_or("page")
                    .to_string(),
                limit: obj.get("limit").and_then(|v| v.as_u64()).unwrap_or(5),
                next_expr: obj
                    .get("next_expr")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                stop_expr: obj
                    .get("stop_expr")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                items_expr: obj
                    .get("items_expr")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                link_expr: obj
                    .get("link_expr")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            });
        }
    }

    /// Parse extension fields
    fn parse_extensions(&self, action: &mut Action, operation: &Operation) -> Result<()> {
        // Process extensions using the extension processor
        let processed_extensions = self
            .extension_processor
            .process_extensions(&operation.extensions)?;

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
        let path_segments: Vec<&str> = operation_info
            .path
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
                                Err(OpenApiToolError::ValidationError(format!(
                                    "Nested parameter reference not supported: {}",
                                    ref_path
                                )))
                            }
                        }
                    } else {
                        Err(OpenApiToolError::ValidationError(format!(
                            "Parameter reference not found: {}",
                            ref_path
                        )))
                    }
                } else {
                    Err(OpenApiToolError::ValidationError(format!(
                        "Components section not found for reference: {}",
                        ref_path
                    )))
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
                return Err(OpenApiToolError::ValidationError(format!(
                    "Invalid parameter location: {}",
                    param.location
                )));
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
                    schema: media_type
                        .schema
                        .as_ref()
                        .map(|s| serde_json::to_value(s).unwrap_or_default()),
                    example: media_type.example.clone(),
                    encoding: if media_type.encoding.is_empty() {
                        None
                    } else {
                        Some(
                            media_type
                                .encoding
                                .iter()
                                .map(|(k, v)| {
                                    (k.clone(), serde_json::to_value(v).unwrap_or_default())
                                })
                                .collect(),
                        )
                    },
                };
                content.insert(content_type.clone(), action_content);
            }

            action.request_body = Some(ActionRequestBody {
                description: request_body.description.clone(),
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
                                Err(OpenApiToolError::ValidationError(format!(
                                    "Nested request body reference not supported: {}",
                                    ref_path
                                )))
                            }
                        }
                    } else {
                        Err(OpenApiToolError::ValidationError(format!(
                            "Request body reference not found: {}",
                            ref_path
                        )))
                    }
                } else {
                    Err(OpenApiToolError::ValidationError(format!(
                        "Components section not found for reference: {}",
                        ref_path
                    )))
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
                    schema: media_type
                        .schema
                        .as_ref()
                        .map(|s| serde_json::to_value(s).unwrap_or_default()),
                    example: media_type.example.clone(),
                    encoding: if media_type.encoding.is_empty() {
                        None
                    } else {
                        Some(
                            media_type
                                .encoding
                                .iter()
                                .map(|(k, v)| {
                                    (k.clone(), serde_json::to_value(v).unwrap_or_default())
                                })
                                .collect(),
                        )
                    },
                };
                content.insert(content_type.clone(), action_content);
            }

            let action_response = ActionResponse {
                description: response.description.clone(),
                content,
                headers: response
                    .headers
                    .iter()
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
                    schema: media_type
                        .schema
                        .as_ref()
                        .map(|s| serde_json::to_value(s).unwrap_or_default()),
                    example: media_type.example.clone(),
                    encoding: if media_type.encoding.is_empty() {
                        None
                    } else {
                        Some(
                            media_type
                                .encoding
                                .iter()
                                .map(|(k, v)| {
                                    (k.clone(), serde_json::to_value(v).unwrap_or_default())
                                })
                                .collect(),
                        )
                    },
                };
                content.insert(content_type.clone(), action_content);
            }

            let action_response = ActionResponse {
                description: response.description.clone(),
                content,
                headers: response
                    .headers
                    .iter()
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
                                Err(OpenApiToolError::ValidationError(format!(
                                    "Nested response reference not supported: {}",
                                    ref_path
                                )))
                            }
                        }
                    } else {
                        Err(OpenApiToolError::ValidationError(format!(
                            "Response reference not found: {}",
                            ref_path
                        )))
                    }
                } else {
                    Err(OpenApiToolError::ValidationError(format!(
                        "Components section not found for reference: {}",
                        ref_path
                    )))
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
                    paths.insert(
                        "/users/{id}".to_string(),
                        PathItem {
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
                                        responses.insert(
                                            "200".to_string(),
                                            ResponseOrReference::Item(Response {
                                                description: "User found".to_string(),
                                                headers: HashMap::new(),
                                                content: HashMap::new(),
                                                links: HashMap::new(),
                                                extensions: HashMap::new(),
                                            }),
                                        );
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
                        },
                    );
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
            validate_schemas: false,
            config_dir: Some("config".to_string()),
            provider_host: Some("api.github.com".to_string()),
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
