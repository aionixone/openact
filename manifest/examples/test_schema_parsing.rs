// 测试 OpenAPI 中的 Request 和 Response Schema 解析
// 演示如何解析和处理 OpenAPI 文档中的 schema 定义

use manifest::action::{ActionParser, ActionParsingOptions};
use manifest::spec::api_spec::*;
use serde_json::json;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 OpenAPI Schema 解析测试");
    
    // 创建测试用的 OpenAPI 规范
    let spec = create_test_openapi_spec();
    
    // 创建 Action Parser
    let options = ActionParsingOptions {
        default_provider: "github".to_string(),
        default_tenant: "tenant123".to_string(),
        validate_schemas: true,
        ..Default::default()
    };
    
    let mut parser = ActionParser::new(options);
    
    // 解析 OpenAPI 规范
    let result = parser.parse_spec(&spec)?;
    
    println!("\n📊 解析结果统计:");
    println!("   - 总操作数: {}", result.stats.total_operations);
    println!("   - 成功解析: {}", result.stats.successful_actions);
    println!("   - 失败操作: {}", result.stats.failed_operations);
    println!("   - 处理时间: {}ms", result.stats.processing_time_ms);
    
    // 分析每个 Action 的 Schema
    for (i, action) in result.actions.iter().enumerate() {
        println!("\n🔧 Action {}: {}", i + 1, action.name);
        println!("   - 方法: {}", action.method);
        println!("   - 路径: {}", action.path);
        println!("   - 参数数量: {}", action.parameters.len());
        
        // 分析参数 Schema
        for param in &action.parameters {
            println!("   📝 参数: {} ({})", param.name, param.location);
            if let Some(schema) = &param.schema {
                println!("      Schema: {}", serde_json::to_string_pretty(schema)?);
            }
            if let Some(example) = &param.example {
                println!("      Example: {}", serde_json::to_string_pretty(example)?);
            }
        }
        
        // 分析 Request Body Schema
        if let Some(request_body) = &action.request_body {
            println!("   📤 Request Body:");
            println!("      Required: {}", request_body.required);
            if let Some(description) = &request_body.description {
                println!("      Description: {}", description);
            }
            for (content_type, content) in &request_body.content {
                println!("      Content-Type: {}", content_type);
                if let Some(schema) = &content.schema {
                    println!("         Schema: {}", serde_json::to_string_pretty(schema)?);
                }
                if let Some(example) = &content.example {
                    println!("         Example: {}", serde_json::to_string_pretty(example)?);
                }
            }
        }
        
        // 分析 Response Schema
        println!("   📥 Responses:");
        for (status_code, response) in &action.responses {
            println!("      Status {}: {}", status_code, response.description);
            for (content_type, content) in &response.content {
                println!("         Content-Type: {}", content_type);
                if let Some(schema) = &content.schema {
                    println!("            Schema: {}", serde_json::to_string_pretty(schema)?);
                }
                if let Some(example) = &content.example {
                    println!("            Example: {}", serde_json::to_string_pretty(example)?);
                }
            }
        }
        
        // 分析扩展字段
        if !action.extensions.is_empty() {
            println!("   🔧 Extensions:");
            for (key, value) in &action.extensions {
                println!("      {}: {}", key, serde_json::to_string_pretty(value)?);
            }
        }
        
        // 分析认证配置
        if let Some(auth_config) = &action.auth_config {
            println!("   🔐 Auth Config:");
            println!("      Connection TRN: {}", auth_config.connection_trn);
            println!("      Scheme: {:?}", auth_config.scheme);
        }
    }
    
    // 测试 Schema 验证
    println!("\n✅ Schema 验证测试:");
    for action in &result.actions {
        match action.validate() {
            Ok(_) => println!("   ✅ {} - 验证通过", action.name),
            Err(e) => println!("   ❌ {} - 验证失败: {}", action.name, e),
        }
    }
    
    Ok(())
}

/// 创建测试用的 OpenAPI 规范，包含完整的 Schema 定义
fn create_test_openapi_spec() -> OpenApi30Spec {
    OpenApi30Spec {
        openapi: "3.0.0".to_string(),
        info: Info {
            title: "GitHub API".to_string(),
            version: "1.0.0".to_string(),
            description: Some("GitHub API with comprehensive schema examples".to_string()),
            terms_of_service: None,
            contact: None,
            license: None,
            extensions: HashMap::new(),
        },
        external_docs: None,
        servers: vec![
            Server {
                url: "https://api.github.com".to_string(),
                description: Some("GitHub API server".to_string()),
                variables: HashMap::new(),
                extensions: HashMap::new(),
            }
        ],
        security: vec![],
        tags: vec![
            Tag {
                name: "users".to_string(),
                description: Some("User operations".to_string()),
                external_docs: None,
                extensions: HashMap::new(),
            },
            Tag {
                name: "repositories".to_string(),
                description: Some("Repository operations".to_string()),
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
                        summary: Some("Get user by username".to_string()),
                        description: Some("Retrieve a user by their username".to_string()),
                        external_docs: None,
                        operation_id: Some("getUser".to_string()),
                        parameters: vec![
                            ParameterOrReference::Item(Parameter {
                                name: "username".to_string(),
                                location: "path".to_string(),
                                description: Some("The username of the user".to_string()),
                                required: true,
                                deprecated: false,
                                allow_empty_value: false,
                                style: None,
                                explode: None,
                                allow_reserved: false,
                                schema: Some(SchemaOrReference::Item(Schema {
                                    r#type: Some("string".to_string()),
                                    description: Some("Username must be alphanumeric".to_string()),
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
                                    description: "User found".to_string(),
                                    headers: HashMap::new(),
                                    content: {
                                        let mut content = HashMap::new();
                                        content.insert("application/json".to_string(), MediaType {
                                            schema: Some(SchemaOrReference::Item(Schema {
                                                r#type: Some("object".to_string()),
                                                description: Some("User object".to_string()),
                                                properties: {
                                                    let mut properties = HashMap::new();
                                                    properties.insert("id".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("integer".to_string()),
                                                        format: Some("int64".to_string()),
                                                        description: Some("User ID".to_string()),
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
                                                        description: Some("Full name".to_string()),
                                                        example: Some(json!("The Octocat")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("email".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        format: Some("email".to_string()),
                                                        description: Some("Email address".to_string()),
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
                                                description: Some("Error object".to_string()),
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
                                                        description: Some("Documentation URL".to_string()),
                                                        example: Some(json!("https://docs.github.com/rest/reference/users#get-a-user")),
                                                        ..Default::default()
                                                    }));
                                                    properties
                                                },
                                                required: vec!["message".to_string()],
                                                example: Some(json!({
                                                    "message": "Not Found",
                                                    "documentation_url": "https://docs.github.com/rest/reference/users#get-a-user"
                                                })),
                                                ..Default::default()
                                            })),
                                            example: Some(json!({
                                                "message": "Not Found",
                                                "documentation_url": "https://docs.github.com/rest/reference/users#get-a-user"
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
                        summary: Some("Create a repository".to_string()),
                        description: Some("Create a new repository for the authenticated user".to_string()),
                        external_docs: None,
                        operation_id: Some("createRepo".to_string()),
                        parameters: vec![],
                        request_body: Some(RequestBodyOrReference::Item(RequestBody {
                            description: Some("Repository creation data".to_string()),
                            required: true,
                            content: {
                                let mut content = HashMap::new();
                                content.insert("application/json".to_string(), MediaType {
                                    schema: Some(SchemaOrReference::Item(Schema {
                                        r#type: Some("object".to_string()),
                                        description: Some("Repository creation request".to_string()),
                                        properties: {
                                            let mut properties = HashMap::new();
                                            properties.insert("name".to_string(), SchemaOrReference::Item(Schema {
                                                r#type: Some("string".to_string()),
                                                description: Some("Repository name".to_string()),
                                                min_length: Some(1),
                                                max_length: Some(100),
                                                pattern: Some("^[a-zA-Z0-9._-]+$".to_string()),
                                                example: Some(json!("my-awesome-repo")),
                                                ..Default::default()
                                            }));
                                            properties.insert("description".to_string(), SchemaOrReference::Item(Schema {
                                                r#type: Some("string".to_string()),
                                                description: Some("Repository description".to_string()),
                                                max_length: Some(350),
                                                example: Some(json!("This is an awesome repository")),
                                                ..Default::default()
                                            }));
                                            properties.insert("private".to_string(), SchemaOrReference::Item(Schema {
                                                r#type: Some("boolean".to_string()),
                                                description: Some("Whether the repository is private".to_string()),
                                                default: Some(json!(false)),
                                                example: Some(json!(false)),
                                                ..Default::default()
                                            }));
                                            properties.insert("auto_init".to_string(), SchemaOrReference::Item(Schema {
                                                r#type: Some("boolean".to_string()),
                                                description: Some("Whether to initialize the repository with a README".to_string()),
                                                default: Some(json!(false)),
                                                example: Some(json!(true)),
                                                ..Default::default()
                                            }));
                                            properties
                                        },
                                        required: vec!["name".to_string()],
                                        example: Some(json!({
                                            "name": "my-awesome-repo",
                                            "description": "This is an awesome repository",
                                            "private": false,
                                            "auto_init": true
                                        })),
                                        ..Default::default()
                                    })),
                                    example: Some(json!({
                                        "name": "my-awesome-repo",
                                        "description": "This is an awesome repository",
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
                                                description: Some("Created repository object".to_string()),
                                                properties: {
                                                    let mut properties = HashMap::new();
                                                    properties.insert("id".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("integer".to_string()),
                                                        format: Some("int64".to_string()),
                                                        description: Some("Repository ID".to_string()),
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
                                                        description: Some("Full repository name".to_string()),
                                                        example: Some(json!("octocat/Hello-World")),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("private".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("boolean".to_string()),
                                                        description: Some("Whether the repository is private".to_string()),
                                                        example: Some(json!(false)),
                                                        ..Default::default()
                                                    }));
                                                    properties.insert("html_url".to_string(), SchemaOrReference::Item(Schema {
                                                        r#type: Some("string".to_string()),
                                                        format: Some("uri".to_string()),
                                                        description: Some("Repository URL".to_string()),
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
                                                description: Some("Validation error object".to_string()),
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
                                                        description: Some("List of validation errors".to_string()),
                                                        items: Some(Box::new(SchemaOrReference::Item(Schema {
                                                            r#type: Some("object".to_string()),
                                                            properties: {
                                                                let mut error_props = HashMap::new();
                                                                error_props.insert("resource".to_string(), SchemaOrReference::Item(Schema {
                                                                    r#type: Some("string".to_string()),
                                                                    example: Some(json!("Repository")),
                                                                    ..Default::default()
                                                                }));
                                                                error_props.insert("field".to_string(), SchemaOrReference::Item(Schema {
                                                                    r#type: Some("string".to_string()),
                                                                    example: Some(json!("name")),
                                                                    ..Default::default()
                                                                }));
                                                                error_props.insert("code".to_string(), SchemaOrReference::Item(Schema {
                                                                    r#type: Some("string".to_string()),
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
