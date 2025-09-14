// Test example for Action parser
// Demonstrates parsing OpenAPI specifications to extract Action definitions

use manifest::action::{ActionParser, ActionParsingOptions};
use manifest::spec::api_spec::*;
use serde_json::json;
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Testing Action Parser");
    
    // Create a sample OpenAPI specification
    let openapi_spec = create_sample_openapi_spec();
    
    // Create action parser with options
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
    println!("📋 Parsing OpenAPI specification...");
    let result = parser.parse_spec(&openapi_spec)?;
    
    // Display results
    println!("✅ Parsing completed!");
    println!("📊 Statistics:");
    println!("   - Total operations: {}", result.stats.total_operations);
    println!("   - Successful actions: {}", result.stats.successful_actions);
    println!("   - Failed operations: {}", result.stats.failed_operations);
    println!("   - Deprecated skipped: {}", result.stats.deprecated_skipped);
    println!("   - Processing time: {}ms", result.stats.processing_time_ms);
    
    if !result.errors.is_empty() {
        println!("❌ Errors encountered:");
        for error in &result.errors {
            println!("   - {}: {}", error.error_type, error.message);
        }
    }
    
    // Display parsed actions
    println!("\n🎯 Parsed Actions:");
    for (i, action) in result.actions.iter().enumerate() {
        println!("   {}. {}", i + 1, action.name);
        println!("      - Method: {}", action.method);
        println!("      - Path: {}", action.path);
        println!("      - TRN: {}", action.trn);
        println!("      - Provider: {}", action.provider);
        println!("      - Tenant: {}", action.tenant);
        println!("      - Parameters: {}", action.parameters.len());
        println!("      - Tags: {:?}", action.tags);
        
        if let Some(description) = &action.description {
            println!("      - Description: {}", description);
        }
        
        if !action.extensions.is_empty() {
            println!("      - Extensions: {} fields", action.extensions.len());
        }
        
        println!();
    }
    
    // Test action validation
    println!("🔍 Testing Action Validation:");
    for action in &result.actions {
        match action.validate() {
            Ok(_) => println!("   ✅ {} - Valid", action.name),
            Err(e) => println!("   ❌ {} - Invalid: {}", action.name, e),
        }
    }
    
    println!("\n🎉 Action parser test completed successfully!");
    
    Ok(())
}

fn create_sample_openapi_spec() -> OpenApi30Spec {
    OpenApi30Spec {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: "GitHub API".to_string(),
            version: "1.0.0".to_string(),
            description: Some("A sample GitHub API for testing Action parser".to_string()),
            terms_of_service: None,
            contact: None,
            license: None,
            extensions: HashMap::new(),
        },
        external_docs: None,
        servers: vec![
            Server {
                url: "https://api.github.com".to_string(),
                description: Some("GitHub API server".to_string()),
                variables: HashMap::new(),
                extensions: HashMap::new(),
            }
        ],
        security: vec![],
        tags: vec![
            Tag {
                name: "users".to_string(),
                description: Some("User management operations".to_string()),
                external_docs: None,
                extensions: HashMap::new(),
            },
        ],
        paths: Paths {
            paths: {
                let mut paths = HashMap::new();
                
                // GET /users/{username}
                paths.insert("/users/{username}".to_string(), PathItem {
                    reference: None,
                    summary: None,
                    description: None,
                    get: Some(Operation {
                        tags: vec!["users".to_string()],
                        summary: Some("Get user by username".to_string()),
                        description: Some("Retrieve a user by their username".to_string()),
                        external_docs: None,
                        operation_id: Some("getUser".to_string()),
                        parameters: vec![
                            ParameterOrReference::Item(Parameter {
                                name: "username".to_string(),
                                location: "path".to_string(),
                                description: Some("The username of the user to retrieve".to_string()),
                                required: true,
                                deprecated: false,
                                allow_empty_value: false,
                                style: Some("simple".to_string()),
                                explode: None,
                                allow_reserved: false,
                                schema: Some(SchemaOrReference::Item(Schema {
                                    r#type: Some("string".to_string()),
                                    ..Default::default()
                                })),
                                content: HashMap::new(),
                                example: None,
                                examples: HashMap::new(),
                                extensions: HashMap::new(),
                            }),
                        ],
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
                            extensions.insert("x-action-type".to_string(), json!("read"));
                            extensions.insert("x-rate-limit".to_string(), json!(5000));
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
                
                paths
            },
            extensions: HashMap::new(),
        },
        components: None,
        extensions: HashMap::new(),
    }
}