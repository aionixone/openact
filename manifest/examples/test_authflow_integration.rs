// Test example for AuthFlow integration
// Demonstrates authentication context injection for Action execution

use manifest::action::{ActionParser, ActionParsingOptions, ActionRunner, AuthAdapter};
use manifest::spec::api_spec::*;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Testing AuthFlow Integration");
    
    // Test authentication configuration parsing
    test_auth_config_parsing();
    
    // Test authentication adapter
    test_auth_adapter().await?;
    
    // Test Action Parser with authentication
    test_action_parser_with_auth().await?;
    
    // Test Action Runner with authentication
    test_action_runner_with_auth().await?;
    
    println!("\n🎉 AuthFlow integration test completed successfully!");
    
    Ok(())
}

fn test_auth_config_parsing() {
    println!("\n📋 Testing Authentication Configuration Parsing");
    
    // Test OAuth2 configuration
    let oauth2_config = json!({
        "type": "oauth2",
        "provider": "github",
        "scopes": ["user:email", "repo:read"]
    });
    
    let auth_config = manifest::action::AuthConfig::from_extension(&oauth2_config).unwrap();
    assert_eq!(auth_config.auth_type, "oauth2");
    assert_eq!(auth_config.provider, "github");
    assert_eq!(auth_config.scopes, vec!["user:email", "repo:read"]);
    println!("   ✅ OAuth2 config parsed successfully");
    
    // Test API Key configuration
    let api_key_config = json!({
        "type": "api_key",
        "provider": "custom",
        "api_key": "sk-1234567890",
        "header_name": "X-API-Key"
    });
    
    let auth_config = manifest::action::AuthConfig::from_extension(&api_key_config).unwrap();
    assert_eq!(auth_config.auth_type, "api_key");
    assert_eq!(auth_config.provider, "custom");
    assert_eq!(auth_config.parameters.get("api_key").unwrap(), &json!("sk-1234567890"));
    println!("   ✅ API Key config parsed successfully");
    
    // Test Basic Auth configuration
    let basic_config = json!({
        "type": "basic",
        "provider": "custom",
        "username": "user123",
        "password": "pass456"
    });
    
    let auth_config = manifest::action::AuthConfig::from_extension(&basic_config).unwrap();
    assert_eq!(auth_config.auth_type, "basic");
    assert_eq!(auth_config.provider, "custom");
    assert_eq!(auth_config.parameters.get("username").unwrap(), &json!("user123"));
    println!("   ✅ Basic Auth config parsed successfully");
}

async fn test_auth_adapter() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔧 Testing Authentication Adapter");
    
    let adapter = AuthAdapter::new("test_tenant".to_string());
    
    // Test OAuth2 authentication
    let oauth2_config = manifest::action::AuthConfig {
        auth_type: "oauth2".to_string(),
        provider: "github".to_string(),
        scopes: vec!["user:email".to_string()],
        parameters: HashMap::new(),
    };
    
    let auth_context = adapter.get_auth_for_action(&oauth2_config).await?;
    assert_eq!(auth_context.provider, "github");
    assert_eq!(auth_context.token_type, "Bearer");
    assert!(auth_context.access_token.starts_with("ghp_"));
    println!("   ✅ OAuth2 authentication context created");
    
    // Test API Key authentication
    let mut api_key_params = HashMap::new();
    api_key_params.insert("api_key".to_string(), json!("sk-1234567890"));
    api_key_params.insert("header_name".to_string(), json!("X-API-Key"));
    
    let api_key_config = manifest::action::AuthConfig {
        auth_type: "api_key".to_string(),
        provider: "custom".to_string(),
        scopes: vec![],
        parameters: api_key_params,
    };
    
    let auth_context = adapter.get_auth_for_action(&api_key_config).await?;
    assert_eq!(auth_context.provider, "custom");
    assert_eq!(auth_context.token_type, "ApiKey");
    assert_eq!(auth_context.access_token, "sk-1234567890");
    assert_eq!(auth_context.headers.get("X-API-Key").unwrap(), "sk-1234567890");
    println!("   ✅ API Key authentication context created");
    
    Ok(())
}

async fn test_action_parser_with_auth() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🎯 Testing Action Parser with Authentication");
    
    // Create a sample OpenAPI specification with authentication
    let openapi_spec = create_openapi_spec_with_auth();
    
    // Create action parser
    let options = ActionParsingOptions {
        default_provider: "github".to_string(),
        default_tenant: "tenant123".to_string(),
        include_deprecated: false,
        validate_schemas: true,
        extension_handlers: HashMap::new(),
    };
    
    let mut parser = ActionParser::new(options);
    
    // Parse the specification
    let result = parser.parse_spec(&openapi_spec)?;
    
    println!("   ✅ Parsed {} actions", result.actions.len());
    
    for action in &result.actions {
        println!("   📋 Action: {}", action.name);
        println!("      - Method: {}", action.method);
        println!("      - Path: {}", action.path);
        
        if let Some(auth_config) = &action.auth_config {
            println!("      - Auth Type: {}", auth_config.auth_type);
            println!("      - Provider: {}", auth_config.provider);
            println!("      - Scopes: {:?}", auth_config.scopes);
        } else {
            println!("      - No authentication required");
        }
    }
    
    Ok(())
}

async fn test_action_runner_with_auth() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🏃 Testing Action Runner with Authentication");
    
    // Create authentication adapter
    let auth_adapter = Arc::new(AuthAdapter::new("test_tenant".to_string()));
    
    // Create action runner
    let mut runner = ActionRunner::with_tenant("test_tenant".to_string());
    runner.set_auth_adapter(auth_adapter);
    
    // Create a sample action with authentication
    let mut action = manifest::action::Action::new(
        "getUser".to_string(),
        "GET".to_string(),
        "/user".to_string(),
        "github".to_string(),
        "test_tenant".to_string(),
        "trn:manifest:test_tenant:action/github-getUser".to_string(),
    );
    
    // Set authentication configuration
    action.auth_config = Some(manifest::action::AuthConfig {
        auth_type: "oauth2".to_string(),
        provider: "github".to_string(),
        scopes: vec!["user:email".to_string()],
        parameters: HashMap::new(),
    });
    
    // Create execution context
    let context = manifest::action::ActionExecutionContext::new(
        "trn:manifest:test_tenant:action/github-getUser".to_string(),
        "trn:manifest:test_tenant:execution/exec123".to_string(),
        "test_tenant".to_string(),
        "github".to_string(),
    );
    
    // Execute the action
    let result = runner.execute_action(&action, context).await?;
    
    println!("   ✅ Action executed successfully");
    println!("      - Status: {:?}", result.status);
    println!("      - Duration: {}ms", result.duration_ms.unwrap_or(0));
    
    if let Some(response_data) = &result.response_data {
        println!("      - Response: {}", response_data);
    }
    
    Ok(())
}

fn create_openapi_spec_with_auth() -> OpenApi30Spec {
    OpenApi30Spec {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: "API with Authentication".to_string(),
            version: "1.0.0".to_string(),
            description: Some("API with authentication examples".to_string()),
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
                
                // GET /user with OAuth2 authentication
                paths.insert("/user".to_string(), PathItem {
                    reference: None,
                    summary: None,
                    description: None,
                    get: Some(Operation {
                        tags: vec!["user".to_string()],
                        summary: Some("Get current user".to_string()),
                        description: Some("Get the current authenticated user".to_string()),
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
                        extensions: {
                            let mut extensions = HashMap::new();
                            extensions.insert("x-auth".to_string(), json!({
                                "type": "oauth2",
                                "provider": "github",
                                "scopes": ["user:email"]
                            }));
                            extensions
                        },
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
                
                // POST /repos with API Key authentication
                paths.insert("/repos".to_string(), PathItem {
                    reference: None,
                    summary: None,
                    description: None,
                    get: None,
                    put: None,
                    post: Some(Operation {
                        tags: vec!["repos".to_string()],
                        summary: Some("Create repository".to_string()),
                        description: Some("Create a new repository".to_string()),
                        external_docs: None,
                        operation_id: Some("createRepo".to_string()),
                        parameters: vec![],
                        request_body: None,
                        responses: Responses {
                            default: None,
                            responses: {
                                let mut responses = HashMap::new();
                                responses.insert("201".to_string(), ResponseOrReference::Item(Response {
                                    description: "Repository created".to_string(),
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
                        extensions: {
                            let mut extensions = HashMap::new();
                            extensions.insert("x-auth".to_string(), json!({
                                "type": "api_key",
                                "provider": "github",
                                "api_key": "ghp_1234567890",
                                "header_name": "Authorization"
                            }));
                            extensions
                        },
                    }),
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
