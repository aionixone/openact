# OpenAct

一个简单、强大、统一的 API 客户端解决方案，基于 AWS Step Functions HTTP Task 设计理念。

## 快速开始

### 1. 环境准备

```bash
# 克隆项目
git clone <repo-url>
cd openact

# 复制环境配置
cp .env.example .env

# 创建数据目录
mkdir -p data
```

### 2. 启动服务器

```bash
# 启动 HTTP API 服务器
RUST_LOG=info OPENACT_DB_URL=sqlite:./data/openact.db?mode=rwc \
cargo run --features server --bin openact
```

服务器将在 `http://127.0.0.1:8080` 启动。

### 3. 基本使用

#### 创建连接配置

```bash
# API Key 认证示例
cat > github_connection.json << 'EOF'
{
  "trn": "trn:openact:demo:connection/github@v1",
  "name": "GitHub API",
  "version": 1,
  "authorization_type": "api_key",
  "auth_parameters": {
    "api_key_auth_parameters": {
      "api_key_name": "Authorization",
      "api_key_value": "Bearer ghp_your_token_here"
    }
  },
  "created_at": "2025-01-23T12:00:00Z",
  "updated_at": "2025-01-23T12:00:00Z"
}
EOF

# 创建连接
curl -X POST http://127.0.0.1:8080/api/v1/connections \
  -H "Content-Type: application/json" \
  -d @github_connection.json
```

#### 创建任务配置

```bash
# 创建获取用户信息的任务
cat > github_user_task.json << 'EOF'
{
  "trn": "trn:openact:demo:task/github-user@v1",
  "name": "Get GitHub User",
  "version": 1,
  "connection_trn": "trn:openact:demo:connection/github@v1",
  "api_endpoint": "https://api.github.com/user",
  "method": "GET",
  "headers": {
    "User-Agent": ["openact/1.0"],
    "Accept": ["application/vnd.github.v3+json"]
  },
  "created_at": "2025-01-23T12:00:00Z",
  "updated_at": "2025-01-23T12:00:00Z"
}
EOF

# 创建任务
curl -X POST http://127.0.0.1:8080/api/v1/tasks \
  -H "Content-Type: application/json" \
  -d @github_user_task.json
```

#### 执行任务

```bash
# 使用 HTTP API 执行
curl -X POST "http://127.0.0.1:8080/api/v1/tasks/trn%3Aopenact%3Ademo%3Atask%2Fgithub-user%40v1/execute" \
  -H "Content-Type: application/json" \
  -d '{}'

# 或使用 CLI
cargo run --bin openact-cli -- execute "trn:openact:demo:task/github-user@v1"

# 或使用 CLI 的 server 模式（代理到 HTTP API）
cargo run --bin openact-cli -- --server http://127.0.0.1:8080 execute "trn:openact:demo:task/github-user@v1"
```

## 认证类型支持

### 1. API Key 认证

```json
{
  "authorization_type": "api_key",
  "auth_parameters": {
    "api_key_auth_parameters": {
      "api_key_name": "X-API-Key",
      "api_key_value": "your-api-key"
    }
  }
}
```

### 2. Basic 认证

```json
{
  "authorization_type": "basic",
  "auth_parameters": {
    "basic_auth_parameters": {
      "username": "your-username",
      "password": "your-password"
    }
  }
}
```

### 3. OAuth2 Client Credentials

```json
{
  "authorization_type": "oauth2_client_credentials",
  "auth_parameters": {
    "oauth_parameters": {
      "client_id": "your-client-id",
      "client_secret": "your-client-secret",
      "token_url": "https://api.example.com/oauth/token",
      "scope": "read write"
    }
  }
}
```

### 4. OAuth2 Authorization Code（复杂流程）

用于需要用户授权的 OAuth2 流程，支持完整的授权码流程。

## CLI 使用

### 连接管理

```bash
# 列出所有连接
openact-cli connection list

# 创建连接
openact-cli connection upsert connection.json

# 获取连接详情
openact-cli connection get "trn:openact:demo:connection/github@v1"

# 删除连接
openact-cli connection delete "trn:openact:demo:connection/github@v1"
```

### 任务管理

```bash
# 列出所有任务
openact-cli task list

# 创建任务
openact-cli task upsert task.json

# 获取任务详情
openact-cli task get "trn:openact:demo:task/github-user@v1"

# 执行任务
openact-cli execute "trn:openact:demo:task/github-user@v1"
```

### 系统管理

```bash
# 查看系统状态
openact-cli system stats

# 清理过期数据
openact-cli system cleanup
```

## 高级功能

### HTTP 策略配置

可以在连接或任务级别配置 HTTP 策略：

```json
{
  "http_policy": {
    "denied_headers": ["host", "content-length"],
    "reserved_headers": ["authorization"],
    "multi_value_append_headers": ["accept", "cookie"],
    "drop_forbidden_headers": true,
    "normalize_header_names": true,
    "max_header_value_length": 8192,
    "max_total_headers": 64,
    "allowed_content_types": ["application/json", "text/plain"]
  }
}
```

### 网络配置

```json
{
  "network_config": {
    "proxy_url": "http://proxy.example.com:8080",
    "tls": {
      "verify_peer": true,
      "ca_pem": null,
      "client_cert_pem": null,
      "client_key_pem": null,
      "server_name": null
    }
  }
}
```

### 超时配置

```json
{
  "timeout_config": {
    "connect_ms": 10000,
    "read_ms": 30000,
    "total_ms": 60000
  }
}
```

## TRN (Tenant Resource Name) 格式

OpenAct 使用 TRN 来唯一标识资源：

```
trn:openact:{tenant}:{resource_type}/{resource_id}
```

示例：
- `trn:openact:demo:connection/github@v1`
- `trn:openact:demo:task/github-user@v1`
- `trn:openact:prod:connection/slack-webhook@v2`

## 开发和调试

### 本地开发

```bash
# 运行测试
cargo test

# 运行特定测试
cargo test test_trn_validation

# 运行服务器（开发模式）
RUST_LOG=debug cargo run --features server --bin openact
```

### 环境变量

参考 `.env.example` 文件了解所有可配置的环境变量。

## 架构设计

- **连接层**: 管理认证信息和网络配置
- **任务层**: 定义具体的API调用逻辑  
- **执行层**: 处理HTTP请求、认证注入、重试等
- **存储层**: SQLite 数据库存储配置和状态

## 许可证

[添加许可证信息]
