use openact::actions::DefaultRouter;
use openact::engine::TaskHandler;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 测试 GitHub 令牌交换 HTTP 请求");
    println!("==================================");

    // 检查环境变量
    let client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| "请设置 GITHUB_CLIENT_ID 环境变量")?;
    let client_secret = std::env::var("GITHUB_CLIENT_SECRET")
        .map_err(|_| "请设置 GITHUB_CLIENT_SECRET 环境变量")?;

    println!("✅ 环境变量检查通过");

    // 创建路由器
    let router = DefaultRouter;

    // 模拟 GitHub 令牌交换 HTTP 请求
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

    println!("🔧 模拟 GitHub 令牌交换 HTTP 请求...");
    println!("📋 请求上下文:");
    println!("{}", serde_json::to_string_pretty(&mock_context)?);

    // 执行 HTTP 请求
    match router.execute("http.request", "TestGitHubToken", &mock_context) {
        Ok(result) => {
            println!("✅ HTTP 请求成功！");
            println!("📋 响应结果:");
            println!("{}", serde_json::to_string_pretty(&result)?);
            
            // 检查响应结构
            if let Some(body) = result.get("body") {
                println!("\n🔍 响应体分析:");
                println!("   body 类型: {:?}", body);
                
                // 检查是否是字符串（表单格式）
                if let Some(body_str) = body.as_str() {
                    println!("   body 是字符串: {}", body_str);
                    println!("   📝 GitHub 返回的是表单格式，需要解析");
                } else if let Some(body_obj) = body.as_object() {
                    println!("   body 是对象:");
                    for (key, value) in body_obj {
                        println!("     {}: {:?}", key, value);
                    }
                } else {
                    println!("   body 是其他类型: {:?}", body);
                }
            } else {
                println!("❌ 响应中没有 body 字段");
            }
        }
        Err(e) => {
            println!("❌ HTTP 请求失败: {}", e);
        }
    }

    Ok(())
}
