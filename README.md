# OpenAct

A simple, powerful, and unified API client solution based on AWS Step Functions HTTP Task design principles.

## Quick Start

### 1. Environment Setup

```bash
# Clone the repository
git clone <repo-url>
cd openact

# Copy environment configuration
cp .env.example .env

# Create data directory
mkdir -p data
```

### 2. Start the Server

```bash
# Start HTTP API server
RUST_LOG=info OPENACT_DB_URL=sqlite:./data/openact.db?mode=rwc \
cargo run --features server --bin openact

# Start server with OpenAPI documentation
RUST_LOG=info OPENACT_DB_URL=sqlite:./data/openact.db?mode=rwc \
cargo run --features server,openapi --bin openact
```

The server will start at `http://127.0.0.1:8080`.

### 📚 API Documentation

With the `openapi` feature enabled, you can access interactive API documentation:

- **Swagger UI**: `http://127.0.0.1:8080/docs`
- **OpenAPI JSON**: `http://127.0.0.1:8080/api-docs/openapi.json`

The API documentation includes complete endpoint descriptions, request/response examples, and authentication information.

### 3. Basic Usage

#### Create Connection Configuration

```bash
# API Key authentication example
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

# Create connection
curl -X POST http://127.0.0.1:8080/api/v1/connections \
  -H "Content-Type: application/json" \
  -d @github_connection.json
```

#### Create Task Configuration

```bash
# Create a task to fetch user information
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

# Create task
curl -X POST http://127.0.0.1:8080/api/v1/tasks \
  -H "Content-Type: application/json" \
  -d @github_user_task.json
```

#### Execute Task

```bash
# Execute using HTTP API
curl -X POST "http://127.0.0.1:8080/api/v1/tasks/trn%3Aopenact%3Ademo%3Atask%2Fgithub-user%40v1/execute" \
  -H "Content-Type: application/json" \
  -d '{}'

# Or use CLI
cargo run --bin openact-cli -- execute "trn:openact:demo:task/github-user@v1"

# Or use CLI in server mode (proxy to HTTP API)
cargo run --bin openact-cli -- --server http://127.0.0.1:8080 execute "trn:openact:demo:task/github-user@v1"
```

## Authentication Types Support

### 1. API Key Authentication

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

### 2. Basic Authentication

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

### 4. OAuth2 Authorization Code (Complex Flow)

For OAuth2 flows that require user authorization, supports complete authorization code flow.

## CLI Usage

### Connection Management

```bash
# List all connections
openact-cli connection list

# Create connection
openact-cli connection upsert connection.json

# Get connection details
openact-cli connection get "trn:openact:demo:connection/github@v1"

# Delete connection
openact-cli connection delete "trn:openact:demo:connection/github@v1"
```

### Task Management

```bash
# List all tasks
openact-cli task list

# Create task
openact-cli task upsert task.json

# Get task details
openact-cli task get "trn:openact:demo:task/github-user@v1"

# Execute task
openact-cli execute "trn:openact:demo:task/github-user@v1"
```

### System Management

```bash
# View system status
openact-cli system stats

# Clean up expired data
openact-cli system cleanup
```

## Advanced Features

### 🔄 Real-time Event Subscription (WebSocket)

OpenAct supports real-time subscription to AuthFlow execution events via WebSocket:

```javascript
// Connect to WebSocket
const ws = new WebSocket('ws://127.0.0.1:8080/ws');

ws.onopen = () => {
    console.log('Connected to OpenAct events');
};

ws.onmessage = (event) => {
    const data = JSON.parse(event.data);
    console.log('Event received:', data);
    
    // Handle different event types
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

**Event Type Examples**:
- `execution_state_change`: Execution state changes
- `workflow_completed`: Workflow completion
- `error_occurred`: Error occurrence

### HTTP Policy Configuration

HTTP policies can be configured at connection or task level:

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

### Network Configuration

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

### Timeout Configuration

```json
{
  "timeout_config": {
    "connect_ms": 10000,
    "read_ms": 30000,
    "total_ms": 60000
  }
}
```

## TRN (Tenant Resource Name) Format

OpenAct uses TRN to uniquely identify resources:

```
trn:openact:{tenant}:{resource_type}/{resource_id}
```

Examples:
- `trn:openact:demo:connection/github@v1`
- `trn:openact:demo:task/github-user@v1`
- `trn:openact:prod:connection/slack-webhook@v2`

## Development and Debugging

### Local Development

```bash
# Run tests
cargo test

# Run specific test
cargo test test_trn_validation

# Run server (development mode)
RUST_LOG=debug cargo run --features server --bin openact
```

### Environment Variables

Refer to the `.env.example` file for all configurable environment variables.

## Architecture Design

- **Connection Layer**: Manages authentication information and network configuration
- **Task Layer**: Defines specific API call logic  
- **Execution Layer**: Handles HTTP requests, authentication injection, retries, etc.
- **Storage Layer**: SQLite database stores configuration and state

## Operations Guide

### System Monitoring

#### Health Check Endpoints

```bash
# Basic health check (no authentication required)
curl http://localhost:8080/api/v1/system/health

# Detailed health information  
curl http://localhost:8080/health
```

#### System Statistics

```bash
# Get detailed system statistics
curl -H "X-API-Key: your-api-key" \
     http://localhost:8080/api/v1/system/stats
```

Information returned includes:
- Database connections, tasks, authentication connections count
- Cache hit rate statistics
- HTTP client pool status
- Memory usage

#### Prometheus Metrics (requires metrics feature)

```bash
# Start server with metrics
cargo run --features server,openapi,metrics --bin openact

# Get Prometheus format metrics
curl -H "X-API-Key: your-api-key" \
     http://localhost:8080/api/v1/system/metrics
```

### Troubleshooting

#### Common Issues Diagnosis

**1. Database Connection Issues**
```bash
# Check database file permissions
ls -la data/openact.db

# Check database integrity
sqlite3 data/openact.db "PRAGMA integrity_check;"
```

**2. Authentication Issues**
```bash
# Verify connection status
curl -H "X-API-Key: your-api-key" \
     "http://localhost:8080/api/v1/connections/{trn}/status"

# Test connection
curl -X POST -H "X-API-Key: your-api-key" \
     "http://localhost:8080/api/v1/connections/{trn}/test"
```

**3. Performance Issues**
```bash
# View client pool status
curl -H "X-API-Key: your-api-key" \
     http://localhost:8080/api/v1/system/stats | jq '.client_pool'

# System cleanup (clear expired authentications)
curl -X POST -H "X-API-Key: your-api-key" \
     http://localhost:8080/api/v1/system/cleanup
```

#### Logging Configuration

```bash
# Debug level logging
RUST_LOG=debug cargo run --features server --bin openact

# JSON format logging (recommended for production)
OPENACT_LOG_JSON=true RUST_LOG=info cargo run --features server --bin openact

# Module-specific logging
RUST_LOG=openact::executor=debug,openact::auth=trace cargo run --features server --bin openact
```

#### Environment Variables Reference

| Variable Name | Default Value | Description |
|---------------|---------------|-------------|
| `OPENACT_DB_URL` | `sqlite:./data/openact.db?mode=rwc` | Database connection URL |
| `OPENACT_MASTER_KEY` | Required | 64-character hexadecimal master key |
| `OPENACT_LOG_JSON` | `false` | Enable JSON format logging |
| `OPENACT_METRICS_ENABLED` | `false` | Enable Prometheus metrics |
| `OPENACT_METRICS_ADDR` | `127.0.0.1:9090` | Metrics service listen address |
| `RUST_LOG` | `info` | Logging level |

### OpenAPI Documentation Usage

With OpenAPI feature enabled, you can access:

- **Swagger UI**: http://localhost:8080/docs
- **OpenAPI JSON**: http://localhost:8080/api-docs/openapi.json

Documentation includes:
- Complete documentation for 27 API endpoints
- Detailed request/response examples
- Error handling guidelines and resolution hints
- Authentication configuration instructions

### Docker Deployment (Recommended)

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
# Build image
docker build -t openact .

# Run container
docker run -p 8080:8080 \
  -e OPENACT_MASTER_KEY=your-64-char-key \
  -e OPENACT_LOG_JSON=true \
  -v ./data:/app/data \
  openact
```

## License

MIT License
