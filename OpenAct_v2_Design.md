# OpenAct v2 设计文档

## 概述

OpenAct v2 是基于 AWS Step Functions HTTP Task 设计理念的完全重构版本，整合了现有的 TRN (Tool Resource Name) 系统，提供简单、强大、统一的 API 客户端解决方案。

## 设计目标

### 核心目标
- **简单优先**: 默认简单，复杂功能可选
- **统一认证**: 通过 Connection 统一管理认证
- **清晰分离**: Connection 管理认证，Task 管理业务逻辑
- **TRN 整合**: 保留现有 TRN 系统，提供统一资源标识
- **AWS 验证**: 基于 AWS 成熟的设计模式

### 非目标
- 不追求功能完整性，优先保证核心功能质量
- 不追求向后兼容，专注新架构设计
- 不追求性能极致，优先保证易用性

## 架构设计

### 整体架构

```
┌─────────────────────────────────────┐
│           用户接口层                  │
│  CLI / HTTP / STDIO / Simple API     │
├─────────────────────────────────────┤
│           执行引擎层                  │
│  ┌─────────────┐  ┌─────────────┐    │
│  │ HTTP Client │  │ Task Engine  │    │
│  └─────────────┘  └─────────────┘    │
├─────────────────────────────────────┤
│           连接管理层                  │
│  Connection Manager (类似AWS)         │
├─────────────────────────────────────┤
│           认证提供者层                 │
│  OAuth2 / API Key / Basic Auth       │
└─────────────────────────────────────┘
```

### 核心改进建议

### A. 输入映射简化（核心静态，动态上层）

- 核心引擎不再内置 JSONata/Mapping，所有动态映射由上层 Resolver 处理，向核心传入已解析的静态值。
- MultiValue 采用简单的多字符串值表示。

```rust
pub struct MultiValue {
    pub values: Vec<String>,
}

pub struct HttpParameter {
    pub key: String,
    pub value: String,
}
```

### B. Header/Query 多值与"禁用头"策略

支持多值 Header/Query，并提供保护策略。

```rust
pub struct HttpPolicy {
    pub denied_headers: Vec<String>,        // 默认：["host","content-length","transfer-encoding","expect"]
    pub reserved_headers: Vec<String>,      // 由系统/Connection 注入，如 "authorization"
    pub multi_value_append_headers: Vec<String>, // 如 ["accept","cookie","set-cookie"]
}

// 更新后的 InvocationHttpParameters（数组形式，便于兼容 AWS 配置）
pub struct InvocationHttpParameters {
    pub body_parameters: Vec<HttpParameter>,
    pub header_parameters: Vec<HttpParameter>,
    pub query_string_parameters: Vec<HttpParameter>,
}
```

### C. 超时、重试、节流、熔断与幂等安全

增强的配置选项，提供企业级稳定性。

```rust
pub struct TimeoutConfig {
    pub connect_ms: u64,
    pub read_ms: u64,
    pub total_ms: u64,
}

pub struct RetryConfig {
    pub max_attempts: u32,
    pub backoff_rate: f64,
    pub interval_seconds: u64,
    pub retry_on_status: Vec<u16>,  // 替代 retry_on: ["429","503",...]
    pub retry_on_errors: Vec<String>, // "timeout","io","tls"
    pub jitter_strategy: JitterStrategy,
    pub respect_retry_after: bool,
}

pub struct RateLimitPolicy {
    pub permit_per_second: f64,
    pub burst: u32,
}

pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub recovery_timeout_seconds: u64,
    pub half_open_trial: u32,
}

pub struct SafetyConfig {
    pub idempotency: bool,
}
```

### D. 安全与密钥管理

多租户、审计、轮换支持。

```rust
pub struct SecretRef {
    pub key: String,     // 存储Key
    pub version: String, // 版本
}

pub enum Credential {
    InlineEncrypted(String), // 加密后的密文（兼容导入）
    Secret(SecretRef),       // 指向 Secret Store
}

pub struct OAuthParameters {
    pub client_id: Credential,
    pub client_secret: Credential,
    pub token_url: String,
    pub scope: Option<String>,
    pub redirect_uri: Option<String>,
    pub use_pkce: bool,                // 建议内置
    pub refresh_token: Option<Credential>,
    pub token_cache_ttl_sec: u64,      // Token 内存缓存
}
```

### E. TLS/代理/私网与二进制/流式响应

企业级网络配置支持。

```rust
pub struct TlsConfig {
    pub verify_peer: bool,             // 默认 true
    pub ca_pem: Option<Vec<u8>>,
    pub client_cert_pem: Option<Vec<u8>>, // mTLS
    pub client_key_pem: Option<Vec<u8>>,
    pub server_name: Option<String>,
}

pub struct NetworkConfig {
    pub proxy_url: Option<String>,
    pub tls: Option<TlsConfig>,
}

pub struct ResponsePolicy {
    pub allow_binary: bool,
    pub max_body_bytes: usize, // 例如 8MB
    pub binary_sink_trn: Option<String>, // 若超过阈值或为二进制，将存入 OSS 并返回引用
}
```

### F. 分页/重试/错误分类的"高阶语义"

内置分页和错误处理策略（分页提取表达式由上层 Resolver 负责，核心仅保留简单限制）。

```rust
pub struct PaginationConfig {
    pub max_pages: Option<u32>,
}
```

### G. 可观测性与测试台

类似 AWS TestState 的测试体验。

```rust
pub enum InspectionLevel {
    Info,
    Debug,
    Trace,
}

pub struct TestConfig {
    pub inspection_level: InspectionLevel,
    pub reveal_secrets: bool,
    pub dry_run: bool,
    pub save_examples: bool,
}
```

## 核心概念

#### 1. Connection (连接)
Connection 负责管理认证凭据和网络连接配置，类似 AWS EventBridge Connection。

```rust
pub struct Connection {
    pub trn: String,  // trn:openact:tenant1:connection/github@v1
    pub name: String,
    pub authorization_type: AuthorizationType,
    pub auth_parameters: AuthParameters,  // InvocationHttpParameters 在此内部
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub enum AuthorizationType { ApiKey, OAuth, Basic }

pub struct AuthParameters {
    pub api_key_auth_parameters: Option<ApiKeyAuthParameters>,
    pub oauth_parameters: Option<OAuthParameters>,
    pub basic_auth_parameters: Option<BasicAuthParameters>,
    pub invocation_http_parameters: Option<InvocationHttpParameters>,
}

pub struct ApiKeyAuthParameters {
    pub api_key_name: String,
    pub api_key_value: Credential,
}

pub struct OAuthParameters {
    pub client_id: Credential,
    pub client_secret: Credential,
    pub token_url: String,
    pub scope: Option<String>,
    pub redirect_uri: Option<String>,
    pub use_pkce: bool,
    pub refresh_token: Option<Credential>,
    pub token_cache_ttl_sec: u64,
    pub grant_type: OAuthGrantType,
}

pub enum OAuthGrantType { ClientCredentials, AuthorizationCode }

pub struct BasicAuthParameters {
    pub username: Credential,
    pub password: Credential,
}

pub struct InvocationHttpParameters {
    pub body_parameters: Vec<HttpParameter>,
    pub header_parameters: Vec<HttpParameter>,
    pub query_string_parameters: Vec<HttpParameter>,
}

pub struct HttpParameter {
    pub key: String,
    pub value: String,
}

pub struct ArrayInvocationHttpParameters {
    pub body_parameters: Vec<KeyValueParameter>,
    pub header_parameters: Vec<KeyValueParameter>,
    pub query_string_parameters: Vec<KeyValueParameter>,
}

pub struct KeyValueParameter { pub key: String, pub value: String }
```

#### 2. Task (任务)
Task 定义具体的 API 调用参数（核心为静态值；若需动态由上层 Resolver 预处理后传入）。

```rust
pub struct HttpTask {
    pub trn: String,  // trn:openact:tenant1:task/list-repos@v1
    pub api_endpoint: String,
    pub method: String,
    pub connection_trn: String,  // 引用 Connection 的 TRN
    pub headers: Option<HashMap<String, MultiValue>>,      // 多值：Vec<String>
    pub query_params: Option<HashMap<String, MultiValue>>, // 多值：Vec<String>
    pub request_body: Option<serde_json::Value>,
    pub transform: Option<TransformConfig>,
    pub retry: Option<RetryConfig>,
    pub timeouts: Option<TimeoutConfig>,
    pub rate_limit: Option<RateLimitPolicy>,
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    pub safety: Option<SafetyConfig>,
    pub network: Option<NetworkConfig>,
    pub response_policy: Option<ResponsePolicy>,
    pub pagination: Option<PaginationConfig>,
    pub http_policy: Option<HttpPolicy>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

#### 3. TRN 系统
保留现有 TRN 系统，用于统一资源标识。

```
TRN 格式:
- Connection: trn:openact:{tenant}:connection/{provider}@{version}
- Task: trn:openact:{tenant}:task/{action}@{version}

示例:
- trn:openact:tenant1:connection/github@v1
- trn:openact:tenant1:task/list-repos@v1
```

## 统一数据模型与规则（调整）

- 核心不包含 JSONata/Mapping。所有动态计算（如从输入选择字段、拼接 URL、动态 Method/Headers）由上层 Resolver 完成，产出静态请求数据再交给核心执行。

### 大小写与覆盖规则

#### 头部大小写
- 内部统一为小写键存储
- 发送时保留首字母大写或原样（不影响协议）

#### 合并顺序
1. 先 Task → 再 Connection
2. 由于 `precedence=ConnectionWins`，冲突键最终以 Connection 为准

#### 多值追加
- 在 `multi_value_append_headers` 列表内（如 `accept`, `cookie`）→ 采用追加而不是覆盖
- 其他头部采用覆盖策略

#### 禁用头策略
- **denied_headers**: `["host","content-length","transfer-encoding","expect","authorization"]`
  - `drop_forbidden_headers = true` → 静默丢弃
  - `drop_forbidden_headers = false` → 报错 `Task("forbidden header: ...")`
- **reserved_headers**: 仅系统/认证层注入（如 `authorization`）
  - Task/Connection 写入一律无效（或报错，按策略）

### OAuth2 最小闭环

#### Grant Types
- `ClientCredentials`: 客户端凭据模式
- `AuthorizationCode`: 授权码模式（支持 PKCE）

#### Token 缓存
- **Key**: `connection.trn`
- **存储**: `access_token`, `expires_at`, `refresh_token?`
- **执行前**: 若将过期（如 <60s）→ 先刷新
- **刷新失败**: 视作 `Auth(String)` 错误

#### PKCE 支持
- 当 `use_pkce = true` 且 `AuthorizationCode` 流程时生效
- CLI 提供 `openact oauth begin/complete`
- HTTP 提供 `/oauth/begin|/complete`

#### 加密存储
- `Credential::Secret(SecretRef)` 标配
- CLI 支持 `openact secret put/get`
- 日志一律脱敏

### RateLimit / CircuitBreaker 作用域

#### RateLimit
- **默认作用域**: `(connection_trn, host)` 作为限流桶 key
- **自然限制**: 某个 provider/host 的访问频率

#### CircuitBreaker
- **默认作用域**: `(task_trn)` 为熔断 key
- **可选支持**: `(connection_trn, host)`
- **状态存储**: 进程内内存 + 可选持久化（Phase 2）

### ResponsePolicy 与二进制处理

#### 默认策略
- `allow_binary = false`: 当 content-type 不是 `application/json` 或 `text/*` 时 → 返回 `Task("binary not allowed")`

#### 允许二进制
- `allow_binary = true`:
  - 若 `len(body) <= max_body_bytes` → `ResponseBody::Bytes`
  - 超限 → 写入 OSS（本地 OSS 模块），返回 `{"oss_trn": "...", "size": N, "sha256": "..."}`
- CLI/HTTP 自动打印"二进制摘要"，不 dump 全量

## 数据流设计

### 执行流程

```
1. 用户请求
   ↓
2. TRN 解析
   ↓
3. Task 查找
   ↓
4. Connection 查找
   ↓
5. 参数合并 (Connection > Task)
   ↓
6. 认证处理
   ↓
7. HTTP 请求
   ↓
8. 重试处理
   ↓
9. 响应返回
```

### 参数合并策略

参考 AWS Step Functions 的数据合并策略：

1. **Headers**: Connection + Task，Connection 优先
   - Connection 的 `header_parameters` + Task 的 `headers`
   - 相同 Key 时，Connection 的值优先
   - 支持多个相同 Key 的 Header

2. **Query Parameters**: Connection + Task，Connection 优先
   - Connection 的 `query_string_parameters` + Task 的 `query_params`
   - 相同 Key 时，Connection 的值优先
   - 支持多个相同 Key 的 Query 参数

3. **Request Body**: Connection + Task，Connection 优先
   - Connection 的 `body_parameters` + Task 的 `request_body`
   - 相同 Key 时，Connection 的值优先
   - 支持嵌套对象合并

4. **认证信息**: 仅来自 Connection
   - API Key: 通过 `api_key_name` 和 `api_key_value` 注入
   - OAuth2: 通过 Token 注入到 Authorization Header
   - Basic: 通过 username/password 注入到 Authorization Header

#### 参数合并示例

```json
// Connection 的 InvocationHttpParameters
{
  "AuthParameters": {
    "InvocationHttpParameters": {
      "HeaderParameters": [
        {"Key": "User-Agent", "Value": "OpenAct/1.0"},
        {"Key": "Accept", "Value": "application/json"}
      ],
      "QueryStringParameters": [
        {"Key": "per_page", "Value": "100"}
      ]
    }
  }
}

// Task 的 Parameters
{
  "Parameters": {
    "Headers": {
      "Accept": "application/vnd.github.v3+json",  // 会被 Connection 覆盖
      "X-Custom": "task-header"
    },
    "QueryParameters": {
      "per_page": "50",   // 会被 Connection 覆盖
      "sort": "updated"
    }
  }
}

// 最终合并结果（Connection 优先）
{
  "headers": {
    "User-Agent": "OpenAct/1.0",          // 来自 Connection
    "Accept": "application/json",          // 来自 Connection (覆盖 Task)
    "X-Custom": "task-header"              // 来自 Task
  },
  "query_params": {
    "per_page": "100",                     // 来自 Connection (覆盖 Task)
    "sort": "updated"                      // 来自 Task
  }
}
```

## 配置设计

### Connection 配置

完全遵循 AWS EventBridge Connection 的真实格式：

#### 1. API Key 认证
```json
{
  "trn": "trn:openact:tenant1:connection/api-service@v1",
  "name": "API Service Connection",
  "AuthorizationType": "API_KEY",
  "AuthParameters": {
    "ApiKeyAuthParameters": {
      "ApiKeyName": "X-API-Key",
      "ApiKeyValue": "your_api_key_here"
    },
    "InvocationHttpParameters": {
      "HeaderParameters": [
        {
          "Key": "User-Agent",
          "Value": "OpenAct/1.0"
        },
        {
          "Key": "Accept",
          "Value": "application/json"
        }
      ],
      "QueryStringParameters": [
        {
          "Key": "version",
          "Value": "v1"
        }
      ],
      "BodyParameters": [
        {
          "Key": "source",
          "Value": "openact"
        }
      ]
    }
  }
}
```

#### 2. OAuth2 认证
```json
{
  "trn": "trn:openact:tenant1:connection/github@v1",
  "name": "GitHub API Connection",
  "AuthorizationType": "OAUTH",
  "AuthParameters": {
    "OAuthParameters": {
      "ClientId": "github_client_id",
      "ClientSecret": "github_client_secret",
      "TokenUrl": "https://github.com/login/oauth/access_token",
      "Scope": "user:email,repo:read",
      "UsePKCE": true,
      "GrantType": "authorization_code"
    },
    "InvocationHttpParameters": {
      "HeaderParameters": [
        {
          "Key": "User-Agent",
          "Value": "OpenAct/1.0"
        },
        {
          "Key": "Accept",
          "Value": "application/vnd.github.v3+json"
        }
      ],
      "QueryStringParameters": [
        {
          "Key": "per_page",
          "Value": "100"
        }
      ]
    }
  }
}
```

#### 3. Basic Auth 认证
```json
{
  "trn": "trn:openact:tenant1:connection/basic-service@v1",
  "name": "Basic Auth Service",
  "AuthorizationType": "BASIC",
  "AuthParameters": {
    "BasicAuthParameters": {
      "Username": "admin",
      "Password": "secret123"
    },
    "InvocationHttpParameters": {
      "HeaderParameters": [
        {
          "Key": "Accept",
          "Value": "application/json"
        }
      ]
    }
  }
}
```

### Task 配置

Task 定义具体的 HTTP 请求参数，类似 AWS Step Functions HTTP Task：

#### 1. 简单 GET 请求
```json
{
  "trn": "trn:openact:tenant1:task/list-repos@v1",
  "Name": "List GitHub Repositories",
  "Type": "Http",
  "Resource": "trn:openact:tenant1:connection/github@v1",
  "Parameters": {
    "ApiEndpoint": "https://api.github.com/user/repos",
    "Method": "GET",
    "Headers": {
      "X-Task-Header": "task-specific-value"
    },
    "QueryParameters": {
      "type": "owner",
      "sort": "updated"
    }
  },
  "Retry": {
    "MaxAttempts": 3,
    "BackoffRate": 2,
    "IntervalSeconds": 1,
    "RetryOnStatus": [429, 503, 504]
  },
  "TimeoutSeconds": 60
}
```

#### 2. POST 请求（JSON Body）
```json
{
  "trn": "trn:openact:tenant1:task/create-issue@v1",
  "Name": "Create GitHub Issue",
  "Type": "Http",
  "Resource": "trn:openact:tenant1:connection/github@v1",
  "Parameters": {
    "ApiEndpoint": "https://api.github.com/repos/owner/repo/issues",
    "Method": "POST",
    "RequestBody": {
      "title": "example title",
      "body": "example body",
      "labels": ["bug"]
    }
  }
}
```

#### 3. POST 请求（Form Data）
```json
{
  "trn": "trn:openact:tenant1:task/create-invoice@v1",
  "Name": "Create Stripe Invoice",
  "Type": "Http", 
  "Resource": "trn:openact:tenant1:connection/stripe@v1",
  "Parameters": {
    "ApiEndpoint": "https://api.stripe.com/v1/invoices",
    "Method": "POST",
    "RequestBody": {
      "customer.$": "$.customer_id",
      "description": "Monthly subscription"
    },
    "Transform": {
      "RequestBodyEncoding": "FORM_URLENCODED"
    }
  },
  "Safety": {
    "IdempotencyKey": true
  }
}
```

## API 设计

### 核心 API

```rust
pub struct OpenAct {
    trn_manager: TrnManager,
    connection_manager: ConnectionManager,
    task_engine: TaskEngine,
}

impl OpenAct {
    // 创建实例
    pub fn new() -> Self
    pub fn with_config(config: Config) -> Self
    
    // TRN 管理
    pub fn register_connection(&mut self, connection: Connection) -> Result<()>
    pub fn register_task(&mut self, task: HttpTask) -> Result<()>
    pub fn list_connections(&self, pattern: &str) -> Result<Vec<Connection>>
    pub fn list_tasks(&self, pattern: &str) -> Result<Vec<HttpTask>>
    
    // 执行任务
    pub async fn execute_task(&self, task_trn: &str, input: Value) -> Result<Response>
    pub async fn execute(&self, task: HttpTask, input: Value) -> Result<Response>
}
```

### 简单 API 使用

```rust
// 创建客户端
let mut client = OpenAct::new();

```rust
// 注册 Connection
let connection = Connection::new()
    .trn("trn:openact:tenant1:connection/github@v1")
    .name("GitHub API")
    .authorization_type(AuthorizationType::OAuth)
    .auth_parameters(AuthParameters {
        oauth_parameters: Some(OAuthParameters {
            client_id: "xxx".to_string(),
            client_secret: "xxx".to_string(),
            token_url: "https://github.com/login/oauth/access_token".to_string(),
            scope: Some("user:email,repo:read".to_string()),
            redirect_uri: None,
        }),
        api_key_auth_parameters: None,
        basic_auth_parameters: None,
    })
    .invocation_http_parameters(Some(InvocationHttpParameters {
        header_parameters: vec![
            HttpParameter {
                key: "User-Agent".to_string(),
                value: "OpenAct/1.0".to_string(),
            },
        ],
        query_string_parameters: vec![
            HttpParameter {
                key: "per_page".to_string(),
                value: "100".to_string(),
            },
        ],
        body_parameters: vec![],
    }));

client.register_connection(connection)?;

// 注册 Task
let task = HttpTask::new()
    .trn("trn:openact:tenant1:task/list-repos@v1")
    .endpoint("https://api.github.com/user/repos")
    .method(Method::GET)
    .connection_trn("trn:openact:tenant1:connection/github@v1");

client.register_task(task)?;

// 执行任务
let response = client.execute_task(
    "trn:openact:tenant1:task/list-repos@v1",
    json!({
        "customer_id": "1234567890"
    })
).await?;
```

### 高级 API 使用

```rust
// 更灵活的 API
let task = HttpTask::new()
    .trn("trn:openact:tenant1:task/list-repos@v1")
    .endpoint("https://api.github.com/user/repos")
    .method(Method::GET)
    .connection_trn("trn:openact:tenant1:connection/github@v1")
    .query_param("type", "owner")
    .query_param("sort", "updated")
    .retry(RetryConfig::default());

let response = client.execute(task, input).await?;
```

## 接口设计

### CLI 接口

```bash
# 列出所有 Connection
openact list connections trn:openact:tenant1:connection/*@*

# 列出所有 Task
openact list tasks trn:openact:tenant1:task/*@*

# 执行 Task
openact execute trn:openact:tenant1:task/list-repos@v1 --input '{"customer_id": "123"}'

# 测试 Task（dry-run）
openact test trn:openact:tenant1:task/list-repos@v1 --input '{"customer_id": "123"}' --trace --reveal-secrets

# 注册 Connection
openact register connection trn:openact:tenant1:connection/github@v1 --config github.yaml

# 注册 Task
openact register task trn:openact:tenant1:task/list-repos@v1 --config task.yaml

# 从配置文件注册
openact register --config connections.yaml
openact register --config tasks.yaml

# 批量注册
openact register --config-dir ./configs/

# OAuth 流程
openact oauth begin trn:openact:tenant1:connection/github@v1
openact oauth complete trn:openact:tenant1:connection/github@v1 --code <auth_code>

# 密钥管理
openact secret put github_client_id --value <secret_value>
openact secret get github_client_id
```

### HTTP 接口

```http
# 列出 Connection
GET /api/v1/connections?pattern=trn:openact:tenant1:connection/*@*

# 列出 Task
GET /api/v1/tasks?pattern=trn:openact:tenant1:task/*@*

# 执行 Task
POST /api/v1/execute
{
  "task_trn": "trn:openact:tenant1:task/list-repos@v1",
  "input": {"customer_id": "123"}
}

# 测试 Task（dry-run）
POST /api/v1/test
{
  "task_trn": "trn:openact:tenant1:task/list-repos@v1",
  "input": {"customer_id": "123"},
  "inspection_level": "trace",
  "reveal_secrets": true
}

# 注册 Connection
POST /api/v1/connections
{
  "trn": "trn:openact:tenant1:connection/github@v1",
  "name": "GitHub API",
  "authorization_type": "oauth",
  "auth_parameters": {
    "oauth_parameters": {
      "client_id": {"secret": "github_client_id"},
      "client_secret": {"secret": "github_client_secret"},
      "token_url": "https://github.com/login/oauth/access_token"
    }
  }
}

# 注册 Task
POST /api/v1/tasks
{
  "trn": "trn:openact:tenant1:task/list-repos@v1",
  "api_endpoint": "https://api.github.com/user/repos",
  "method": "GET",
  "connection_trn": "trn:openact:tenant1:connection/github@v1"
}

# OAuth 流程
GET /oauth/begin?connection_trn=trn:openact:tenant1:connection/github@v1
POST /oauth/complete
{
  "connection_trn": "trn:openact:tenant1:connection/github@v1",
  "code": "<auth_code>"
}

# 密钥管理
POST /api/v1/secrets
{
  "key": "github_client_id",
  "value": "<secret_value>"
}

GET /api/v1/secrets/github_client_id
```

### STDIO 接口

```json
// JSON-RPC 格式
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "execute_task",
  "params": {
    "task_trn": "trn:openact:tenant1:task/list-repos@v1",
    "input": {"customer_id": "123"}
  }
}

// 响应
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "status": 200,
    "headers": {...},
    "body": {...}
  }
}
```

## 认证系统（类型修正）

### 认证类型

#### 1. OAuth2
```rust
pub struct OAuthParameters {
    pub client_id: Credential,
    pub client_secret: Credential,
    pub token_url: String,
    pub scope: Option<String>,
    pub redirect_uri: Option<String>,
}
```

#### 2. API Key
```rust
pub struct ApiKeyAuthParameters {
    pub api_key_name: String,    // Header 名称，如 "Authorization"
    pub api_key_value: String,   // API Key 值
}
```

#### 3. Basic Auth
```rust
pub struct BasicAuthParameters {
    pub username: String,
    pub password: String,
}
```

### 认证流程

```
1. 接收请求
   ↓
2. 查找 Connection
   ↓
3. 检查认证状态
   ↓
4. 获取/刷新 Token
   ↓
5. 注入认证信息
   ↓
6. 发送请求
```

## URL 编码处理

### 请求体编码

默认情况下，OpenAct 发送 JSON 格式的请求体。如果 API 提供者需要 form-urlencoded 请求体，可以通过 Transform 配置指定 URL 编码。

### Transform 配置

```rust
pub struct TransformConfig {
    pub request_body_encoding: RequestBodyEncoding,
    pub request_encoding_options: Option<RequestEncodingOptions>,
}

pub enum RequestBodyEncoding {
    None,        // 默认 JSON 格式
    UrlEncoded,  // URL 编码格式
}
```

### 数组编码选项

当使用 URL 编码时，支持以下数组编码格式：

#### 1. INDICES (默认)
重复键并为每个数组项添加索引括号。

```json
{"array": ["a","b","c","d"]}
```
编码为：
```
array[0]=a&array[1]=b&array[2]=c&array[3]=d
```

#### 2. REPEAT
重复键，不添加索引。

```json
{"array": ["a","b","c","d"]}
```
编码为：
```
array=a&array=b&array=c&array=d
```

#### 3. COMMAS
将所有值编码为逗号分隔的列表。

```json
{"array": ["a","b","c","d"]}
```
编码为：
```
array=a,b,c,d
```

#### 4. BRACKETS
重复键并添加空括号表示数组。

```json
{"array": ["a","b","c","d"]}
```
编码为：
```
array[]=a&array[]=b&array[]=c&array[]=d
```

### 使用示例

```yaml
# Task 配置
transform:
  request_body_encoding: "url_encoded"
  request_encoding_options:
    array_format: "indices"

# 请求体
request_body:
  "customer.$": "$.customer_id"
  "description": "Monthly subscription"
  "tags": ["urgent", "billing"]
  "metadata":
    "order_details": "monthly report data"

# 最终编码结果
# customer=1234567890&description=Monthly%20subscription&tags[0]=urgent&tags[1]=billing&metadata[order_details]=monthly%20report%20data
```

### 注意事项

1. **Content-Type**: 使用 URL 编码时，必须设置 `Content-Type: application/x-www-form-urlencoded`
2. **自动编码**: OpenAct 会自动进行 URL 编码
3. **嵌套对象**: 支持嵌套对象的编码
4. **特殊字符**: 特殊字符会自动进行 URL 编码

## 错误处理

### 错误分类

```rust
#[derive(Debug, thiserror::Error)]
pub enum OpenActError {
    #[error("TRN error: {0}")]
    Trn(String),
    
    #[error("Connection error: {0}")]
    Connection(String),
    
    #[error("Task error: {0}")]
    Task(String),
    
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    
    #[error("Auth error: {0}")]
    Auth(String),
    
    #[error("Config error: {0}")]
    Config(String),
}
```

## 实施计划（工程落地导向）

### 阶段 1: TRN 核心 (1-2 天)
- [ ] TRN 解析器
- [ ] TRN 管理器
- [ ] TRN 验证
- [ ] 版本管理

### 阶段 2: 核心架构 + 基础安全 (2-3 天)
- [ ] Connection Manager
- [ ] Task Engine
- [ ] HTTP Client
- [ ] 参数合并逻辑
- [ ] **新增**: TimeoutConfig（先 total_ms）
- [ ] **新增**: HttpPolicy（denylist）
- [ ] **新增**: NetworkConfig（代理 + TLS verify）
- [ ] **新增**: ResponsePolicy（默认禁止二进制）

### 阶段 3: 认证系统 + 密钥管理 (2-3 天)
- [ ] OAuth2 Provider
- [ ] API Key Provider
- [ ] Basic Auth Provider
- [ ] **新增**: Secret Store + OAuth2 刷新 + PKCE
- [ ] **新增**: Token 内存缓存与过期刷新
- [ ] **新增**: Credential 加密存储

### 阶段 4: 接口层 + 测试台 (2-3 天)
- [ ] CLI 接口
- [ ] HTTP 接口
- [ ] STDIO 接口
- [ ] 配置管理
- [ ] **新增**: /test 与 dry-run
- [ ] **新增**: CLI 提供 `openact task test <TRN> --input ... --trace --reveal-secrets`

### 阶段 5: 高级功能 + 稳定性 (2-3 天)
- [ ] **新增**: RateLimit + CircuitBreaker + Idempotency
- [ ] **新增**: Retry respect_retry_after
- [ ] **新增**: JSONata 统一映射
- [ ] **新增**: MultiValue Header/Query 支持
- [ ] **新增**: 分页（可作为 Phase 2）

### 阶段 6: 可观测性 + 脚手架 (1-2 天)
- [ ] 单元测试
- [ ] 集成测试
- [ ] 使用文档
- [ ] API 文档
- [ ] **新增**: 示例与模板做成脚手架（`openact init github` 一键出 connection+task 样例）

## 最小增量修改清单

### 1. JSONata 统一映射
- [ ] 新增 `Mapping`/`JsonataExpr` 结构
- [ ] 在 `api_endpoint`/`headers`/`query`/`request_body`/`output` 使用
- [ ] 替换所有 JsonPath 为 JSONata

### 2. 多值头与合并策略
- [ ] `headers`/`query_params` 类型从 `HashMap<String, String>` → `HashMap<String, MultiValue>`
- [ ] 实现大小写无关的 key 归一化与覆盖/追加策略
- [ ] 实现 `HttpPolicy` 保护列表

### 3. 超时/重试/节流/熔断/幂等
- [ ] 加入 `TimeoutConfig`/`RateLimitPolicy`/`CircuitBreakerConfig`/`SafetyConfig`
- [ ] Task 层可覆盖 Connection 层
- [ ] 执行器按"取最严格/最小上限"处理

### 4. Secret Store 接口
- [ ] `Credential::Secret(SecretRef)` 与 inline 加密
- [ ] 日志/TRACE 统一脱敏
- [ ] CLI 支持 `--reveal-secrets` 仅用于本地调试

### 5. TLS/代理/响应政策
- [ ] 按 `NetworkConfig` 与 `ResponsePolicy` 执行
- [ ] 默认仅 JSON 文本
- [ ] 如允许二进制则落本地 OSS，返回对象 TRN

### 6. 可观测性
- [ ] 增加 `inspection_level`、`dry_run`、`stats` 指标暴露（`/status`）

### 7. 分页（Phase 2）
- [ ] 加 `PaginationConfig` 与一套内置迭代器

## 小处但会极大提升体验

### Method 动态
- [ ] 允许 `"method.$": "<JSONata>"`（例如 GET/POST 动态选择）

### Content-Type 自动化
- [ ] 当 `transform=url_encoded` 自动补足 Content-Type
- [ ] 若用户已设置则尊重 Connection 覆盖规则

### 输入/输出 Schema
- [ ] Task 可选声明 `input_schema` / `output_schema`（JSON Schema）
- [ ] 方便 Agent 与 UI 做表单/校验与类型提示

### 错误消息抛出
- [ ] 若 `error_path` 命中，抛出带"用户可读 message + 原始 status/traceId"的结构化错误

## 技术栈

### 核心依赖
- **HTTP**: reqwest
- **序列化**: serde, serde_json, serde_yaml
- **异步**: tokio
- **错误处理**: anyhow, thiserror
- **日志**: tracing

### 接口依赖
- **CLI**: clap
- **HTTP**: axum
- **STDIO**: JSON-RPC

### 认证依赖
- **OAuth2**: oauth2
- **加密**: base64

## 总结

OpenAct v2 通过整合 AWS Step Functions 的设计理念和现有 TRN 系统，提供了一个简单、强大、统一的 API 客户端解决方案。核心设计原则是简单优先、清晰分离、统一管理，确保用户能够快速上手并灵活扩展功能。

## 未实现项清单（近期迭代）

### 高优先级
- [ ] 实现 HTTP API - Connections CRUD endpoints
- [ ] 实现 HTTP API - Tasks CRUD endpoints
- [ ] 实现 HTTP API - Task 执行端点（POST /api/v1/tasks/{trn}/execute）
- [ ] 在服务器路由中接入新端点及处理器
- [ ] 统一的 API 错误模型与 JSON 响应结构

### 中优先级
- [ ] 实现系统统计与清理端点（/api/v1/system/stats, /api/v1/system/cleanup）
- [ ] CLI 增加 --server 开关，支持通过 HTTP API 执行
- [ ] CLI CRUD/执行命令走 HTTP API
- [ ] API 处理器的单元测试（connections、tasks、execute）
- [ ] CLI⇄HTTP API 端到端集成测试

### 低优先级（企业级能力）
- [ ] HttpExecutor 重试策略（尊重 Retry-After）
- [ ] 每连接限流（rate limiting）
- [ ] 熔断器（circuit breaker）
- [ ] 导出 Client Pool 统计到 API/CLI
- [ ] Connection 支持 TLS 证书/私钥文件路径

### 设计边界澄清
- Authflow 仅用于复杂认证编排（AC/CC、PKCE、回调、刷新）；Task 执行走 Executor/HTTP API/CLI。
