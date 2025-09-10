// Test example for Golden Playback testing framework
// Demonstrates regression testing with recorded results

use manifest::testing::{GoldenPlayback, GoldenPlaybackConfig, ActionTestRunner};
use manifest::action::AuthAdapter;
use manifest::spec::api_spec::*;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Testing Golden Playback Framework");
    
    // Test basic Golden Playback functionality
    test_basic_golden_playback().await?;
    
    // Test Action parsing with Golden Playback
    test_action_parsing_golden_playback().await?;
    
    // Test Action execution with Golden Playback
    test_action_execution_golden_playback().await?;
    
    // Test authentication flow with Golden Playback
    test_auth_flow_golden_playback().await?;
    
    println!("\n🎉 Golden Playback testing completed successfully!");
    
    Ok(())
}

async fn test_basic_golden_playback() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📋 Testing Basic Golden Playback");
    
    let config = GoldenPlaybackConfig {
        golden_dir: std::path::PathBuf::from("testdata/golden"),
        update_on_mismatch: false,
        ignore_timestamps: true,
        ignore_dynamic_fields: true,
        ignored_fields: vec![
            "timestamp".to_string(),
            "execution_trn".to_string(),
            "access_token".to_string(),
        ],
    };
    
    let golden = GoldenPlayback::new(config);
    
    // Test 1: New test (should create golden file)
    let result1 = golden.run_test("basic_test_new", || async {
        Ok(json!({
            "message": "Hello, World!",
            "timestamp": "2023-01-01T00:00:00Z",
            "count": 42
        }))
    }).await?;
    
    println!("   ✅ New test result: {:?}", result1.status);
    // First run should be New, subsequent runs should be Passed
    assert!(matches!(result1.status, manifest::testing::TestStatus::New | manifest::testing::TestStatus::Passed));
    
    // Test 2: Same test (should pass)
    let result2 = golden.run_test("basic_test_new", || async {
        Ok(json!({
            "message": "Hello, World!",
            "timestamp": "2023-01-02T00:00:00Z", // Different timestamp, but ignored
            "count": 42
        }))
    }).await?;
    
    println!("   ✅ Same test result: {:?}", result2.status);
    assert!(matches!(result2.status, manifest::testing::TestStatus::Passed));
    
    // Test 3: Different test (should fail)
    let result3 = golden.run_test("basic_test_new", || async {
        Ok(json!({
            "message": "Hello, Universe!", // Different message
            "timestamp": "2023-01-01T00:00:00Z",
            "count": 42
        }))
    }).await?;
    
    println!("   ✅ Different test result: {:?}", result3.status);
    assert!(matches!(result3.status, manifest::testing::TestStatus::Failed));
    assert!(!result3.differences.is_empty());
    
    Ok(())
}

async fn test_action_parsing_golden_playback() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🎯 Testing Action Parsing with Golden Playback");
    
    let runner = ActionTestRunner::with_defaults();
    
    // Create a test OpenAPI specification
    let spec = create_test_openapi_spec();
    
    // Test Action parsing
    let result = runner.test_action_parsing("action_parsing_test", &spec).await?;
    
    println!("   ✅ Action parsing test result: {:?}", result.status);
    println!("   📊 Test name: {}", result.test_name);
    println!("   ⏱️  Duration: {}ms", result.metadata.duration_ms);
    
    if let Some(actual) = &result.actual {
        if let Some(actions) = actual.get("actions").and_then(|v| v.as_array()) {
            println!("   📋 Parsed {} actions", actions.len());
            for action in actions {
                if let Some(name) = action.get("name").and_then(|v| v.as_str()) {
                    println!("      - Action: {}", name);
                }
            }
        }
    }
    
    Ok(())
}

async fn test_action_execution_golden_playback() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🏃 Testing Action Execution with Golden Playback");
    
    // Create authentication adapter
    let auth_adapter = Arc::new(AuthAdapter::new("test_tenant".to_string()));
    
    let mut runner = ActionTestRunner::with_defaults();
    runner.set_auth_adapter(auth_adapter);
    
    // Create a test action
    let action = create_test_action();
    
    // Create execution context
    let context = manifest::action::ActionExecutionContext::new(
        "trn:manifest:test_tenant:action/github-getUser".to_string(),
        "trn:manifest:test_tenant:execution/exec123".to_string(),
        "test_tenant".to_string(),
        "github".to_string(),
    );
    
    // Test Action execution
    let result = runner.test_action_execution("action_execution_test", &action, context).await?;
    
    println!("   ✅ Action execution test result: {:?}", result.status);
    println!("   📊 Test name: {}", result.test_name);
    println!("   ⏱️  Duration: {}ms", result.metadata.duration_ms);
    
    if let Some(actual) = &result.actual {
        if let Some(status) = actual.get("status").and_then(|v| v.as_str()) {
            println!("   📋 Execution status: {}", status);
        }
        if let Some(method) = actual.get("method").and_then(|v| v.as_str()) {
            println!("   📋 HTTP method: {}", method);
        }
        if let Some(path) = actual.get("path").and_then(|v| v.as_str()) {
            println!("   📋 API path: {}", path);
        }
    }
    
    Ok(())
}

async fn test_auth_flow_golden_playback() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔐 Testing Authentication Flow with Golden Playback");
    
    let runner = ActionTestRunner::with_defaults();
    
    // Test OAuth2 authentication
    let oauth2_config = manifest::action::AuthConfig {
        auth_type: "oauth2".to_string(),
        provider: "github".to_string(),
        scopes: vec!["user:email".to_string()],
        parameters: HashMap::new(),
    };
    
    let result = runner.test_auth_flow("oauth2_auth_test", &oauth2_config).await?;
    
    println!("   ✅ OAuth2 auth test result: {:?}", result.status);
    println!("   📊 Test name: {}", result.test_name);
    println!("   ⏱️  Duration: {}ms", result.metadata.duration_ms);
    
    if let Some(actual) = &result.actual {
        if let Some(provider) = actual.get("provider").and_then(|v| v.as_str()) {
            println!("   📋 Auth provider: {}", provider);
        }
        if let Some(token_type) = actual.get("token_type").and_then(|v| v.as_str()) {
            println!("   📋 Token type: {}", token_type);
        }
    }
    
    Ok(())
}

fn create_test_openapi_spec() -> OpenApi30Spec {
    OpenApi30Spec {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: "Test API for Golden Playback".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test API for Golden Playback testing".to_string()),
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
                
                // GET /user with authentication
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
                            extensions.insert("x-action-type".to_string(), json!("read"));
                            extensions.insert("x-rate-limit".to_string(), json!(1000));
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
                
                // POST /repos
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
                                "type": "oauth2",
                                "provider": "github",
                                "scopes": ["repo"]
                            }));
                            extensions.insert("x-action-type".to_string(), json!("create"));
                            extensions.insert("x-rate-limit".to_string(), json!(100));
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

fn create_test_action() -> manifest::action::Action {
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
    
    action
}
