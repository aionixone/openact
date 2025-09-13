// AI Agent 使用 Action Schema 的示例
// 演示 AI Agent 如何解析和使用 Action 的 Schema 信息

use manifest::action::{ActionParser, ActionParsingOptions};
use manifest::spec::api_spec::*;
use serde_json::{json, Value};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🤖 AI Agent Schema 使用示例");
    
    // 创建测试用的 OpenAPI 规范
    let spec = create_ai_friendly_openapi_spec();
    
    // 解析 OpenAPI 规范
    let options = ActionParsingOptions {
        default_provider: "github".to_string(),
        default_tenant: "tenant123".to_string(),
        validate_schemas: true,
        ..Default::default()
    };
    
    let mut parser = ActionParser::new(options);
    let result = parser.parse_spec(&spec)?;
    
    // 模拟 AI Agent 使用 Action Schema
    for action in &result.actions {
        println!("\n🔧 AI Agent 分析 Action: {}", action.name);
        
        // 1. AI Agent 分析 Action 能力
        analyze_action_capability(action);
        
        // 2. AI Agent 构建请求参数
        let request_params = build_request_parameters(action)?;
        
        // 3. AI Agent 执行 Action（模拟）
        let response = simulate_action_execution(action, &request_params);
        
        // 4. AI Agent 解析响应
        parse_response_data(action, &response);
    }
    
    Ok(())
}

/// AI Agent 分析 Action 的能力和用途
fn analyze_action_capability(action: &manifest::action::models::Action) {
    println!("   📋 能力分析:");
    println!("      - 方法: {}", action.method);
    println!("      - 路径: {}", action.path);
    println!("      - 描述: {:?}", action.description);
    
    // 分析参数能力
    if !action.parameters.is_empty() {
        println!("      - 输入参数: {} 个", action.parameters.len());
        for param in &action.parameters {
            println!("        * {} ({}) - {}", param.name, param.location, 
                param.description.as_deref().unwrap_or("无描述"));
        }
    }
    
    // 分析请求体能力
    if let Some(request_body) = &action.request_body {
        println!("      - 请求体: 支持 {} 种内容类型", request_body.content.len());
        for content_type in request_body.content.keys() {
            println!("        * {}", content_type);
        }
    }
    
    // 分析响应能力
    println!("      - 响应: 支持 {} 种状态码", action.responses.len());
    for status_code in action.responses.keys() {
        println!("        * {}", status_code);
    }
    
    // 分析认证要求
    if let Some(auth_config) = &action.auth_config {
        println!("      - 认证: TRN={} Scheme={:?}", auth_config.connection_trn, auth_config.scheme);
    }
}

/// AI Agent 根据 Schema 构建请求参数
fn build_request_parameters(action: &manifest::action::models::Action) -> Result<HashMap<String, Value>, Box<dyn std::error::Error>> {
    println!("   🔨 构建请求参数:");
    let mut params = HashMap::new();
    
    // 处理路径参数
    for param in &action.parameters {
        if matches!(param.location, manifest::action::models::ParameterLocation::Path) {
            let value = generate_parameter_value(param);
            params.insert(param.name.clone(), value);
            println!("      - {} (path): {}", param.name, serde_json::to_string(&params[&param.name])?);
        }
    }
    
    // 处理查询参数
    for param in &action.parameters {
        if matches!(param.location, manifest::action::models::ParameterLocation::Query) {
            let value = generate_parameter_value(param);
            params.insert(param.name.clone(), value);
            println!("      - {} (query): {}", param.name, serde_json::to_string(&params[&param.name])?);
        }
    }
    
    // 处理请求体
    if let Some(request_body) = &action.request_body {
        if !request_body.content.is_empty() {
            let body_value = generate_request_body_value(request_body);
            params.insert("request_body".to_string(), body_value);
            println!("      - request_body: {}", serde_json::to_string(&params["request_body"])?);
        }
    }
    
    Ok(params)
}

/// 根据参数 Schema 生成合适的值
fn generate_parameter_value(param: &manifest::action::models::ActionParameter) -> Value {
    // 如果有示例值，使用示例值
    if let Some(example) = &param.example {
        return example.clone();
    }
    
    // 根据 Schema 生成值
    if let Some(schema) = &param.schema {
        return generate_value_from_schema(schema);
    }
    
    // 默认值
    match param.name.as_str() {
        "username" => json!("octocat"),
        "id" => json!(123),
        "limit" => json!(10),
        "page" => json!(1),
        _ => json!("default_value"),
    }
}

/// 根据请求体 Schema 生成请求体值
fn generate_request_body_value(request_body: &manifest::action::models::ActionRequestBody) -> Value {
    // 优先使用 JSON 内容类型
    if let Some(json_content) = request_body.content.get("application/json") {
        if let Some(schema) = &json_content.schema {
            return generate_value_from_schema(schema);
        }
        if let Some(example) = &json_content.example {
            return example.clone();
        }
    }
    
    // 默认请求体
    json!({
        "name": "test-repo",
        "description": "Test repository created by AI Agent",
        "private": false
    })
}

/// 根据 Schema 生成值
fn generate_value_from_schema(schema: &Value) -> Value {
    if let Some(schema_obj) = schema.as_object() {
        // 处理对象类型
        if schema_obj.get("type") == Some(&json!("object")) {
            let mut obj = HashMap::new();
            
            if let Some(properties) = schema_obj.get("properties") {
                if let Some(properties_obj) = properties.as_object() {
                    for (key, prop_schema) in properties_obj {
                        obj.insert(key.clone(), generate_value_from_schema(prop_schema));
                    }
                }
            }
            
            return json!(obj);
        }
        
        // 处理数组类型
        if schema_obj.get("type") == Some(&json!("array")) {
            if let Some(items) = schema_obj.get("items") {
                return json!([generate_value_from_schema(items)]);
            }
            return json!([]);
        }
        
        // 处理基本类型
        if let Some(example) = schema_obj.get("example") {
            return example.clone();
        }
        
        if let Some(Value::String(type_str)) = schema_obj.get("type") {
            match type_str.as_str() {
                "string" => {
                    if let Some(Value::String(format)) = schema_obj.get("format") {
                        match format.as_str() {
                            "email" => return json!("test@example.com"),
                            "uri" => return json!("https://example.com"),
                            "date-time" => return json!("2023-01-01T00:00:00Z"),
                            _ => return json!("test_string"),
                        }
                    } else {
                        return json!("test_string");
                    }
                }
                "integer" => return json!(123),
                "number" => return json!(123.45),
                "boolean" => return json!(true),
                _ => return json!("default_value"),
            }
        }
    }
    
    json!("default_value")
}

/// 模拟 Action 执行
fn simulate_action_execution(
    action: &manifest::action::models::Action, 
    params: &HashMap<String, Value>
) -> Value {
    println!("   ⚡ 执行 Action:");
    
    // 模拟不同的响应
    match action.name.as_str() {
        "getUser" => {
            println!("      - 调用 GitHub API: GET /users/{}", 
                params.get("username").unwrap_or(&json!("octocat")));
            json!({
                "id": 1,
                "login": "octocat",
                "name": "The Octocat",
                "email": "octocat@github.com",
                "public_repos": 8
            })
        }
        "createRepo" => {
            println!("      - 调用 GitHub API: POST /user/repos");
            if let Some(body) = params.get("request_body") {
                println!("      - 请求体: {}", serde_json::to_string(body).unwrap());
            }
            json!({
                "id": 1296269,
                "name": "test-repo",
                "full_name": "octocat/test-repo",
                "private": false,
                "html_url": "https://github.com/octocat/test-repo",
                "created_at": "2023-01-01T00:00:00Z"
            })
        }
        _ => json!({"message": "Action executed successfully"})
    }
}

/// AI Agent 解析响应数据
fn parse_response_data(action: &manifest::action::models::Action, response: &Value) {
    println!("   📊 解析响应数据:");
    
    // 根据响应 Schema 解析数据
    if let Some(success_response) = action.responses.get("200") {
        println!("      - 成功响应 (200): {}", success_response.description);
        parse_response_content(&success_response.content, response);
    }
    
    if let Some(created_response) = action.responses.get("201") {
        println!("      - 创建响应 (201): {}", created_response.description);
        parse_response_content(&created_response.content, response);
    }
    
    // 提取关键信息
    if let Some(response_obj) = response.as_object() {
        for (key, value) in response_obj {
            println!("      - {}: {}", key, value);
        }
    }
}

/// 解析响应内容
fn parse_response_content(content: &HashMap<String, manifest::action::models::ActionContent>, _response: &Value) {
    for (content_type, action_content) in content {
        println!("        * Content-Type: {}", content_type);
        
        if let Some(schema) = &action_content.schema {
            println!("        * Schema: {}", serde_json::to_string_pretty(schema).unwrap());
        }
        
        if let Some(example) = &action_content.example {
            println!("        * Example: {}", serde_json::to_string_pretty(example).unwrap());
        }
    }
}

/// 创建 AI Agent 友好的 OpenAPI 规范
fn create_ai_friendly_openapi_spec() -> OpenApi30Spec {
    OpenApi30Spec {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: "AI Agent Friendly API".to_string(),
            version: "1.0.0".to_string(),
            description: Some("API designed for AI Agent consumption with clear schemas".to_string()),
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
            },
            Tag {
                name: "repositories".to_string(),
                description: Some("Repository management operations".to_string()),
                external_docs: None,
                extensions: HashMap::new(),
            }
        ],
        paths: Paths {
            paths: {
                let mut paths = HashMap::new();
                
                // GET /users/{username} - 获取用户信息
                paths.insert("/users/{username}".to_string(), PathItem {
                    reference: None,
                    summary: None,
                    description: None,
                    get: Some(Operation {
                        tags: vec!["users".to_string()],
                        summary: Some("Get user information".to_string()),
                        description: Some("Retrieve detailed information about a user by username".to_string()),
                        external_docs: None,
                        operation_id: Some("getUser".to_string()),
                        parameters: vec![
                            ParameterOrReference::Item(Parameter {
                                name: "username".to_string(),
                                location: "path".to_string(),
                                description: Some("The username of the user to retrieve".to_string()),
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
                                    description: "User information retrieved successfully".to_string(),
                                    headers: HashMap::new(),
                                    content: {
                                        let mut content = HashMap::new();
                                        content.insert("application/json".to_string(), MediaType {
                                            schema: Some(SchemaOrReference::Item(Schema {
                                                r#type: Some("object".to_string()),
                                                description: Some("User object with complete information".to_string()),
                                                properties: {
                                                    let mut properties = HashMap::new();
                                                    properties.insert("id".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("integer".to_string()),
                                                        format: Some("int64".to_string()),
                                                        description: Some("Unique user identifier".to_string()),
                                                        example: Some(json!(1)),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("login".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        description: Some("Username".to_string()),
                                                        example: Some(json!("octocat")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("name".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        description: Some("Full name of the user".to_string()),
                                                        example: Some(json!("The Octocat")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("email".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        format: Some("email".to_string()),
                                                        description: Some("Email address of the user".to_string()),
                                                        example: Some(json!("octocat@github.com")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("public_repos".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("integer".to_string()),
                                                        description: Some("Number of public repositories".to_string()),
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
                                    description: "User not found".to_string(),
                                    headers: HashMap::new(),
                                    content: {
                                        let mut content = HashMap::new();
                                        content.insert("application/json".to_string(), MediaType {
                                            schema: Some(SchemaOrReference::Item(Schema {
                                                r#type: Some("object".to_string()),
                                                description: Some("Error response when user is not found".to_string()),
                                                properties: {
                                                    let mut properties = HashMap::new();
                                                    properties.insert("message".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        description: Some("Error message".to_string()),
                                                        example: Some(json!("Not Found")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("documentation_url".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        format: Some("uri".to_string()),
                                                        description: Some("Link to documentation".to_string()),
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
                            extensions.insert("x-auth".to_string(), json!({
                                "auth_type": "oauth2",
                                "provider": "github",
                                "scopes": ["user:email"]
                            }));
                            extensions.insert("x-ai-friendly".to_string(), json!(true));
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
                
                // POST /user/repos - 创建仓库
                paths.insert("/user/repos".to_string(), PathItem {
                    reference: None,
                    summary: None,
                    description: None,
                    get: None,
                    put: None,
                    post: Some(Operation {
                        tags: vec!["repositories".to_string()],
                        summary: Some("Create a new repository".to_string()),
                        description: Some("Create a new repository for the authenticated user".to_string()),
                        external_docs: None,
                        operation_id: Some("createRepo".to_string()),
                        parameters: vec![],
                        request_body: Some(RequestBodyOrReference::Item(RequestBody {
                            description: Some("Repository creation data with clear schema for AI Agent".to_string()),
                            required: true,
                            content: {
                                let mut content = HashMap::new();
                                content.insert("application/json".to_string(), MediaType {
                                    schema: Some(SchemaOrReference::Item(Schema {
                                        r#type: Some("object".to_string()),
                                        description: Some("Repository creation request with AI-friendly schema".to_string()),
                                        properties: {
                                            let mut properties = HashMap::new();
                                            properties.insert("name".to_string(), SchemaOrReference::Item(Schema {
                                                r#type: Some("string".to_string()),
                                                description: Some("Repository name (required, 1-100 characters, alphanumeric with dots, dashes, underscores)".to_string()),
                                                min_length: Some(1),
                                                max_length: Some(100),
                                                pattern: Some("^[a-zA-Z0-9._-]+$".to_string()),
                                                example: Some(json!("my-awesome-repo")),
                                                ..Default::default()
                                            }));
                                            properties.insert("description".to_string(), SchemaOrReference::Item(Schema {
                                                r#type: Some("string".to_string()),
                                                description: Some("Repository description (optional, max 350 characters)".to_string()),
                                                max_length: Some(350),
                                                example: Some(json!("This is an awesome repository created by AI Agent")),
                                                ..Default::default()
                                            }));
                                            properties.insert("private".to_string(), SchemaOrReference::Item(Schema {
                                                r#type: Some("boolean".to_string()),
                                                description: Some("Whether the repository should be private (default: false)".to_string()),
                                                default: Some(json!(false)),
                                                example: Some(json!(false)),
                                                ..Default::default()
                                            }));
                                            properties.insert("auto_init".to_string(), SchemaOrReference::Item(Schema {
                                                r#type: Some("boolean".to_string()),
                                                description: Some("Whether to initialize with README (default: false)".to_string()),
                                                default: Some(json!(false)),
                                                example: Some(json!(true)),
                                                ..Default::default()
                                            }));
                                            properties
                                        },
                                        required: vec!["name".to_string()],
                                        example: Some(json!({
                                            "name": "my-awesome-repo",
                                            "description": "This is an awesome repository created by AI Agent",
                                            "private": false,
                                            "auto_init": true
                                        })),
                                        ..Default::default()
                                    })),
                                    example: Some(json!({
                                        "name": "my-awesome-repo",
                                        "description": "This is an awesome repository created by AI Agent",
                                        "private": false,
                                        "auto_init": true
                                    })),
                                    examples: HashMap::new(),
                                    encoding: HashMap::new(),
                                    extensions: HashMap::new(),
                                });
                                content
                            },
                            extensions: HashMap::new(),
                        })),
                        responses: Responses {
                            default: None,
                            responses: {
                                let mut responses = HashMap::new();
                                responses.insert("201".to_string(), ResponseOrReference::Item(Response {
                                    description: "Repository created successfully".to_string(),
                                    headers: HashMap::new(),
                                    content: {
                                        let mut content = HashMap::new();
                                        content.insert("application/json".to_string(), MediaType {
                                            schema: Some(SchemaOrReference::Item(Schema {
                                                r#type: Some("object".to_string()),
                                                description: Some("Created repository object with all details".to_string()),
                                                properties: {
                                                    let mut properties = HashMap::new();
                                                    properties.insert("id".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("integer".to_string()),
                                                        format: Some("int64".to_string()),
                                                        description: Some("Unique repository identifier".to_string()),
                                                        example: Some(json!(1296269)),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("name".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        description: Some("Repository name".to_string()),
                                                        example: Some(json!("Hello-World")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("full_name".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        description: Some("Full repository name with owner".to_string()),
                                                        example: Some(json!("octocat/Hello-World")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("private".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("boolean".to_string()),
                                                        description: Some("Repository visibility".to_string()),
                                                        example: Some(json!(false)),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("html_url".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        format: Some("uri".to_string()),
                                                        description: Some("Repository web URL".to_string()),
                                                        example: Some(json!("https://github.com/octocat/Hello-World")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("created_at".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        format: Some("date-time".to_string()),
                                                        description: Some("Creation timestamp".to_string()),
                                                        example: Some(json!("2011-01-26T19:01:12Z")),
                                                        ..Default::default()
                                                    }));
                                                    properties
                                                },
                                                required: vec!["id".to_string(), "name".to_string(), "full_name".to_string()],
                                                example: Some(json!({
                                                    "id": 1296269,
                                                    "name": "Hello-World",
                                                    "full_name": "octocat/Hello-World",
                                                    "private": false,
                                                    "html_url": "https://github.com/octocat/Hello-World",
                                                    "created_at": "2011-01-26T19:01:12Z"
                                                })),
                                                ..Default::default()
                                            })),
                                            example: Some(json!({
                                                "id": 1296269,
                                                "name": "Hello-World",
                                                "full_name": "octocat/Hello-World",
                                                "private": false,
                                                "html_url": "https://github.com/octocat/Hello-World",
                                                "created_at": "2011-01-26T19:01:12Z"
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
                                responses.insert("422".to_string(), ResponseOrReference::Item(Response {
                                    description: "Validation failed".to_string(),
                                    headers: HashMap::new(),
                                    content: {
                                        let mut content = HashMap::new();
                                        content.insert("application/json".to_string(), MediaType {
                                            schema: Some(SchemaOrReference::Item(Schema {
                                                r#type: Some("object".to_string()),
                                                description: Some("Validation error with detailed field information".to_string()),
                                                properties: {
                                                    let mut properties = HashMap::new();
                                                    properties.insert("message".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        description: Some("Error message".to_string()),
                                                        example: Some(json!("Validation Failed")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("errors".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("array".to_string()),
                                                        description: Some("List of validation errors with field details".to_string()),
                                                        items: Some(Box::new(SchemaOrReference::Item(Schema {
                                                            r#type: Some("object".to_string()),
                                                            properties: {
                                                                let mut error_props = HashMap::new();
                                                                error_props.insert("resource".to_string(), SchemaOrReference::Item(Schema {
                                                                    r#type: Some("string".to_string()),
                                                                    description: Some("Resource type that failed validation".to_string()),
                                                                    example: Some(json!("Repository")),
                                                                    ..Default::default()
                                                                }));
                                                                error_props.insert("field".to_string(), SchemaOrReference::Item(Schema {
                                                                    r#type: Some("string".to_string()),
                                                                    description: Some("Field name that failed validation".to_string()),
                                                                    example: Some(json!("name")),
                                                                    ..Default::default()
                                                                }));
                                                                error_props.insert("code".to_string(), SchemaOrReference::Item(Schema {
                                                                    r#type: Some("string".to_string()),
                                                                    description: Some("Error code indicating the type of validation failure".to_string()),
                                                                    example: Some(json!("missing_field")),
                                                                    ..Default::default()
                                                                }));
                                                                error_props
                                                            },
                                                            required: vec!["resource".to_string(), "field".to_string(), "code".to_string()],
                                                            ..Default::default()
                                                        }))),
                                                        example: Some(json!([
                                                            {
                                                                "resource": "Repository",
                                                                "field": "name",
                                                                "code": "missing_field"
                                                            }
                                                        ])),
                                                        ..Default::default()
                                                    }));
                                                    properties
                                                },
                                                required: vec!["message".to_string()],
                                                example: Some(json!({
                                                    "message": "Validation Failed",
                                                    "errors": [
                                                        {
                                                            "resource": "Repository",
                                                            "field": "name",
                                                            "code": "missing_field"
                                                        }
                                                    ]
                                                })),
                                                ..Default::default()
                                            })),
                                            example: Some(json!({
                                                "message": "Validation Failed",
                                                "errors": [
                                                    {
                                                        "resource": "Repository",
                                                        "field": "name",
                                                        "code": "missing_field"
                                                    }
                                                ]
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
                            extensions.insert("x-action-type".to_string(), json!("create"));
                            extensions.insert("x-rate-limit".to_string(), json!(100));
                            extensions.insert("x-auth".to_string(), json!({
                                "auth_type": "oauth2",
                                "provider": "github",
                                "scopes": ["repo"]
                            }));
                            extensions.insert("x-ai-friendly".to_string(), json!(true));
                            extensions
                        },
                    }),
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
