use openact::authflow::engine::{run_flow, run_until_pause_or_end, RunOutcome};
use openact::authflow::actions::DefaultRouter;
use serde_json::json;
use std::fs;
use stepflow_dsl::dsl::WorkflowDSL;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 读取 GitHub OAuth2 模板
    let template_content = fs::read_to_string("templates/providers/github/oauth2.json")?;
    let template: serde_json::Value = serde_json::from_str(&template_content)?;
    
    // 提取 OAuth 流程
    let oauth_flow = &template["provider"]["flows"]["OAuth"];
    let dsl: WorkflowDSL = serde_json::from_value(oauth_flow.clone())?;
    
    println!("🚀 Starting GitHub OAuth2 flow test...");
    println!("📋 DSL validation: {:?}", dsl.validate());
    
    // 创建路由器
    let router = DefaultRouter;
    
    // 准备输入上下文（secrets + input 覆盖）
    let cid = std::env::var("GITHUB_CLIENT_ID").unwrap_or_default();
    let csec = std::env::var("GITHUB_CLIENT_SECRET").unwrap_or_default();
    if cid.is_empty() || csec.is_empty() {
        eprintln!("Missing env GITHUB_CLIENT_ID / GITHUB_CLIENT_SECRET");
        eprintln!("export GITHUB_CLIENT_ID=...; export GITHUB_CLIENT_SECRET=...");
    }
    let input_context = json!({
        "input": {
            "tenant": "test-tenant",
            "redirectUri": "http://localhost:8080/oauth/callback"
        },
        "global": {},
        "secrets": {
            "github_client_id": cid,
            "github_client_secret": csec
        }
    });
    
    println!("🔧 Input context prepared:");
    println!("{}", serde_json::to_string_pretty(&input_context)?);
    
    // 先只运行 Config 状态，看看 vars 是否正确设置
    println!("🔧 Running Config state only...");
    let ctx_after_config = run_flow(&dsl, &dsl.start_at, input_context.clone(), &router, 1)?;
    println!("🔍 Context after Config:");
    println!("{}", serde_json::to_string_pretty(&ctx_after_config)?);
    
    // 运行直到暂停或结束
    println!("🔧 Running flow until pause or end...");
    let result = run_until_pause_or_end(&dsl, &dsl.start_at, input_context, &router, 100)?;
    
    match result {
        RunOutcome::Pending(pending_info) => {
            println!("✅ Flow paused for callback");
            // 从 context 中提取授权 URL
            if let Some(url) = pending_info.context
                .pointer("/states/StartAuth/result/authorize_url")
                .and_then(|v| v.as_str()) {
                println!("🔗 Authorization URL:\n{}", url);
            } else {
                println!("⚠️  Could not extract authorization URL from context");
            }
            println!("\n🎯 GitHub OAuth2 flow is working correctly!");
            println!("📝 Next steps:");
            println!("   1. Visit the authorization URL above");
            println!("   2. Authorize the GitHub app");
            println!("   3. GitHub will redirect to callback URL with auth code");
            println!("   4. Resume flow with the auth code to complete token exchange");
        }
        RunOutcome::Finished(context) => {
            println!("✅ Flow completed unexpectedly");
            println!("🔍 Final context:");
            println!("{}", serde_json::to_string_pretty(&context)?);
        }
    }
    
    Ok(())
}
