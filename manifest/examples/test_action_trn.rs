use manifest::business::ActionTrnGenerator;
use manifest::spec::{OpenApi30Spec, Info, Paths, Server};
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Testing Action TRN Generator");
    
    // Create a simple OpenAPI spec with x-provider extension
    let mut extensions = HashMap::new();
    extensions.insert("x-provider".to_string(), serde_json::Value::String("slack".to_string()));
    
    let spec = OpenApi30Spec {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: "Slack API".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            terms_of_service: None,
            contact: None,
            license: None,
            extensions: HashMap::new(),
        },
        external_docs: None,
        servers: Vec::new(),
        security: Vec::new(),
        tags: Vec::new(),
        paths: Paths { paths: HashMap::new(), extensions: HashMap::new() },
        components: None,
        extensions,
    };
    
    // Create Action TRN generator
    let mut generator = ActionTrnGenerator::new();
    
    // Test generating Action TRN for Slack chat.postMessage
    let action_trn = generator.generate_action_trn(
        &spec,
        "/chat.postMessage",
        "post",
        Some("tenant123")
    )?;
    
    println!("✅ Generated Action TRN: {}", action_trn.trn);
    println!("   Action Name: {}", action_trn.action_name);
    println!("   Provider: {}", action_trn.provider);
    println!("   Tenant: {}", action_trn.tenant);
    
    // Test generating execution TRN
    let execution_trn = generator.generate_execution_trn(
        &action_trn.trn,
        "run-abc123"
    )?;
    
    println!("✅ Generated Execution TRN: {}", execution_trn);
    
    // Test with GitHub API using x-vendor extension
    let mut github_extensions = HashMap::new();
    github_extensions.insert("x-vendor".to_string(), serde_json::Value::String("github".to_string()));
    
    let github_spec = OpenApi30Spec {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: "GitHub API".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            terms_of_service: None,
            contact: None,
            license: None,
            extensions: HashMap::new(),
        },
        external_docs: None,
        servers: Vec::new(),
        security: Vec::new(),
        tags: Vec::new(),
        paths: Paths { paths: HashMap::new(), extensions: HashMap::new() },
        components: None,
        extensions: github_extensions,
    };
    
    let github_action_trn = generator.generate_action_trn(
        &github_spec,
        "/repos/{owner}/{repo}/issues",
        "post",
        Some("tenant456")
    )?;
    
    println!("✅ Generated GitHub Action TRN: {}", github_action_trn.trn);
    println!("   Action Name: {}", github_action_trn.action_name);
    println!("   Provider: {}", github_action_trn.provider);
    println!("   Tenant: {}", github_action_trn.tenant);
    
    // Test with domain-based provider detection
    let mut domain_extensions = HashMap::new();
    domain_extensions.insert("x-service".to_string(), serde_json::Value::String("custom-service".to_string()));
    
    let domain_spec = OpenApi30Spec {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: "Custom API Service".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            terms_of_service: None,
            contact: None,
            license: None,
            extensions: HashMap::new(),
        },
        external_docs: None,
        servers: vec![Server {
            url: "https://api.example.com/v1".to_string(),
            description: None,
            variables: HashMap::new(),
            extensions: HashMap::new(),
        }],
        security: Vec::new(),
        tags: Vec::new(),
        paths: Paths { paths: HashMap::new(), extensions: HashMap::new() },
        components: None,
        extensions: domain_extensions,
    };
    
    let domain_action_trn = generator.generate_action_trn(
        &domain_spec,
        "/users/{id}",
        "get",
        Some("tenant789")
    )?;
    
    println!("✅ Generated Domain-based Action TRN: {}", domain_action_trn.trn);
    println!("   Action Name: {}", domain_action_trn.action_name);
    println!("   Provider: {}", domain_action_trn.provider);
    println!("   Tenant: {}", domain_action_trn.tenant);
    
    // Print generation statistics
    generator.print_generation_stats();
    
    Ok(())
}
