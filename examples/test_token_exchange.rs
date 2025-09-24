use openact::authflow::actions::DefaultRouter;
use openact::authflow::engine::TaskHandler;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing GitHub token exchange response format");
    println!("================================");

    // Check environment variables
    let client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| "Please set the GITHUB_CLIENT_ID environment variable")?;
    let client_secret = std::env::var("GITHUB_CLIENT_SECRET")
        .map_err(|_| "Please set the GITHUB_CLIENT_SECRET environment variable")?;

    println!("✅ Environment variables check passed");

    // Create router
    let router = DefaultRouter;

    // Simulate token exchange request
    let mock_context = json!({
        "tokenUrl": "https://github.com/login/oauth/access_token",
        "clientId": client_id,
        "clientSecret": client_secret,
        "redirectUri": "http://localhost:8080/oauth/callback",
        "code": "mock_auth_code_12345",
        "codeVerifier": "mock_code_verifier"
    });

    println!("🔧 Simulating token exchange request...");
    println!("📋 Request context:");
    println!("{}", serde_json::to_string_pretty(&mock_context)?);

    // Execute token exchange
    match router.execute("oauth2.exchange_token", "TestExchange", &mock_context) {
        Ok(result) => {
            println!("✅ Token exchange successful!");
            println!("📋 Response result:");
            println!("{}", serde_json::to_string_pretty(&result)?);
            
            // Check response structure
            if let Some(body) = result.get("body") {
                println!("\n🔍 Analyzing response body:");
                if let Some(access_token) = body.get("access_token") {
                    println!("   access_token: {:?}", access_token);
                } else {
                    println!("   ❌ access_token field not found");
                }
                if let Some(token_type) = body.get("token_type") {
                    println!("   token_type: {:?}", token_type);
                } else {
                    println!("   ❌ token_type field not found");
                }
                if let Some(scope) = body.get("scope") {
                    println!("   scope: {:?}", scope);
                } else {
                    println!("   ❌ scope field not found");
                }
            } else {
                println!("❌ No body field in response");
            }
        }
        Err(e) => {
            println!("❌ Token exchange failed: {}", e);
        }
    }

    Ok(())
}
