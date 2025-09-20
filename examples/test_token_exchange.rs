use openact::authflow::actions::DefaultRouter;
use openact::authflow::engine::TaskHandler;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 测试 GitHub 令牌交换响应格式");
    println!("================================");

    // 检查环境变量
    let client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| "请设置 GITHUB_CLIENT_ID 环境变量")?;
    let client_secret = std::env::var("GITHUB_CLIENT_SECRET")
        .map_err(|_| "请设置 GITHUB_CLIENT_SECRET 环境变量")?;

    println!("✅ 环境变量检查通过");

    // 创建路由器
    let router = DefaultRouter;

    // 模拟令牌交换请求
    let mock_context = json!({
        "tokenUrl": "https://github.com/login/oauth/access_token",
        "clientId": client_id,
        "clientSecret": client_secret,
        "redirectUri": "http://localhost:8080/oauth/callback",
        "code": "mock_auth_code_12345",
        "codeVerifier": "mock_code_verifier"
    });

    println!("🔧 模拟令牌交换请求...");
    println!("📋 请求上下文:");
    println!("{}", serde_json::to_string_pretty(&mock_context)?);

    // 执行令牌交换
    match router.execute("oauth2.exchange_token", "TestExchange", &mock_context) {
        Ok(result) => {
            println!("✅ 令牌交换成功！");
            println!("📋 响应结果:");
            println!("{}", serde_json::to_string_pretty(&result)?);
            
            // 检查响应结构
            if let Some(body) = result.get("body") {
                println!("\n🔍 响应体分析:");
                if let Some(access_token) = body.get("access_token") {
                    println!("   access_token: {:?}", access_token);
                } else {
                    println!("   ❌ 未找到 access_token 字段");
                }
                if let Some(token_type) = body.get("token_type") {
                    println!("   token_type: {:?}", token_type);
                } else {
                    println!("   ❌ 未找到 token_type 字段");
                }
                if let Some(scope) = body.get("scope") {
                    println!("   scope: {:?}", scope);
                } else {
                    println!("   ❌ 未找到 scope 字段");
                }
            } else {
                println!("❌ 响应中没有 body 字段");
            }
        }
        Err(e) => {
            println!("❌ 令牌交换失败: {}", e);
        }
    }

    Ok(())
}
