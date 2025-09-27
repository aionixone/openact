//! MCP manifest generation from HTTP action configurations

use crate::http::actions::HttpAction;
use openact_core::types::{McpManifest, McpToolsSchema, ParameterMcpManifest, McpOverrides};
// JsonValue import removed as it's not used
use std::collections::HashMap;

/// Converter for generating MCP manifests from HTTP actions
pub struct McpConverter;

impl McpConverter {
    /// Convert HttpAction to McpManifest with optional overrides
    pub fn http_action_to_mcp_manifest(
        action: &HttpAction,
        action_name: &str,
        overrides: Option<&McpOverrides>,
    ) -> McpManifest {
        // Use override tool name or default to action name
        let tool_name = overrides
            .and_then(|o| o.tool_name.as_ref())
            .unwrap_or(&action_name.to_string())
            .to_string();

        // Generate description from action or use override
        let description = overrides
            .and_then(|o| o.description.as_ref())
            .map(|s| s.clone())
            .unwrap_or_else(|| Self::generate_description(action, action_name));

        // Build input schema from action parameters
        let input_schema = Self::build_input_schema(action);

        McpManifest {
            name: tool_name,
            description: Some(description),
            input_schema: input_schema,
        }
    }

    /// Generate a default description for the HTTP action
    fn generate_description(action: &HttpAction, _action_name: &str) -> String {
        format!(
            "Execute {} request to {} endpoint",
            action.method.to_uppercase(),
            action.path
        )
    }

    /// Build MCP input schema from HTTP action parameters
    fn build_input_schema(action: &HttpAction) -> Option<McpToolsSchema> {
        let mut properties = HashMap::new();
        let mut required = Vec::new();

        // Add path parameters (derived from path template)
        Self::add_path_parameters(action, &mut properties, &mut required);

        // Add query parameters if present
        Self::add_query_parameters(action, &mut properties);

        // Add header parameters if present
        Self::add_header_parameters(action, &mut properties);

        // Add body parameters if request body is expected
        Self::add_body_parameters(action, &mut properties);

        // Only return schema if we have parameters
        if properties.is_empty() {
            None
        } else {
            Some(McpToolsSchema {
                schema_type: "object".to_string(),
                properties: Some(properties),
                required: if required.is_empty() { None } else { Some(required) },
            })
        }
    }

    /// Extract and add path parameters from URL template
    fn add_path_parameters(
        action: &HttpAction,
        properties: &mut HashMap<String, ParameterMcpManifest>,
        required: &mut Vec<String>,
    ) {
        // Simple path parameter detection (looking for {param} patterns)
        let path_params = Self::extract_path_parameters(&action.path);
        
        for param in path_params {
            properties.insert(param.clone(), ParameterMcpManifest {
                param_type: "string".to_string(),
                description: Some(format!("Path parameter: {}", param)),
                items: None,
                additional_properties: None,
            });
            required.push(param);
        }
    }

    /// Add query parameters to schema
    fn add_query_parameters(
        action: &HttpAction,
        properties: &mut HashMap<String, ParameterMcpManifest>,
    ) {
        // For now, we'll add a generic query_params object
        // In a real implementation, this could be more sophisticated
        // based on action configuration or OpenAPI specs
        if action.query_params.is_some() {
            properties.insert("query_params".to_string(), ParameterMcpManifest {
                param_type: "object".to_string(),
                description: Some("Query parameters for the request".to_string()),
                items: None,
                additional_properties: None,
            });
        }
    }

    /// Add header parameters to schema
    fn add_header_parameters(
        action: &HttpAction,
        properties: &mut HashMap<String, ParameterMcpManifest>,
    ) {
        // Add custom headers parameter if the action supports custom headers
        if action.headers.is_some() {
            properties.insert("custom_headers".to_string(), ParameterMcpManifest {
                param_type: "object".to_string(),
                description: Some("Additional headers for the request".to_string()),
                items: None,
                additional_properties: None,
            });
        }
    }

    /// Add body parameters to schema
    fn add_body_parameters(
        action: &HttpAction,
        properties: &mut HashMap<String, ParameterMcpManifest>,
    ) {
        // Add request body parameter for POST/PUT/PATCH methods
        if matches!(action.method.to_uppercase().as_str(), "POST" | "PUT" | "PATCH") {
            properties.insert("request_body".to_string(), ParameterMcpManifest {
                param_type: "object".to_string(),
                description: Some("Request body data".to_string()),
                items: None,
                additional_properties: None,
            });
        }
    }

    /// Extract path parameters from URL template (simple implementation)
    fn extract_path_parameters(path: &str) -> Vec<String> {
        let mut params = Vec::new();
        let mut chars = path.chars().peekable();
        
        while let Some(ch) = chars.next() {
            if ch == '{' {
                let mut param = String::new();
                while let Some(ch) = chars.next() {
                    if ch == '}' {
                        break;
                    }
                    param.push(ch);
                }
                if !param.is_empty() {
                    params.push(param);
                }
            }
        }
        
        params
    }

    /// Generate MCP manifest for a complete HTTP action with metadata
    pub fn generate_tool_manifest(
        action: &HttpAction,
        action_name: &str,
        overrides: Option<&McpOverrides>,
        tags: Option<&[String]>,
    ) -> McpManifest {
        let mut manifest = Self::http_action_to_mcp_manifest(action, action_name, overrides);
        
        // Add tags to description if provided
        if let Some(tags) = tags {
            if !tags.is_empty() {
                let tag_suffix = format!(" (Tags: {})", tags.join(", "));
                manifest.description = Some(
                    manifest.description.unwrap_or_default() + &tag_suffix
                );
            }
        }
        
        manifest
    }

    /// Validate that an action can be safely exposed as MCP tool
    pub fn validate_mcp_safety(action: &HttpAction) -> Result<(), String> {
        // Check for potentially dangerous configurations
        
        // 1. No sensitive headers should be exposed
        if let Some(headers) = &action.headers {
            for (key, _) in headers {
                let key_lower = key.to_lowercase();
                if key_lower.contains("authorization") || 
                   key_lower.contains("token") || 
                   key_lower.contains("secret") || 
                   key_lower.contains("key") {
                    return Err(format!("Sensitive header '{}' should not be exposed in MCP manifest", key));
                }
            }
        }
        
        // 2. Check for dangerous HTTP methods (optional - might be too restrictive)
        match action.method.to_uppercase().as_str() {
            "DELETE" => {
                // Could be dangerous - maybe warn but don't block
            }
            _ => {}
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::actions::HttpAction;
    use std::collections::HashMap;

    #[test]
    fn test_simple_get_action() {
        let action = HttpAction::new("GET".to_string(), "/users/{id}".to_string());
        
        let manifest = McpConverter::http_action_to_mcp_manifest(&action, "get_user", None);
        
        assert_eq!(manifest.name, "get_user");
        assert!(manifest.description.is_some());
        assert!(manifest.input_schema.is_some());
        
        let schema = manifest.input_schema.unwrap();
        assert_eq!(schema.schema_type, "object");
        assert!(schema.properties.is_some());
        
        let properties = schema.properties.unwrap();
        assert!(properties.contains_key("id")); // Path parameter
        assert_eq!(properties["id"].param_type, "string");
    }

    #[test]
    fn test_post_action_with_body() {
        let mut action = HttpAction::new("POST".to_string(), "/users".to_string());
        action.request_body = Some(serde_json::json!({"name": "test"}));
        
        let manifest = McpConverter::http_action_to_mcp_manifest(&action, "create_user", None);
        
        let schema = manifest.input_schema.unwrap();
        let properties = schema.properties.unwrap();
        assert!(properties.contains_key("request_body"));
    }

    #[test]
    fn test_action_with_overrides() {
        let action = HttpAction::new("GET".to_string(), "/users".to_string());
        let overrides = McpOverrides {
            tool_name: Some("list_all_users".to_string()),
            description: Some("Retrieve a list of all users in the system".to_string()),
            tags: vec!["user".to_string(), "admin".to_string()],
            requires_auth: true,
        };
        
        let manifest = McpConverter::http_action_to_mcp_manifest(&action, "get_users", Some(&overrides));
        
        assert_eq!(manifest.name, "list_all_users");
        assert_eq!(manifest.description.unwrap(), "Retrieve a list of all users in the system");
    }

    #[test]
    fn test_path_parameter_extraction() {
        let params = McpConverter::extract_path_parameters("/users/{id}/posts/{post_id}");
        assert_eq!(params, vec!["id", "post_id"]);
        
        let params = McpConverter::extract_path_parameters("/simple/path");
        assert!(params.is_empty());
        
        let params = McpConverter::extract_path_parameters("/users/{id}");
        assert_eq!(params, vec!["id"]);
    }

    #[test]
    fn test_security_validation() {
        let mut action = HttpAction::new("GET".to_string(), "/users".to_string());
        
        // Safe action should pass
        assert!(McpConverter::validate_mcp_safety(&action).is_ok());
        
        // Action with sensitive header should fail
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), vec!["Bearer token".to_string()]);
        action.headers = Some(headers);
        
        assert!(McpConverter::validate_mcp_safety(&action).is_err());
    }

    #[test]
    fn test_action_without_parameters() {
        let action = HttpAction::new("GET".to_string(), "/health".to_string());
        
        let manifest = McpConverter::http_action_to_mcp_manifest(&action, "health_check", None);
        
        // Should have no input schema for parameter-less actions
        assert!(manifest.input_schema.is_none());
    }
}
