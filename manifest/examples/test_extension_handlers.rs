// Test example for Extension field handlers
// Demonstrates processing of OpenAPI x-* extension fields

use manifest::action::{ActionParser, ActionParsingOptions, ExtensionProcessor, ExtensionRegistry};
use manifest::spec::api_spec::*;
use serde_json::json;
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Testing Extension Field Handlers");
    
    // Test individual extension handlers
    test_extension_handlers();
    
    // Test extension processor
    test_extension_processor();
    
    // Test with Action parser
    let _ = test_action_parser_with_extensions();
    
    println!("\n🎉 Extension field handlers test completed successfully!");
    
    Ok(())
}

fn test_extension_handlers() {
    println!("\n📋 Testing Individual Extension Handlers");
    
    // Test x-action-type handler
    println!("   Testing x-action-type handler...");
    let mut processor = ExtensionProcessor::new();
    processor.register_handler(Box::new(manifest::action::ActionTypeHandler));
    
    let mut extensions = HashMap::new();
    extensions.insert("x-action-type".to_string(), json!("read"));
    extensions.insert("x-action-type-invalid".to_string(), json!("invalid"));
    
    let result = processor.process_extensions(&extensions);
    match result {
        Ok(processed) => {
            println!("   ✅ Processed {} extensions", processed.len());
            for ext in processed {
                println!("      - {}: {:?}", ext.key, ext.value);
            }
        }
        Err(e) => println!("   ❌ Error: {}", e),
    }
    
    // Test x-rate-limit handler
    println!("   Testing x-rate-limit handler...");
    let mut processor = ExtensionProcessor::new();
    processor.register_handler(Box::new(manifest::action::RateLimitHandler));
    
    let mut extensions = HashMap::new();
    extensions.insert("x-rate-limit".to_string(), json!(1000));
    extensions.insert("x-rate-limit-invalid".to_string(), json!(0));
    
    let result = processor.process_extensions(&extensions);
    match result {
        Ok(processed) => {
            println!("   ✅ Processed {} extensions", processed.len());
            for ext in processed {
                println!("      - {}: {:?}", ext.key, ext.value);
            }
        }
        Err(e) => println!("   ❌ Error: {}", e),
    }
}

fn test_extension_processor() {
    println!("\n🔧 Testing Extension Processor");
    
    let processor = ExtensionRegistry::create_default_processor();
    
    let mut extensions = HashMap::new();
    extensions.insert("x-action-type".to_string(), json!("create"));
    extensions.insert("x-rate-limit".to_string(), json!(5000));
    extensions.insert("x-timeout".to_string(), json!(30000));
    extensions.insert("x-retry".to_string(), json!({
        "max_attempts": 3,
        "delay": 1000
    }));
    extensions.insert("x-auth".to_string(), json!({
        "type": "oauth2",
        "scopes": ["read", "write"]
    }));
    extensions.insert("x-unknown".to_string(), json!("test"));
    
    let result = processor.process_extensions(&extensions);
    match result {
        Ok(processed) => {
            println!("   ✅ Processed {} extensions", processed.len());
            for ext in processed {
                println!("      - {}: {} ({:?})", 
                    ext.key, 
                    ext.value, 
                    ext.metadata.field_type
                );
                if let Some(desc) = &ext.metadata.description {
                    println!("        Description: {}", desc);
                }
            }
        }
        Err(e) => println!("   ❌ Error: {}", e),
    }
}

fn test_action_parser_with_extensions() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🎯 Testing Action Parser with Extensions");
    
    // Create a sample OpenAPI specification with extensions
    let openapi_spec = create_openapi_spec_with_extensions();
    
    // Create action parser
    let options = ActionParsingOptions {
        default_provider: "github".to_string(),
        default_tenant: "tenant123".to_string(),
        include_deprecated: false,
        validate_schemas: true,
        extension_handlers: HashMap::new(),
        config_dir: Some("config".to_string()),
        provider_host: Some("api.github.com".to_string()),
    };
    
    let mut parser = ActionParser::new(options);
    
    // Parse the specification
    let result = parser.parse_spec(&openapi_spec)?;
    
    println!("   ✅ Parsed {} actions", result.actions.len());
    
    for action in &result.actions {
        println!("   📋 Action: {}", action.name);
        println!("      - Method: {}", action.method);
        println!("      - Path: {}", action.path);
        println!("      - Extensions: {} fields", action.extensions.len());
        
        for (key, value) in &action.extensions {
            println!("        - {}: {}", key, value);
        }
    }
    
    Ok(())
}

fn create_openapi_spec_with_extensions() -> OpenApi30Spec {
    OpenApi30Spec {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: "API with Extensions".to_string(),
            version: "1.0.0".to_string(),
            description: Some("API with various extension fields".to_string()),
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
                
                // GET /users with extensions
                paths.insert("/users".to_string(), PathItem {
                    reference: None,
                    summary: None,
                    description: None,
                    get: Some(Operation {
                        tags: vec!["users".to_string()],
                        summary: Some("Get users".to_string()),
                        description: Some("Retrieve all users".to_string()),
                        external_docs: None,
                        operation_id: Some("getUsers".to_string()),
                        parameters: vec![],
                        request_body: None,
                        responses: Responses {
                            default: None,
                            responses: {
                                let mut responses = HashMap::new();
                                responses.insert("200".to_string(), ResponseOrReference::Item(Response {
                                    description: "Users found".to_string(),
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
                            extensions.insert("x-action-type".to_string(), json!("read"));
                            extensions.insert("x-rate-limit".to_string(), json!(1000));
                            extensions.insert("x-timeout".to_string(), json!(5000));
                            extensions.insert("x-retry".to_string(), json!({
                                "max_attempts": 3,
                                "delay": 1000
                            }));
                            extensions.insert("x-auth".to_string(), json!({
                                "type": "oauth2",
                                "scopes": ["read"]
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
                
                // POST /users with extensions
                paths.insert("/users".to_string(), PathItem {
                    reference: None,
                    summary: None,
                    description: None,
                    get: None,
                    put: None,
                    post: Some(Operation {
                        tags: vec!["users".to_string()],
                        summary: Some("Create user".to_string()),
                        description: Some("Create a new user".to_string()),
                        external_docs: None,
                        operation_id: Some("createUser".to_string()),
                        parameters: vec![],
                        request_body: None,
                        responses: Responses {
                            default: None,
                            responses: {
                                let mut responses = HashMap::new();
                                responses.insert("201".to_string(), ResponseOrReference::Item(Response {
                                    description: "User created".to_string(),
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
                            extensions.insert("x-action-type".to_string(), json!("create"));
                            extensions.insert("x-rate-limit".to_string(), json!(100));
                            extensions.insert("x-timeout".to_string(), json!(10000));
                            extensions.insert("x-auth".to_string(), json!({
                                "type": "oauth2",
                                "scopes": ["write"]
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
