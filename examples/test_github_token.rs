use openact::authflow::actions::DefaultRouter;
use openact::authflow::engine::TaskHandler;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing GitHub token exchange HTTP request");
    println!("==================================");

    // Check environment variables
    let client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| "Please set the GITHUB_CLIENT_ID environment variable")?;
    let client_secret = std::env::var("GITHUB_CLIENT_SECRET")
        .map_err(|_| "Please set the GITHUB_CLIENT_SECRET environment variable")?;

    println!("✅ Environment variables check passed");

    // Create router
    let router = DefaultRouter;

    // Simulate GitHub token exchange HTTP request
    let mock_context = json!({
        "method": "POST",
        "url": "https://github.com/login/oauth/access_token",
        "headers": {
            "Content-Type": "application/x-www-form-urlencoded",
            "Accept": "application/json"
        },
        "body": {
            "grant_type": "authorization_code",
            "client_id": client_id,
            "client_secret": client_secret,
            "redirect_uri": "http://localhost:8080/oauth/callback",
            "code": "mock_auth_code_12345",
            "code_verifier": "mock_code_verifier"
        }
    });

    println!("🔧 Simulating GitHub token exchange HTTP request...");
    println!("📋 Request context:");
    println!("{}", serde_json::to_string_pretty(&mock_context)?);

    // Execute HTTP request
    match router.execute("http.request", "TestGitHubToken", &mock_context) {
        Ok(result) => {
            println!("✅ HTTP request successful!");
            println!("📋 Response result:");
            println!("{}", serde_json::to_string_pretty(&result)?);
            
            // Check response structure
            if let Some(body) = result.get("body") {
                println!("\n🔍 Response body analysis:");
                println!("   body type: {:?}", body);
                
                // Check if it is a string (form format)
                if let Some(body_str) = body.as_str() {
                    println!("   body is a string: {}", body_str);
                    println!("   📝 GitHub returned a form format, needs parsing");
                } else if let Some(body_obj) = body.as_object() {
                    println!("   body is an object:");
                    for (key, value) in body_obj {
                        println!("     {}: {:?}", key, value);
                    }
                } else {
                    println!("   body is another type: {:?}", body);
                }
            } else {
                println!("❌ No body field in response");
            }
        }
        Err(e) => {
            println!("❌ HTTP request failed: {}", e);
        }
    }

    Ok(())
}
