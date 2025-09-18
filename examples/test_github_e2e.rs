use openact::engine::{run_until_pause_or_end, RunOutcome};
use openact::actions::DefaultRouter;
use serde_json::json;
use std::fs;
use stepflow_dsl::dsl::WorkflowDSL;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 GitHub OAuth2 端到端测试");
    println!("============================");

    // 检查环境变量
    let client_id = std::env::var("GITHUB_CLIENT_ID")
        .map_err(|_| "请设置 GITHUB_CLIENT_ID 环境变量")?;
    let client_secret = std::env::var("GITHUB_CLIENT_SECRET")
        .map_err(|_| "请设置 GITHUB_CLIENT_SECRET 环境变量")?;

    println!("✅ 环境变量检查通过");
    println!("   Client ID: {}...", &client_id[..8.min(client_id.len())]);

    // 加载 GitHub OAuth2 模板
    let template_content = fs::read_to_string("templates/providers/github/oauth2.json")?;
    let template: serde_json::Value = serde_json::from_str(&template_content)?;
    
    // 提取 OAuth 流程
    let oauth_flow = template["provider"]["flows"]["OAuth"].clone();
    let dsl: WorkflowDSL = serde_json::from_value(oauth_flow)?;
    
    println!("📋 DSL 验证: {:?}", dsl.validate());

    // 准备输入上下文
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

    println!("🔧 输入上下文准备完成");

    // 创建路由器
    let router = DefaultRouter;

    // 运行流程直到暂停或完成
    println!("🔧 运行完整 OAuth2 流程...");
    let result = run_until_pause_or_end(&dsl, &dsl.start_at, input_context, &router, 100)?;

    match result {
        RunOutcome::Pending(pending_info) => {
            println!("✅ 流程暂停，等待用户授权");
            
            // 提取授权 URL
            if let Some(url) = pending_info.context
                .pointer("/states/StartAuth/result/authorize_url")
                .and_then(|v| v.as_str()) {
                println!("🔗 授权 URL:");
                println!("{}", url);
                println!();
                println!("📝 下一步操作:");
                println!("   1. 在浏览器中访问上面的授权 URL");
                println!("   2. 登录 GitHub 并授权应用");
                println!("   3. 从回调 URL 中获取授权码");
                println!("   4. 使用授权码继续流程");
                println!();
                
                // 模拟获取授权码（在实际场景中，这来自用户授权后的回调）
                println!("🔄 模拟用户授权完成...");
                let mock_code = "mock_auth_code_12345";
                println!("🔑 模拟授权码: {}", mock_code);
                
                // 继续执行流程
                println!("🚀 继续执行流程...");
                
                // 更新上下文，添加授权码到顶层
                let mut continue_context = pending_info.context.clone();
                if let Some(obj) = continue_context.as_object_mut() {
                    obj.insert("code".to_string(), json!(mock_code));
                }
                
                // 从 AwaitCallback 状态继续执行
                let final_result = run_until_pause_or_end(&dsl, "AwaitCallback", continue_context, &router, 50)?;
                
                match final_result {
                    RunOutcome::Finished(context) => {
                        println!("🎉 流程执行完成！");
                        println!();
                        println!("📋 最终结果:");
                        
                        // 显示令牌交换结果
                        if let Some(exchange_result) = context
                            .pointer("/states/ExchangeToken/result") {
                            println!("🔑 令牌交换结果:");
                            println!("{}", serde_json::to_string_pretty(exchange_result)?);
                        }
                        
                        // 显示访问令牌信息
                        if let Some(access_token) = context
                            .pointer("/states/ExchangeToken/result/access_token")
                            .and_then(|v| v.as_str()) {
                            println!("🔑 访问令牌: {}...", &access_token[..10.min(access_token.len())]);
                        } else {
                            println!("❌ 未找到访问令牌");
                        }
                        
                        // 显示用户信息
                        if let Some(user_login) = context
                            .pointer("/states/GetUser/result/user_login")
                            .and_then(|v| v.as_str()) {
                            println!("👤 用户登录名: {}", user_login);
                        } else {
                            println!("❌ 未找到用户信息");
                        }
                        
                        // 显示连接持久化结果
                        if let Some(connection_result) = context
                            .pointer("/states/PersistConnection/result") {
                            println!("💾 连接持久化结果:");
                            println!("{}", serde_json::to_string_pretty(connection_result)?);
                        } else {
                            println!("❌ 未找到连接持久化结果");
                        }
                        
                        println!();
                        println!("🎯 GitHub OAuth2 端到端测试完成！");
                        println!("📊 执行状态:");
                        println!("   ✓ 配置初始化");
                        println!("   ✓ 授权 URL 生成");
                        println!("   ✓ 授权码交换 (模拟)");
                        println!("   ⚠️  用户信息获取 (需要真实授权码)");
                        println!("   ⚠️  连接持久化 (需要真实授权码)");
                        
                    }
                    RunOutcome::Pending(_) => {
                        println!("⚠️  流程仍在等待中");
                    }
                }
            } else {
                println!("⚠️  未找到授权 URL");
            }
        }
        RunOutcome::Finished(context) => {
            println!("✅ 流程意外完成");
            println!("📋 最终上下文:");
            println!("{}", serde_json::to_string_pretty(&context)?);
        }
    }

    Ok(())
}
