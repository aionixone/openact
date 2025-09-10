// OpenAPI 文档质量检查工具
// 确保 OpenAPI 文档适合 AI Agent 使用

use manifest::action::{ActionParser, ActionParsingOptions};
use manifest::spec::api_spec::*;
use serde_json::{json, Value};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 OpenAPI 文档质量检查工具");
    
    // 创建测试用的 OpenAPI 规范
    let spec = create_test_openapi_spec();
    
    // 解析 OpenAPI 规范
    let options = ActionParsingOptions {
        default_provider: "github".to_string(),
        default_tenant: "tenant123".to_string(),
        validate_schemas: true,
        ..Default::default()
    };
    
    let mut parser = ActionParser::new(options);
    let result = parser.parse_spec(&spec)?;
    
    // 对每个 Action 进行质量检查
    for action in &result.actions {
        println!("\n🔧 检查 Action: {}", action.name);
        let quality_score = check_action_quality(action);
        println!("   📊 质量评分: {}/100", quality_score);
        
        if quality_score >= 80 {
            println!("   ✅ 质量优秀 - 适合 AI Agent 使用");
        } else if quality_score >= 60 {
            println!("   ⚠️  质量一般 - 建议改进");
        } else {
            println!("   ❌ 质量较差 - 需要大幅改进");
        }
    }
    
    Ok(())
}

/// 检查 Action 的质量
fn check_action_quality(action: &manifest::action::models::Action) -> u32 {
    let mut score = 0;
    let mut issues = Vec::new();
    
    // 1. 基本信息质量 (20分)
    score += check_basic_info_quality(action, &mut issues);
    
    // 2. 参数质量 (30分)
    score += check_parameters_quality(action, &mut issues);
    
    // 3. 请求体质量 (20分)
    score += check_request_body_quality(action, &mut issues);
    
    // 4. 响应质量 (30分)
    score += check_responses_quality(action, &mut issues);
    
    // 输出问题详情
    if !issues.is_empty() {
        println!("   📋 发现的问题:");
        for issue in issues {
            println!("      - {}", issue);
        }
    }
    
    score
}

/// 检查基本信息质量
fn check_basic_info_quality(action: &manifest::action::models::Action, issues: &mut Vec<String>) -> u32 {
    let mut score = 0;
    
    // 检查描述 (10分)
    if let Some(description) = &action.description {
        if description.len() > 20 {
            score += 10;
        } else {
            issues.push("描述太短，建议提供更详细的说明".to_string());
            score += 5;
        }
    } else {
        issues.push("缺少操作描述".to_string());
    }
    
    // 检查标签 (5分)
    if !action.tags.is_empty() {
        score += 5;
    } else {
        issues.push("缺少标签分类".to_string());
    }
    
    // 检查扩展字段 (5分)
    if action.extensions.contains_key("x-ai-friendly") {
        score += 5;
    } else {
        issues.push("建议添加 x-ai-friendly 标记".to_string());
    }
    
    score
}

/// 检查参数质量
fn check_parameters_quality(action: &manifest::action::models::Action, issues: &mut Vec<String>) -> u32 {
    let mut score = 0;
    
    if action.parameters.is_empty() {
        return 30; // 没有参数也是合理的
    }
    
    for param in &action.parameters {
        let param_score = check_parameter_quality(param, issues);
        score += param_score;
    }
    
    // 平均分
    score / action.parameters.len() as u32
}

/// 检查单个参数质量
fn check_parameter_quality(param: &manifest::action::models::ActionParameter, issues: &mut Vec<String>) -> u32 {
    let mut score = 0;
    
    // 检查描述 (5分)
    if let Some(description) = &param.description {
        if description.len() > 10 {
            score += 5;
        } else {
            issues.push(format!("参数 '{}' 描述太短", param.name));
            score += 2;
        }
    } else {
        issues.push(format!("参数 '{}' 缺少描述", param.name));
    }
    
    // 检查 Schema (10分)
    if let Some(schema) = &param.schema {
        score += check_schema_quality(schema, &format!("参数 '{}'", param.name), issues);
    } else {
        issues.push(format!("参数 '{}' 缺少 Schema 定义", param.name));
    }
    
    // 检查示例 (5分)
    if param.example.is_some() {
        score += 5;
    } else {
        issues.push(format!("参数 '{}' 缺少示例值", param.name));
    }
    
    // 检查必填字段 (5分)
    if param.required {
        score += 5;
    } else {
        score += 3; // 可选参数也是合理的
    }
    
    // 检查位置 (5分)
    match param.location {
        manifest::action::models::ParameterLocation::Path => {
            if param.required {
                score += 5;
            } else {
                issues.push(format!("路径参数 '{}' 应该是必填的", param.name));
            }
        }
        _ => score += 5,
    }
    
    score
}

/// 检查请求体质量
fn check_request_body_quality(action: &manifest::action::models::Action, issues: &mut Vec<String>) -> u32 {
    let mut score = 0;
    
    if let Some(request_body) = &action.request_body {
        // 检查描述 (5分)
        if let Some(description) = &request_body.description {
            if description.len() > 10 {
                score += 5;
            } else {
                issues.push("请求体描述太短".to_string());
                score += 2;
            }
        } else {
            issues.push("请求体缺少描述".to_string());
        }
        
        // 检查内容类型 (5分)
        if !request_body.content.is_empty() {
            score += 5;
        } else {
            issues.push("请求体缺少内容类型定义".to_string());
        }
        
        // 检查 Schema 质量 (10分)
        for (content_type, content) in &request_body.content {
            if let Some(schema) = &content.schema {
                score += check_schema_quality(schema, &format!("请求体 '{}'", content_type), issues);
            } else {
                issues.push(format!("请求体 '{}' 缺少 Schema 定义", content_type));
            }
        }
    } else {
        // 没有请求体也是合理的（如 GET 请求）
        score = 20;
    }
    
    score
}

/// 检查响应质量
fn check_responses_quality(action: &manifest::action::models::Action, issues: &mut Vec<String>) -> u32 {
    let mut score = 0;
    
    if action.responses.is_empty() {
        issues.push("缺少响应定义".to_string());
        return 0;
    }
    
    for (status_code, response) in &action.responses {
        let response_score = check_response_quality(status_code, response, issues);
        score += response_score;
    }
    
    // 平均分
    score / action.responses.len() as u32
}

/// 检查单个响应质量
fn check_response_quality(status_code: &str, response: &manifest::action::models::ActionResponse, issues: &mut Vec<String>) -> u32 {
    let mut score = 0;
    
    // 检查描述 (5分)
    if response.description.len() > 10 {
        score += 5;
    } else {
        issues.push(format!("响应 '{}' 描述太短", status_code));
        score += 2;
    }
    
    // 检查内容类型 (5分)
    if !response.content.is_empty() {
        score += 5;
    } else {
        issues.push(format!("响应 '{}' 缺少内容类型定义", status_code));
    }
    
    // 检查 Schema 质量 (20分)
    for (content_type, content) in &response.content {
        if let Some(schema) = &content.schema {
            score += check_schema_quality(schema, &format!("响应 '{}' '{}'", status_code, content_type), issues);
        } else {
            issues.push(format!("响应 '{}' '{}' 缺少 Schema 定义", status_code, content_type));
        }
    }
    
    score
}

/// 检查 Schema 质量
fn check_schema_quality(schema: &serde_json::Value, context: &str, issues: &mut Vec<String>) -> u32 {
    let mut score = 0;
    
    if let Some(schema_obj) = schema.as_object() {
        // 检查类型 (5分)
        if schema_obj.get("type").is_some() {
            score += 5;
        } else {
            issues.push(format!("{} Schema 缺少类型定义", context));
        }
        
        // 检查描述 (5分)
        if let Some(Value::String(description)) = schema_obj.get("description") {
            if description.len() > 10 {
                score += 5;
            } else {
                issues.push(format!("{} Schema 描述太短", context));
                score += 2;
            }
        } else {
            issues.push(format!("{} Schema 缺少描述", context));
        }
        
        // 检查示例 (5分)
        if schema_obj.get("example").is_some() {
            score += 5;
        } else {
            issues.push(format!("{} Schema 缺少示例", context));
        }
        
        // 检查对象属性 (5分)
        if let Some(properties) = schema_obj.get("properties") {
            if let Some(properties_obj) = properties.as_object() {
                if !properties_obj.is_empty() {
                    score += 5;
                } else {
                    issues.push(format!("{} Schema 对象缺少属性定义", context));
                }
            }
        }
        
        // 检查必填字段 (5分)
        if let Some(required) = schema_obj.get("required") {
            if let Some(required_array) = required.as_array() {
                if !required_array.is_empty() {
                    score += 5;
                }
            }
        }
        
        // 检查验证规则 (5分)
        let has_validation = schema_obj.get("minLength").is_some() ||
                           schema_obj.get("maxLength").is_some() ||
                           schema_obj.get("pattern").is_some() ||
                           schema_obj.get("minimum").is_some() ||
                           schema_obj.get("maximum").is_some();
        
        if has_validation {
            score += 5;
        } else {
            issues.push(format!("{} Schema 缺少验证规则", context));
        }
    }
    
    score
}

/// 创建测试用的 OpenAPI 规范
fn create_test_openapi_spec() -> OpenApi30Spec {
    OpenApi30Spec {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: "Quality Test API".to_string(),
            version: "1.0.0".to_string(),
            description: Some("API for testing OpenAPI quality".to_string()),
            terms_of_service: None,
            contact: None,
            license: None,
            extensions: HashMap::new(),
        },
        external_docs: None,
        servers: vec![
            Server {
                url: "https://api.example.com".to_string(),
                description: Some("Main API server".to_string()),
                variables: HashMap::new(),
                extensions: HashMap::new(),
            }
        ],
        security: vec![],
        tags: vec![
            Tag {
                name: "users".to_string(),
                description: Some("User management operations".to_string()),
                external_docs: None,
                extensions: HashMap::new(),
            }
        ],
        paths: Paths {
            paths: {
                let mut paths = HashMap::new();
                
                // 高质量示例
                paths.insert("/users/{username}".to_string(), PathItem {
                    reference: None,
                    summary: None,
                    description: None,
                    get: Some(Operation {
                        tags: vec!["users".to_string()],
                        summary: Some("Get user information".to_string()),
                        description: Some("Retrieve detailed information about a user by username. This endpoint provides comprehensive user data including profile information, statistics, and metadata.".to_string()),
                        external_docs: None,
                        operation_id: Some("getUser".to_string()),
                        parameters: vec![
                            ParameterOrReference::Item(Parameter {
                                name: "username".to_string(),
                                location: "path".to_string(),
                                description: Some("GitHub username (1-39 characters, alphanumeric with dashes and underscores only)".to_string()),
                                required: true,
                                deprecated: false,
                                allow_empty_value: false,
                                style: None,
                                explode: None,
                                allow_reserved: false,
                                schema: Some(SchemaOrReference::Item(Schema {
                                    r#type: Some("string".to_string()),
                                    description: Some("Username must be alphanumeric and 1-39 characters".to_string()),
                                    pattern: Some("^[a-zA-Z0-9_-]+$".to_string()),
                                    min_length: Some(1),
                                    max_length: Some(39),
                                    example: Some(json!("octocat")),
                                    ..Default::default()
                                })),
                                content: HashMap::new(),
                                example: Some(json!("octocat")),
                                examples: HashMap::new(),
                                extensions: HashMap::new(),
                            })
                        ],
                        request_body: None,
                        responses: Responses {
                            default: None,
                            responses: {
                                let mut responses = HashMap::new();
                                responses.insert("200".to_string(), ResponseOrReference::Item(Response {
                                    description: "User information retrieved successfully. Returns complete user profile with all available data.".to_string(),
                                    headers: HashMap::new(),
                                    content: {
                                        let mut content = HashMap::new();
                                        content.insert("application/json".to_string(), MediaType {
                                            schema: Some(SchemaOrReference::Item(Schema {
                                                r#type: Some("object".to_string()),
                                                description: Some("User object with complete information including profile, statistics, and metadata".to_string()),
                                                properties: {
                                                    let mut properties = HashMap::new();
                                                    properties.insert("id".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("integer".to_string()),
                                                        format: Some("int64".to_string()),
                                                        description: Some("Unique user identifier assigned by the system".to_string()),
                                                        example: Some(json!(1)),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("login".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        description: Some("Username used for login and identification".to_string()),
                                                        example: Some(json!("octocat")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("name".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        description: Some("Full display name of the user".to_string()),
                                                        example: Some(json!("The Octocat")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("email".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        format: Some("email".to_string()),
                                                        description: Some("Primary email address of the user".to_string()),
                                                        example: Some(json!("octocat@github.com")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("public_repos".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("integer".to_string()),
                                                        description: Some("Total number of public repositories owned by the user".to_string()),
                                                        minimum: Some(0.0),
                                                        example: Some(json!(8)),
                                                        ..Default::default()
                                                    }));
                                                    properties
                                                },
                                                required: vec!["id".to_string(), "login".to_string()],
                                                example: Some(json!({
                                                    "id": 1,
                                                    "login": "octocat",
                                                    "name": "The Octocat",
                                                    "email": "octocat@github.com",
                                                    "public_repos": 8
                                                })),
                                                ..Default::default()
                                            })),
                                            example: Some(json!({
                                                "id": 1,
                                                "login": "octocat",
                                                "name": "The Octocat",
                                                "email": "octocat@github.com",
                                                "public_repos": 8
                                            })),
                                            examples: HashMap::new(),
                                            encoding: HashMap::new(),
                                            extensions: HashMap::new(),
                                        });
                                        content
                                    },
                                    links: HashMap::new(),
                                    extensions: HashMap::new(),
                                }));
                                responses.insert("404".to_string(), ResponseOrReference::Item(Response {
                                    description: "User not found. The specified username does not exist or is not accessible.".to_string(),
                                    headers: HashMap::new(),
                                    content: {
                                        let mut content = HashMap::new();
                                        content.insert("application/json".to_string(), MediaType {
                                            schema: Some(SchemaOrReference::Item(Schema {
                                                r#type: Some("object".to_string()),
                                                description: Some("Error response when user is not found or inaccessible".to_string()),
                                                properties: {
                                                    let mut properties = HashMap::new();
                                                    properties.insert("message".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        description: Some("Human-readable error message describing the issue".to_string()),
                                                        example: Some(json!("Not Found")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("documentation_url".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        format: Some("uri".to_string()),
                                                        description: Some("Link to relevant documentation for this error".to_string()),
                                                        example: Some(json!("https://docs.example.com/rest/reference/users#get-a-user")),
                                                        ..Default::default()
                                                    }));
                                                    properties
                                                },
                                                required: vec!["message".to_string()],
                                                example: Some(json!({
                                                    "message": "Not Found",
                                                    "documentation_url": "https://docs.example.com/rest/reference/users#get-a-user"
                                                })),
                                                ..Default::default()
                                            })),
                                            example: Some(json!({
                                                "message": "Not Found",
                                                "documentation_url": "https://docs.example.com/rest/reference/users#get-a-user"
                                            })),
                                            examples: HashMap::new(),
                                            encoding: HashMap::new(),
                                            extensions: HashMap::new(),
                                        });
                                        content
                                    },
                                    links: HashMap::new(),
                                    extensions: HashMap::new(),
                                }));
                                responses
                            },
                            extensions: HashMap::new(),
                        },
                        callbacks: HashMap::new(),
                        deprecated: false,
                        security: vec![],
                        servers: vec![],
                        extensions: {
                            let mut extensions = HashMap::new();
                            extensions.insert("x-action-type".to_string(), json!("read"));
                            extensions.insert("x-rate-limit".to_string(), json!(5000));
                            extensions.insert("x-ai-friendly".to_string(), json!(true));
                            extensions.insert("x-ai-description".to_string(), json!("This endpoint is optimized for AI Agent consumption with clear schemas and examples"));
                            extensions
                        },
                    }),
                    put: None,
                    post: None,
                    delete: None,
                    options: None,
                    head: None,
                    patch: None,
                    trace: None,
                    servers: vec![],
                    parameters: vec![],
                    extensions: HashMap::new(),
                });
                
                // 低质量示例
                paths.insert("/bad-endpoint".to_string(), PathItem {
                    reference: None,
                    summary: None,
                    description: None,
                    get: Some(Operation {
                        tags: vec![],
                        summary: None,
                        description: None,
                        external_docs: None,
                        operation_id: Some("badEndpoint".to_string()),
                        parameters: vec![
                            ParameterOrReference::Item(Parameter {
                                name: "param".to_string(),
                                location: "query".to_string(),
                                description: None,
                                required: false,
                                deprecated: false,
                                allow_empty_value: false,
                                style: None,
                                explode: None,
                                allow_reserved: false,
                                schema: None,
                                content: HashMap::new(),
                                example: None,
                                examples: HashMap::new(),
                                extensions: HashMap::new(),
                            })
                        ],
                        request_body: None,
                        responses: Responses {
                            default: None,
                            responses: {
                                let mut responses = HashMap::new();
                                responses.insert("200".to_string(), ResponseOrReference::Item(Response {
                                    description: "OK".to_string(),
                                    headers: HashMap::new(),
                                    content: HashMap::new(),
                                    links: HashMap::new(),
                                    extensions: HashMap::new(),
                                }));
                                responses
                            },
                            extensions: HashMap::new(),
                        },
                        callbacks: HashMap::new(),
                        deprecated: false,
                        security: vec![],
                        servers: vec![],
                        extensions: HashMap::new(),
                    }),
                    put: None,
                    post: None,
                    delete: None,
                    options: None,
                    head: None,
                    patch: None,
                    trace: None,
                    servers: vec![],
                    parameters: vec![],
                    extensions: HashMap::new(),
                });
                
                paths
            },
            extensions: HashMap::new(),
        },
        components: Some(Components {
            schemas: HashMap::new(),
            responses: HashMap::new(),
            parameters: HashMap::new(),
            examples: HashMap::new(),
            request_bodies: HashMap::new(),
            headers: HashMap::new(),
            security_schemes: HashMap::new(),
            links: HashMap::new(),
            callbacks: HashMap::new(),
            extensions: HashMap::new(),
        }),
        extensions: HashMap::new(),
    }
}
