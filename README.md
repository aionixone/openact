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

# 启动带 OpenAPI 文档的服务器
RUST_LOG=info OPENACT_DB_URL=sqlite:./data/openact.db?mode=rwc \
cargo run --features server,openapi --bin openact
```

服务器将在 `http://127.0.0.1:8080` 启动。

### 📚 API 文档

启用 `openapi` 特性后，可以访问交互式 API 文档：

- **Swagger UI**: `http://127.0.0.1:8080/docs`
- **OpenAPI JSON**: `http://127.0.0.1:8080/api-docs/openapi.json`

API 文档包含完整的端点说明、请求/响应示例和认证信息。

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

### 🔄 实时事件订阅 (WebSocket)

OpenAct 支持通过 WebSocket 实时订阅 AuthFlow 执行事件：

```javascript
// 连接到 WebSocket
const ws = new WebSocket('ws://127.0.0.1:8080/ws');

ws.onopen = () => {
    console.log('Connected to OpenAct events');
};

ws.onmessage = (event) => {
    const data = JSON.parse(event.data);
    console.log('Event received:', data);
    
    // 处理不同类型的事件
    switch (data.type) {
        case 'execution_state_change':
            console.log(`Execution ${data.execution_id} changed from ${data.from_state} to ${data.to_state}`);
            break;
        case 'workflow_completed':
            console.log(`Workflow ${data.workflow_id} completed`);
            break;
    }
};

ws.onerror = (error) => {
    console.error('WebSocket error:', error);
};
```

**事件类型示例**:
- `execution_state_change`: 执行状态变更
- `workflow_completed`: 工作流完成
- `error_occurred`: 错误发生

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

## 运维指南

### 系统监控

#### 健康检查端点

```bash
# 基础健康检查（无需认证）
curl http://localhost:8080/api/v1/system/health

# 详细健康信息  
curl http://localhost:8080/health
```

#### 系统统计

```bash
# 获取详细系统统计
curl -H "X-API-Key: your-api-key" \
     http://localhost:8080/api/v1/system/stats
```

返回信息包括：
- 数据库连接数、任务数、认证连接数
- 缓存命中率统计
- HTTP 客户端池状态
- 内存使用情况

#### Prometheus 指标（需要 metrics feature）

```bash
# 启动带指标的服务器
cargo run --features server,openapi,metrics --bin openact

# 获取 Prometheus 格式指标
curl -H "X-API-Key: your-api-key" \
     http://localhost:8080/api/v1/system/metrics
```

### 故障排除

#### 常见问题诊断

**1. 数据库连接问题**
```bash
# 检查数据库文件权限
ls -la data/openact.db

# 检查数据库完整性
sqlite3 data/openact.db "PRAGMA integrity_check;"
```

**2. 认证问题**
```bash
# 验证连接状态
curl -H "X-API-Key: your-api-key" \
     "http://localhost:8080/api/v1/connections/{trn}/status"

# 测试连接
curl -X POST -H "X-API-Key: your-api-key" \
     "http://localhost:8080/api/v1/connections/{trn}/test"
```

**3. 性能问题**
```bash
# 查看客户端池状态
curl -H "X-API-Key: your-api-key" \
     http://localhost:8080/api/v1/system/stats | jq '.client_pool'

# 系统清理（清理过期认证）
curl -X POST -H "X-API-Key: your-api-key" \
     http://localhost:8080/api/v1/system/cleanup
```

#### 日志配置

```bash
# 调试级别日志
RUST_LOG=debug cargo run --features server --bin openact

# JSON 格式日志（生产环境推荐）
OPENACT_LOG_JSON=true RUST_LOG=info cargo run --features server --bin openact

# 特定模块日志
RUST_LOG=openact::executor=debug,openact::auth=trace cargo run --features server --bin openact
```

#### 环境变量参考

| 变量名 | 默认值 | 说明 |
|--------|--------|------|
| `OPENACT_DB_URL` | `sqlite:./data/openact.db?mode=rwc` | 数据库连接URL |
| `OPENACT_MASTER_KEY` | 必需 | 64位十六进制主密钥 |
| `OPENACT_LOG_JSON` | `false` | 启用JSON格式日志 |
| `OPENACT_METRICS_ENABLED` | `false` | 启用Prometheus指标 |
| `OPENACT_METRICS_ADDR` | `127.0.0.1:9090` | 指标服务监听地址 |
| `RUST_LOG` | `info` | 日志级别 |

### OpenAPI 文档使用

启用 OpenAPI 功能后，可访问：

- **Swagger UI**: http://localhost:8080/docs
- **OpenAPI JSON**: http://localhost:8080/api-docs/openapi.json

文档包含：
- 27个API端点的完整文档
- 详细的请求/响应示例
- 错误处理指南和解决提示
- 认证配置说明

### Docker 部署（推荐）

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release --features server,openapi,metrics

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/openact /usr/local/bin/
EXPOSE 8080
CMD ["openact"]
```

```bash
# 构建镜像
docker build -t openact .

# 运行容器
docker run -p 8080:8080 \
  -e OPENACT_MASTER_KEY=your-64-char-key \
  -e OPENACT_LOG_JSON=true \
  -v ./data:/app/data \
  openact
```

## 许可证

MIT License
