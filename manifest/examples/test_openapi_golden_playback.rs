// 实际使用示例：OpenAPI 文档的 Golden Playback 测试
// 演示如何测试 OpenAPI 解析的回归

use manifest::testing::{GoldenPlayback, GoldenPlaybackConfig};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 OpenAPI Golden Playback 测试示例");
    
    // 1. 创建测试配置
    let config = GoldenPlaybackConfig {
        golden_dir: std::path::PathBuf::from("testdata/golden"),
        update_on_mismatch: false,
        ignore_timestamps: true,
        ignore_dynamic_fields: true,
        ignored_fields: vec![
            "timestamp".to_string(),
            "execution_trn".to_string(),
            "created_at".to_string(),
            "updated_at".to_string(),
        ],
    };
    
    let golden = GoldenPlayback::new(config);
    
    // 2. 模拟 OpenAPI 文档解析测试
    test_openapi_parsing(&golden).await?;
    
    // 3. 模拟 Action 执行测试
    test_action_execution(&golden).await?;
    
    println!("\n🎉 测试完成！");
    println!("📁 查看生成的 Golden 文件：ls testdata/golden/");
    
    Ok(())
}

async fn test_openapi_parsing(golden: &GoldenPlayback) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n📋 测试 OpenAPI 文档解析");
    
    // 模拟解析 GitHub API 的 OpenAPI 文档
    let result = golden.run_test("github_api_parsing", || async {
        // 这里应该是真实的 OpenAPI 解析逻辑
        let mock_parsing_result = json!({
            "actions": [
                {
                    "name": "getUser",
                    "method": "GET",
                    "path": "/user",
                    "provider": "github",
                    "auth_config": {
                        "auth_type": "oauth2",
                        "provider": "github",
                        "scopes": ["user:email"]
                    },
                    "extensions": {
                        "x-action-type": "read",
                        "x-rate-limit": 5000
                    }
                },
                {
                    "name": "createRepo",
                    "method": "POST", 
                    "path": "/user/repos",
                    "provider": "github",
                    "auth_config": {
                        "auth_type": "oauth2",
                        "provider": "github",
                        "scopes": ["repo"]
                    },
                    "extensions": {
                        "x-action-type": "create",
                        "x-rate-limit": 100
                    }
                }
            ],
            "stats": {
                "total_operations": 2,
                "successful_actions": 2,
                "failed_operations": 0
            }
        });
        
        Ok(mock_parsing_result)
    }).await?;
    
    println!("   📊 测试结果: {:?}", result.status);
    println!("   📁 Golden 文件: testdata/golden/github_api_parsing.json");
    
    match result.status {
        manifest::testing::TestStatus::New => {
            println!("   ✅ 首次运行，已创建 Golden 文件");
        }
        manifest::testing::TestStatus::Passed => {
            println!("   ✅ 回归测试通过，结果与 Golden 文件一致");
        }
        manifest::testing::TestStatus::Failed => {
            println!("   ❌ 回归测试失败，发现差异：");
            for diff in &result.differences {
                println!("      - {}: {:?} -> {:?}", diff.path, diff.expected, diff.actual);
            }
        }
        manifest::testing::TestStatus::Updated => {
            println!("   🔄 Golden 文件已更新");
        }
    }
    
    Ok(())
}

async fn test_action_execution(golden: &GoldenPlayback) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🏃 测试 Action 执行");
    
    // 模拟执行 GitHub API 调用
    let result = golden.run_test("github_api_execution", || async {
        // 这里应该是真实的 API 调用逻辑
        let mock_execution_result = json!({
            "execution_trn": "trn:manifest:tenant123:execution/exec_456",
            "status": "Success",
            "response_data": {
                "id": 12345,
                "login": "testuser",
                "name": "Test User",
                "email": "test@example.com"
            },
            "status_code": 200,
            "duration_ms": 150,
            "auth_info": {
                "provider": "github",
                "token_type": "Bearer",
                "scopes": ["user:email"]
            }
        });
        
        Ok(mock_execution_result)
    }).await?;
    
    println!("   📊 测试结果: {:?}", result.status);
    println!("   📁 Golden 文件: testdata/golden/github_api_execution.json");
    
    match result.status {
        manifest::testing::TestStatus::New => {
            println!("   ✅ 首次运行，已创建 Golden 文件");
        }
        manifest::testing::TestStatus::Passed => {
            println!("   ✅ 回归测试通过，API 响应格式一致");
        }
        manifest::testing::TestStatus::Failed => {
            println!("   ❌ 回归测试失败，API 响应格式发生变化：");
            for diff in &result.differences {
                println!("      - {}: {:?} -> {:?}", diff.path, diff.expected, diff.actual);
            }
        }
        manifest::testing::TestStatus::Updated => {
            println!("   🔄 Golden 文件已更新");
        }
    }
    
    Ok(())
}
