use manifest::storage::*;
use manifest::business::*;
use manifest::spec::*;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 测试 OpenAct 持久化层");
    
    // 1. 初始化数据库
    let database_url = "sqlite:./data/openact.db";
    let db = ActionDatabase::new(database_url).await?;
    println!("✅ 数据库连接成功");
    
    // 2. 创建 Repository
    let action_repo = ActionRepository::new(db.pool.clone());
    let execution_repo = ExecutionRepository::new(db.pool.clone());
    println!("✅ Repository 创建成功");
    
    // 3. 测试 TRN 生成
    let mut generator = ActionTrnGenerator::new();
    
    // 创建测试用的 OpenAPI 规范
    let openapi_spec = OpenApi30Spec {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: "Test API".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test API for OpenAct".to_string()),
            contact: None,
            license: None,
            terms_of_service: None,
            extensions: HashMap::new(),
        },
        servers: vec![
            Server {
                url: "https://api.example.com".to_string(),
                description: Some("Production server".to_string()),
                variables: HashMap::new(),
                extensions: HashMap::new(),
            }
        ],
        paths: Paths {
            paths: HashMap::new(),
            extensions: HashMap::new(),
        },
        components: None,
        security: vec![],
        tags: vec![],
        external_docs: None,
        extensions: HashMap::new(),
    };
    
    // 生成 Action TRN
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    let action_trn_result = generator.generate_action_trn(
        &openapi_spec,
        &format!("/users/{{id}}_{}", timestamp),
        "GET",
        Some("tenant123")
    )?;
    
    println!("✅ 生成 Action TRN: {}", action_trn_result.trn);
    println!("   - Action 名称: {}", action_trn_result.action_name);
    println!("   - 提供商: {}", action_trn_result.provider);
    println!("   - 租户: {}", action_trn_result.tenant);
    
    // 4. 测试创建 Action
    let create_request = CreateActionRequest {
        trn: action_trn_result.trn.clone(),
        tenant: "tenant123".to_string(),
        name: action_trn_result.action_name.clone(),
        provider: action_trn_result.provider.clone(),
        openapi_spec: serde_json::to_string(&openapi_spec)?,
        extensions: Some(r#"{"x-test": "value"}"#.to_string()),
        auth_flow: None,
        metadata: Some(r#"{"description": "Test action"}"#.to_string()),
        is_active: true,
    };
    
    let action = action_repo.create_action(create_request).await?;
    println!("✅ 创建 Action 成功");
    println!("   - ID: {:?}", action.id);
    println!("   - TRN: {}", action.trn);
    println!("   - 名称: {}", action.name);
    println!("   - 提供商: {}", action.provider);
    
    // 5. 测试查询 Action
    let retrieved_action = action_repo.get_action_by_trn(&action.trn).await?;
    println!("✅ 查询 Action 成功");
    println!("   - 名称: {}", retrieved_action.name);
    println!("   - 是否激活: {}", retrieved_action.is_active());
    
    // 6. 测试创建执行记录
    let execution_trn = generator.generate_execution_trn(&action.trn, &format!("exec-{}", timestamp))?;
    
    let create_execution_request = CreateExecutionRequest {
        execution_trn: execution_trn.clone(),
        action_trn: action.trn.clone(),
        tenant: "tenant123".to_string(),
        input_data: Some(r#"{"id": "123"}"#.to_string()),
    };
    
    let execution = execution_repo.create_execution(create_execution_request).await?;
    println!("✅ 创建执行记录成功");
    println!("   - 执行 TRN: {}", execution.execution_trn);
    println!("   - 状态: {}", execution.status);
    
    // 7. 测试更新执行结果
    let execution_result = ExecutionResult {
        output_data: Some(r#"{"user": {"id": "123", "name": "John Doe"}}"#.to_string()),
        status: "completed".to_string(),
        status_code: Some(200),
        error_message: None,
        duration_ms: Some(150),
    };
    
    execution_repo.update_execution_result(execution.id.unwrap(), execution_result).await?;
    println!("✅ 更新执行结果成功");
    
    // 8. 测试统计信息
    let action_stats = action_repo.get_action_stats(Some("tenant123")).await?;
    println!("✅ 获取 Action 统计信息");
    println!("   - 总 Actions: {}", action_stats.total_actions);
    println!("   - 激活 Actions: {}", action_stats.active_actions);
    
    let execution_stats = execution_repo.get_execution_stats(Some(&action.trn), None).await?;
    println!("✅ 获取执行统计信息");
    println!("   - 总执行次数: {}", execution_stats.total_executions);
    println!("   - 成功次数: {}", execution_stats.successful_executions);
    println!("   - 失败次数: {}", execution_stats.failed_executions);
    println!("   - 成功率: {:.2}%", execution_stats.success_rate.unwrap_or(0.0) * 100.0);
    
    // 9. 测试数据库统计
    let db_stats = db.get_database_stats().await?;
    println!("✅ 获取数据库统计信息");
    println!("   - 总 Actions: {}", db_stats.total_actions);
    println!("   - 总执行记录: {}", db_stats.total_executions);
    println!("   - 总测试用例: {}", db_stats.total_tests);
    println!("   - 总指标数据: {}", db_stats.total_metrics);
    
    println!("\n🎉 所有测试通过！持久化层工作正常。");
    
    Ok(())
}
