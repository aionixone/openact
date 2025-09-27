//! MCP Integration Demo - Showcasing ActionRecord to MCP tool conversion
//! 
//! This demo shows the complete flow from ActionRecord storage to MCP manifest generation.

use crate::http::{
    actions::HttpAction,
    mcp_converter::McpConverter,
};
use openact_core::types::{ActionRecord, McpOverrides, ConnectorKind, Trn};
use serde_json::{json, Value as JsonValue};
use chrono::Utc;
use std::collections::HashMap;

/// Demo scenarios for MCP integration
pub struct McpIntegrationDemo;

impl McpIntegrationDemo {
    /// Scenario 1: Basic GET action without MCP customization
    pub fn demo_basic_get_action() -> (ActionRecord, Option<openact_core::types::McpManifest>) {
        println!("=== Demo 1: Basic GET Action ===");
        
        let action_record = ActionRecord {
            trn: Trn::new("trn:openact:demo:action/http/get-user@v1".to_string()),
            connector: ConnectorKind::new("http"),
            name: "get-user".to_string(),
            connection_trn: Trn::new("trn:openact:demo:connection/http/api@v1".to_string()),
            config_json: json!({
                "method": "GET",
                "path": "/users/{user_id}",
                "headers": {
                    "Accept": "application/json"
                },
                "query_params": {}
            }),
            mcp_enabled: false, // Not exposed as MCP tool
            mcp_overrides: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        };

        println!("Action Record: {:?}", action_record);
        println!("MCP Enabled: {}", action_record.mcp_enabled);
        
        // Since MCP is disabled, no manifest is generated
        let manifest = if action_record.mcp_enabled {
            Self::generate_mcp_manifest(&action_record)
        } else {
            None
        };
        
        println!("MCP Manifest: {:?}\n", manifest);
        (action_record, manifest)
    }

    /// Scenario 2: POST action with MCP enabled and custom overrides
    pub fn demo_post_action_with_mcp() -> (ActionRecord, Option<openact_core::types::McpManifest>) {
        println!("=== Demo 2: POST Action with MCP Enabled ===");
        
        let mcp_overrides = McpOverrides {
            tool_name: Some("create_user_account".to_string()),
            description: Some("Create a new user account in the system with full profile information".to_string()),
            tags: vec!["user".to_string(), "admin".to_string(), "create".to_string()],
            requires_auth: true,
        };

        let action_record = ActionRecord {
            trn: Trn::new("trn:openact:demo:action/http/create-user@v2".to_string()),
            connector: ConnectorKind::new("http"),
            name: "create-user".to_string(),
            connection_trn: Trn::new("trn:openact:demo:connection/http/api@v1".to_string()),
            config_json: json!({
                "method": "POST",
                "path": "/users",
                "headers": {
                    "Content-Type": "application/json",
                    "Accept": "application/json"
                },
                "request_body": {
                    "name": "${name}",
                    "email": "${email}",
                    "role": "${role}"
                }
            }),
            mcp_enabled: true, // Exposed as MCP tool
            mcp_overrides: Some(mcp_overrides),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 2,
        };

        println!("Action Record: {:?}", action_record);
        println!("MCP Enabled: {}", action_record.mcp_enabled);
        println!("MCP Overrides: {:?}", action_record.mcp_overrides);
        
        let manifest = if action_record.mcp_enabled {
            Self::generate_mcp_manifest(&action_record)
        } else {
            None
        };
        
        println!("Generated MCP Manifest: {:#?}\n", manifest);
        (action_record, manifest)
    }

    /// Scenario 3: Complex action with path parameters and security validation
    pub fn demo_complex_action_with_validation() -> (ActionRecord, Option<openact_core::types::McpManifest>) {
        println!("=== Demo 3: Complex Action with Security Validation ===");
        
        let action_record = ActionRecord {
            trn: Trn::new("trn:openact:demo:action/http/update-user-profile@v1".to_string()),
            connector: ConnectorKind::new("http"),
            name: "update-user-profile".to_string(),
            connection_trn: Trn::new("trn:openact:demo:connection/http/api@v1".to_string()),
            config_json: json!({
                "method": "PUT",
                "path": "/users/{user_id}/profile/{profile_id}",
                "headers": {
                    "Content-Type": "application/json",
                    "Authorization": "Bearer ${token}" // This should trigger security validation
                },
                "query_params": {
                    "include": "metadata,preferences"
                },
                "request_body": {
                    "profile_data": "${profile_data}"
                }
            }),
            mcp_enabled: true,
            mcp_overrides: None, // Use default generation
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        };

        println!("Action Record: {:?}", action_record);
        
        // Try to generate manifest and check for security issues
        let manifest = if action_record.mcp_enabled {
            match Self::generate_mcp_manifest_with_validation(&action_record) {
                Ok(m) => m,
                Err(e) => {
                    println!("Security validation failed: {}", e);
                    None
                }
            }
        } else {
            None
        };
        
        println!("Generated MCP Manifest (with validation): {:#?}\n", manifest);
        (action_record, manifest)
    }

    /// Scenario 4: Batch demonstration of multiple actions
    pub fn demo_batch_actions() -> Vec<(ActionRecord, Option<openact_core::types::McpManifest>)> {
        println!("=== Demo 4: Batch Processing Multiple Actions ===");
        
        let actions = vec![
            // Health check action (simple, no params)
            ActionRecord {
                trn: Trn::new("trn:openact:demo:action/http/health-check@v1".to_string()),
                connector: ConnectorKind::new("http"),
                name: "health-check".to_string(),
                connection_trn: Trn::new("trn:openact:demo:connection/http/api@v1".to_string()),
                config_json: json!({"method": "GET", "path": "/health"}),
                mcp_enabled: true,
                mcp_overrides: Some(McpOverrides {
                    tool_name: Some("api_health_check".to_string()),
                    description: Some("Check API service health status".to_string()),
                    tags: vec!["monitoring".to_string(), "health".to_string()],
                    requires_auth: false,
                }),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                version: 1,
            },
            // Delete action (potentially dangerous)
            ActionRecord {
                trn: Trn::new("trn:openact:demo:action/http/delete-user@v1".to_string()),
                connector: ConnectorKind::new("http"),
                name: "delete-user".to_string(),
                connection_trn: Trn::new("trn:openact:demo:connection/http/api@v1".to_string()),
                config_json: json!({
                    "method": "DELETE",
                    "path": "/users/{user_id}",
                    "headers": {"Authorization": "Bearer ${admin_token}"}
                }),
                mcp_enabled: true,
                mcp_overrides: Some(McpOverrides {
                    tool_name: Some("admin_delete_user".to_string()),
                    description: Some("Delete a user account (admin only)".to_string()),
                    tags: vec!["admin".to_string(), "dangerous".to_string()],
                    requires_auth: true,
                }),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                version: 1,
            },
            // Search action with query parameters
            ActionRecord {
                trn: Trn::new("trn:openact:demo:action/http/search-users@v1".to_string()),
                connector: ConnectorKind::new("http"),
                name: "search-users".to_string(),
                connection_trn: Trn::new("trn:openact:demo:connection/http/api@v1".to_string()),
                config_json: json!({
                    "method": "GET",
                    "path": "/users/search",
                    "query_params": {
                        "q": "${query}",
                        "limit": "${limit}",
                        "offset": "${offset}"
                    }
                }),
                mcp_enabled: true,
                mcp_overrides: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                version: 1,
            },
        ];

        let mut results = Vec::new();
        
        for (i, action) in actions.into_iter().enumerate() {
            println!("Processing action {}: {}", i + 1, action.name);
            
            let manifest = if action.mcp_enabled {
                Self::generate_mcp_manifest(&action)
            } else {
                None
            };
            
            println!("  MCP Enabled: {}", action.mcp_enabled);
            if let Some(ref m) = manifest {
                println!("  Tool Name: {}", m.name);
                println!("  Description: {}", m.description.as_ref().unwrap_or(&"None".to_string()));
                if let Some(ref schema) = m.input_schema {
                    if let Some(ref props) = schema.properties {
                        let param_names: Vec<String> = props.keys().map(|k| k.clone()).collect();
                        println!("  Parameters: {}", param_names.join(", "));
                    }
                }
            }
            
            results.push((action, manifest));
            println!();
        }
        
        results
    }

    /// Generate MCP manifest from ActionRecord
    fn generate_mcp_manifest(action_record: &ActionRecord) -> Option<openact_core::types::McpManifest> {
        // Parse the config_json as HttpAction
        match Self::parse_http_action(&action_record.config_json) {
            Ok(http_action) => {
                let manifest = McpConverter::http_action_to_mcp_manifest(
                    &http_action,
                    &action_record.name,
                    action_record.mcp_overrides.as_ref(),
                );
                Some(manifest)
            }
            Err(e) => {
                println!("Failed to parse HTTP action: {}", e);
                None
            }
        }
    }

    /// Generate MCP manifest with security validation
    fn generate_mcp_manifest_with_validation(action_record: &ActionRecord) -> Result<Option<openact_core::types::McpManifest>, String> {
        // Parse the config_json as HttpAction
        let http_action = Self::parse_http_action(&action_record.config_json)
            .map_err(|e| format!("Failed to parse HTTP action: {}", e))?;
        
        // Validate security
        McpConverter::validate_mcp_safety(&http_action)?;
        
        // Generate manifest
        let manifest = McpConverter::http_action_to_mcp_manifest(
            &http_action,
            &action_record.name,
            action_record.mcp_overrides.as_ref(),
        );
        
        Ok(Some(manifest))
    }

    /// Parse JsonValue as HttpAction (simplified)
    fn parse_http_action(config_json: &JsonValue) -> Result<HttpAction, String> {
        let method = config_json.get("method")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'method' field")?
            .to_string();
        
        let path = config_json.get("path")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'path' field")?
            .to_string();
        
        let mut action = HttpAction::new(method, path);
        
        // Add headers if present
        if let Some(headers_obj) = config_json.get("headers").and_then(|v| v.as_object()) {
            let mut headers = HashMap::new();
            for (key, value) in headers_obj {
                if let Some(val_str) = value.as_str() {
                    headers.insert(key.clone(), vec![val_str.to_string()]);
                }
            }
            action.headers = Some(headers);
        }
        
        // Add query params if present
        if let Some(query_obj) = config_json.get("query_params").and_then(|v| v.as_object()) {
            let mut query_params = HashMap::new();
            for (key, value) in query_obj {
                if let Some(val_str) = value.as_str() {
                    query_params.insert(key.clone(), vec![val_str.to_string()]);
                }
            }
            action.query_params = Some(query_params);
        }
        
        // Add request body if present
        if let Some(body) = config_json.get("request_body") {
            action.request_body = Some(body.clone());
        }
        
        Ok(action)
    }

    /// Run all demo scenarios
    pub fn run_all_demos() {
        println!("🚀 MCP Integration Comprehensive Demo\n");
        println!("This demo showcases the complete integration between ActionRecord and MCP manifest generation.\n");
        
        let _demo1 = Self::demo_basic_get_action();
        let _demo2 = Self::demo_post_action_with_mcp();
        let _demo3 = Self::demo_complex_action_with_validation();
        let _demo4 = Self::demo_batch_actions();
        
        println!("✅ All MCP integration demos completed successfully!");
        println!("\nKey Features Demonstrated:");
        println!("- ActionRecord with mcp_enabled flag");
        println!("- McpOverrides for customizing tool names and descriptions");
        println!("- Automatic parameter extraction from HTTP actions");
        println!("- Security validation for sensitive headers");
        println!("- Batch processing of multiple actions");
        println!("- Path parameter detection from URL templates");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_get_action_demo() {
        let (action, manifest) = McpIntegrationDemo::demo_basic_get_action();
        assert!(!action.mcp_enabled);
        assert!(manifest.is_none());
    }

    #[test]
    fn test_post_action_with_mcp_demo() {
        let (action, manifest) = McpIntegrationDemo::demo_post_action_with_mcp();
        assert!(action.mcp_enabled);
        assert!(manifest.is_some());
        
        let manifest = manifest.unwrap();
        assert_eq!(manifest.name, "create_user_account");
        assert!(manifest.description.is_some());
        assert!(manifest.input_schema.is_some());
    }

    #[test]
    fn test_complex_action_validation() {
        let (action, manifest) = McpIntegrationDemo::demo_complex_action_with_validation();
        assert!(action.mcp_enabled);
        // Should fail due to Authorization header
        assert!(manifest.is_none());
    }

    #[test]
    fn test_batch_processing() {
        let results = McpIntegrationDemo::demo_batch_actions();
        assert_eq!(results.len(), 3);
        
        // Health check should succeed
        assert!(results[0].1.is_some());
        
        // Delete action should fail validation due to Authorization header
        // But since demo doesn't use validation, it should succeed
        assert!(results[1].1.is_some());
        
        // Search action should succeed
        assert!(results[2].1.is_some());
    }

    #[test]
    fn test_http_action_parsing() {
        let config = json!({
            "method": "GET",
            "path": "/users/{id}",
            "headers": {"Accept": "application/json"},
            "query_params": {"include": "profile"}
        });
        
        let action = McpIntegrationDemo::parse_http_action(&config).unwrap();
        assert_eq!(action.method, "GET");
        assert_eq!(action.path, "/users/{id}");
        assert!(action.headers.is_some());
        assert!(action.query_params.is_some());
    }
}
