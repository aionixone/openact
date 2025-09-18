use openact::engine::{run_until_pause_or_end, RunOutcome};
use openact::actions::DefaultRouter;
use serde_json::json;
use stepflow_dsl::WorkflowDSL;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 GitHub OAuth2 完整流程测试");
    println!("================================");

    // 设置环境变量
    unsafe {
        std::env::set_var("GITHUB_CLIENT_ID", "Ov23lihVkExosE0hR0Bh");
        std::env::set_var("GITHUB_CLIENT_SECRET", "1766570dda50d46701559cc7b86e9d315cb2f23a");
    }

    let router = DefaultRouter;
    
    // 创建 GitHub OAuth2 工作流 DSL
    let dsl = serde_json::from_value::<WorkflowDSL>(json!({
        "version": "1.0",
        "startAt": "Config",
        "states": {
            "Config": {
                "type": "pass",
                "assign": {
                    "config": {
                        "authorizeUrl": "https://github.com/login/oauth/authorize",
                        "tokenUrl": "https://github.com/login/oauth/access_token",
                        "redirectUri": "http://localhost:8080/oauth/callback",
                        "defaultScope": "user:email"
                    },
                    "creds": {
                        "client_id": "{% vars.secrets.github_client_id %}",
                        "client_secret": "{% vars.secrets.github_client_secret %}"
                    }
                },
                "next": "StartAuth"
            },
            "StartAuth": {
                "type": "task",
                "resource": "oauth2.authorize_redirect",
                "parameters": {
                    "authorizeUrl": "{% $config.authorizeUrl %}",
                    "clientId": "{% $creds.client_id %}",
                    "redirectUri": "{% $config.redirectUri %}",
                    "scope": "{% $config.defaultScope %}",
                    "usePKCE": true
                },
                "assign": {
                    "auth_state": "{% result.state %}",
                    "code_verifier": "{% result.code_verifier %}"
                },
                "next": "AwaitCallback"
            },
            "AwaitCallback": {
                "type": "task",
                "resource": "oauth2.await_callback",
                "assign": {
                    "callback_code": "{% result.code %}"
                },
                "next": "ExchangeToken"
            },
            "ExchangeToken": {
                "type": "task",
                "resource": "http.request",
                "parameters": {
                    "method": "POST",
                    "url": "{% $config.tokenUrl %}",
                    "headers": {
                        "Content-Type": "application/x-www-form-urlencoded",
                        "Accept": "application/json"
                    },
                    "body": {
                        "grant_type": "authorization_code",
                        "client_id": "{% $creds.client_id %}",
                        "client_secret": "{% $creds.client_secret %}",
                        "redirect_uri": "{% $config.redirectUri %}",
                        "code": "{% $callback_code %}",
                        "code_verifier": "{% $code_verifier %}"
                    }
                },
                "assign": {
                    "access_token": "{% result.body.access_token %}",
                    "refresh_token": "{% result.body.refresh_token %}",
                    "token_type": "{% result.body.token_type %}",
                    "scope": "{% result.body.scope %}"
                },
                "output": {
                    "access_token": "{% $access_token %}",
                    "refresh_token": "{% $refresh_token ? $refresh_token : null %}",
                    "token_type": "{% $token_type ? $token_type : 'bearer' %}",
                    "scope": "{% $scope ? $scope : '' %}"
                },
                "next": "GetUser"
            },
            "GetUser": {
                "type": "task",
                "resource": "http.request",
                "parameters": {
                    "method": "GET",
                    "url": "https://api.github.com/user",
                    "headers": {
                        "Authorization": "{% 'Bearer ' & $access_token %}",
                        "Accept": "application/vnd.github+json",
                        "User-Agent": "openact/0.1"
                    }
                },
                "assign": {
                    "user_login": "{% result.body.login %}"
                },
                "next": "PersistConnection"
            },
            "PersistConnection": {
                "type": "task",
                "resource": "connection.update",
                "parameters": {
                    "tenant": "test-tenant",
                    "provider": "github",
                    "user_id": "{% $user_login %}",
                    "access_token": "{% $access_token %}",
                    "refresh_token": "{% $refresh_token %}",
                    "token_type": "{% $token_type %}",
                    "scope": "{% $scope %}"
                },
                "end": true
            }
        }
    }))?;

    // 创建初始上下文
    let context = json!({
        "input": {
            "tenant": "test-tenant",
            "redirectUri": "http://localhost:8080/oauth/callback"
        },
        "secrets": {
            "github_client_id": "Ov23lihVkExosE0hR0Bh",
            "github_client_secret": "1766570dda50d46701559cc7b86e9d315cb2f23a"
        }
    });

    println!("📋 步骤 1: 执行到 AwaitCallback 状态...");
    
    // 执行到 AwaitCallback 状态
    match run_until_pause_or_end(&dsl, "Config", context.clone(), &router, 100) {
        Ok(RunOutcome::Pending(pending_info)) => {
            println!("✅ 流程暂停在 AwaitCallback 状态");
            println!("📋 执行 ID: {}", pending_info.run_id);
            println!("📋 下一个状态: {}", pending_info.next_state);
            
            // 获取授权 URL
            if let Some(authorize_url) = pending_info.context.pointer("/states/StartAuth/result/authorize_url") {
                println!("🔗 授权 URL: {}", authorize_url);
            }
            
            // 模拟用户授权，注入授权码
            println!("🔄 步骤 2: 模拟用户授权...");
            let mut continue_context = pending_info.context;
            continue_context["code"] = json!("mock_auth_code_12345");
            continue_context["state"] = continue_context.pointer("/vars/auth_state").cloned().unwrap_or(json!(""));
            
            println!("📋 注入授权码: mock_auth_code_12345");
            
            // 从 AwaitCallback 状态继续执行
            println!("🚀 步骤 3: 从 AwaitCallback 状态继续执行...");
            match run_until_pause_or_end(&dsl, "AwaitCallback", continue_context, &router, 100) {
                Ok(RunOutcome::Finished(final_context)) => {
                    println!("🎉 流程执行完成！");
                    println!("📋 最终状态:");
                    println!("{}", serde_json::to_string_pretty(&final_context)?);
                    
                    // 检查各个状态的结果
                    if let Some(config_result) = final_context.pointer("/states/Config/result") {
                        println!("✅ Config 状态结果: {}", config_result);
                    }
                    
                    if let Some(start_auth_result) = final_context.pointer("/states/StartAuth/result") {
                        println!("✅ StartAuth 状态结果: {}", start_auth_result);
                    }
                    
                    if let Some(await_callback_result) = final_context.pointer("/states/AwaitCallback/result") {
                        println!("✅ AwaitCallback 状态结果: {}", await_callback_result);
                    }
                    
                    if let Some(exchange_token_result) = final_context.pointer("/states/ExchangeToken/result") {
                        println!("✅ ExchangeToken 状态结果: {}", exchange_token_result);
                    }
                    
                    if let Some(get_user_result) = final_context.pointer("/states/GetUser/result") {
                        println!("✅ GetUser 状态结果: {}", get_user_result);
                    }
                    
                    if let Some(persist_connection_result) = final_context.pointer("/states/PersistConnection/result") {
                        println!("✅ PersistConnection 状态结果: {}", persist_connection_result);
                    }
                    
                    println!("🎯 GitHub OAuth2 完整流程测试成功完成！");
                    
                }
                Ok(RunOutcome::Pending(pending_info)) => {
                    println!("⚠️  流程再次暂停: {}", pending_info.next_state);
                }
                Err(e) => {
                    println!("❌ 继续执行失败: {}", e);
                }
            }
        }
        Ok(RunOutcome::Finished(context)) => {
            println!("✅ 流程直接完成（未暂停）");
            println!("📋 结果: {}", serde_json::to_string_pretty(&context)?);
        }
        Err(e) => {
            println!("❌ 执行失败: {}", e);
        }
    }

    Ok(())
}
