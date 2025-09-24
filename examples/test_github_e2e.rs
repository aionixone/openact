use openact::authflow::engine::{run_until_pause_or_end, RunOutcome};
use openact::authflow::actions::DefaultRouter;
use serde_json::json;
use std::fs;
use stepflow_dsl::dsl::WorkflowDSL;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 GitHub OAuth2 End-to-End Test");
    println!("============================");

    // Check environment variables
    let client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| "Please set the GITHUB_CLIENT_ID environment variable")?;
    let client_secret = std::env::var("GITHUB_CLIENT_SECRET")
        .map_err(|_| "Please set the GITHUB_CLIENT_SECRET environment variable")?;

    println!("✅ Environment variables check passed");
    println!("   Client ID: {}...", &client_id[..8.min(client_id.len())]);

    // Load GitHub OAuth2 template
    let template_content = fs::read_to_string("templates/providers/github/oauth2.json")?;
    let template: serde_json::Value = serde_json::from_str(&template_content)?;
    
    // Extract OAuth flow
    let oauth_flow = template["provider"]["flows"]["OAuth"].clone();
    let dsl: WorkflowDSL = serde_json::from_value(oauth_flow)?;
    
    println!("📋 DSL Validation: {:?}", dsl.validate());

    // Prepare input context
    let input_context = json!({
        "input": {
            "tenant": "test-tenant",
            "redirectUri": "http://localhost:8080/oauth/callback"
        },
        "global": {},
        "secrets": {
            "github_client_id": client_id,
            "github_client_secret": client_secret
        }
    });

    println!("🔧 Input context prepared");

    // Create router
    let router = DefaultRouter;

    // Run the flow until pause or completion
    println!("🔧 Running full OAuth2 flow...");
    let result = run_until_pause_or_end(&dsl, &dsl.start_at, input_context, &router, 100)?;

    match result {
        RunOutcome::Pending(pending_info) => {
            println!("✅ Flow paused, waiting for user authorization");
            
            // Extract authorization URL
            if let Some(url) = pending_info.context
                .pointer("/states/StartAuth/result/authorize_url")
                .and_then(|v| v.as_str()) {
                println!("🔗 Authorization URL:");
                println!("{}", url);
                println!();
                println!("📝 Next steps:");
                println!("   1. Visit the authorization URL above in your browser");
                println!("   2. Log in to GitHub and authorize the application");
                println!("   3. Retrieve the authorization code from the callback URL");
                println!("   4. Use the authorization code to continue the flow");
                println!();
                
                // Simulate obtaining the authorization code (in a real scenario, this comes from the user's authorization callback)
                println!("🔄 Simulating user authorization completion...");
                let mock_code = "mock_auth_code_12345";
                println!("🔑 Simulated authorization code: {}", mock_code);
                
                // Continue the flow
                println!("🚀 Continuing the flow...");
                
                // Update context, adding the authorization code to the top level
                let mut continue_context = pending_info.context.clone();
                if let Some(obj) = continue_context.as_object_mut() {
                    obj.insert("code".to_string(), json!(mock_code));
                }
                
                // Continue execution from AwaitCallback state
                let final_result = run_until_pause_or_end(&dsl, "AwaitCallback", continue_context, &router, 50)?;
                
                match final_result {
                    RunOutcome::Finished(context) => {
                        println!("🎉 Flow execution completed!");
                        println!();
                        println!("📋 Final result:");
                        
                        // Display token exchange result
                        if let Some(exchange_result) = context
                            .pointer("/states/ExchangeToken/result") {
                            println!("🔑 Token exchange result:");
                            println!("{}", serde_json::to_string_pretty(exchange_result)?);
                        }
                        
                        // Display access token information
                        if let Some(access_token) = context
                            .pointer("/states/ExchangeToken/result/access_token")
                            .and_then(|v| v.as_str()) {
                            println!("🔑 Access token: {}...", &access_token[..10.min(access_token.len())]);
                        } else {
                            println!("❌ Access token not found");
                        }
                        
                        // Display user information
                        if let Some(user_login) = context
                            .pointer("/states/GetUser/result/user_login")
                            .and_then(|v| v.as_str()) {
                            println!("👤 User login: {}", user_login);
                        } else {
                            println!("❌ User information not found");
                        }
                        
                        // Display connection persistence result
                        if let Some(connection_result) = context
                            .pointer("/states/PersistConnection/result") {
                            println!("💾 Connection persistence result:");
                            println!("{}", serde_json::to_string_pretty(connection_result)?);
                        } else {
                            println!("❌ Connection persistence result not found");
                        }
                        
                        println!();
                        println!("🎯 GitHub OAuth2 End-to-End Test Completed!");
                        println!("📊 Execution status:");
                        println!("   ✓ Configuration initialized");
                        println!("   ✓ Authorization URL generated");
                        println!("   ✓ Authorization code exchange (simulated)");
                        println!("   ⚠️  User information retrieval (requires real authorization code)");
                        println!("   ⚠️  Connection persistence (requires real authorization code)");
                        
                    }
                    RunOutcome::Pending(_) => {
                        println!("⚠️  Flow is still pending");
                    }
                }
            } else {
                println!("⚠️  Authorization URL not found");
            }
        }
        RunOutcome::Finished(context) => {
            println!("✅ Flow unexpectedly completed");
            println!("📋 Final context:");
            println!("{}", serde_json::to_string_pretty(&context)?);
        }
    }

    Ok(())
}
