# OpenAct

A powerful, unified API execution platform built with modern Rust architecture featuring **responsibility separation**, **shared execution core**, and **AuthFlow workflow-based authentication**. Designed for seamless API integration with support for multiple entry points (CLI, REST, MCP) while maintaining consistent execution behavior.

## 🏗️ Architecture Highlights

- **🔧 Responsibility Separation**: Clear separation between configuration, runtime, and connectors
- **⚡ Shared Execution Core**: Unified execution path for all entry points (CLI, REST, MCP)
- **🔐 AuthFlow Workflow Authentication**: Sophisticated workflow-based authentication engine
  - OAuth2 authorization code flow with callback handling
  - Token refresh and management automation
  - Complex multi-step authentication workflows
  - Secure credential storage and injection
- **🔌 Plugin Architecture**: Dynamic connector loading and management
- **🎯 Build Optimization**: Selective compilation with centralized connector control
- **🚀 Performance Focused**: Zero-dependency runtime core with optimized build times
- **📦 Modular Design**: Isolated crates for easy testing and maintenance

## 🚀 Quick Start

### 1. Prerequisites

```bash
# Rust 1.70+ required
rustup update

# Clone the repository
git clone <repo-url>
cd openact
```

### 2. Build System

OpenAct uses a modern **xtask-based build system** with **selective connector compilation**:

```bash
# Build CLI with default connectors
cargo run -p xtask -- build -p openact-cli

# Build server with all connectors
cargo run -p xtask -- build -p openact-server

# Build with specific connectors only (faster builds)
echo '[connectors]
http = true
postgresql = false' > connectors.toml

cargo run -p xtask -- build -p openact-cli
```

### 3. New CLI Commands

OpenAct now provides two powerful execution modes:

#### File-based Configuration
```bash
# Execute using configuration file
./target/debug/openact execute-file \
  --config examples/postgres.yaml \
  --action list_tables \
  --format json \
  --output results.json

# Dry run (validate without executing)
./target/debug/openact execute-file \
  --config config.yaml \
  --action my_action \
  --dry-run
```

#### Inline Configuration
```bash
# Execute with inline JSON configuration
./target/debug/openact execute-inline \
  --config-json '{
    "connections": {
      "api": {
        "kind": "http",
        "base_url": "https://api.github.com",
        "authorization": "bearer_token",
        "token": "${GITHUB_TOKEN}"
      }
    },
    "actions": {
      "get_user": {
        "connection": "api",
        "kind": "http",
        "method": "GET",
        "path": "/user"
      }
    }
  }' \
  --action get_user \
  --format yaml
```

### 4. Server Mode (REST API)

```bash
# Start server with all features (including AuthFlow)
cargo run -p xtask -- build -p openact-server
./target/debug/openact-server --port 8080

# Or legacy method
RUST_LOG=info cargo run --features server,openapi --bin openact
```

### 5. AuthFlow Authentication Setup

For OAuth2 providers like GitHub, use the built-in AuthFlow templates:

```bash
# Start OAuth2 flow for GitHub
curl -X POST http://localhost:8080/auth/oauth2/start \
  -H "Content-Type: application/json" \
  -d '{
    "provider": "github",
    "client_id": "your-github-client-id",
    "scopes": ["user", "repo"]
  }'

# AuthFlow will:
# 1. Generate PKCE challenge
# 2. Redirect to GitHub authorization
# 3. Handle callback with code exchange
# 4. Fetch user information
# 5. Persist encrypted credentials
# 6. Provide ready-to-use authenticated connection
```

## 🔌 Connector Support

### HTTP Connector
```yaml
connections:
  github:
    kind: http
    base_url: https://api.github.com
    authorization: bearer_token
    token: "${GITHUB_TOKEN}"
    
actions:
  get_repos:
    connection: github
    kind: http
    method: GET
    path: /user/repos
    headers:
      Accept: application/vnd.github.v3+json
```

### PostgreSQL Connector
```yaml
connections:
  database:
    kind: postgres
    host: localhost
    port: 5432
    database: mydb
    user: "${DB_USER}"
    password: "${DB_PASSWORD}"
    
actions:
  list_tables:
    connection: database
    kind: postgres
    statement: |
      SELECT table_name 
      FROM information_schema.tables 
      WHERE table_schema = 'public'
```

### Adding New Connectors

The plugin architecture makes adding connectors straightforward:

1. **Implement the connector** in `crates/openact-connectors/src/`
2. **Register the factory** in `crates/openact-plugins/src/lib.rs`
3. **Enable in build** via `connectors.toml`
4. **Build and test** with the new connector

## 🧪 Testing & Validation

### Quick Smoke Test
```bash
# 1-minute validation of core functionality
make test-quick
```

### Comprehensive Testing
```bash
# Full test suite with reporting
make test-all

# Essential tests only
make test-all-quick

# Specific test categories
make test-architecture  # Architecture validation
make test-connectors    # Connector functionality
make test-performance   # Build and execution performance
make test-integration   # End-to-end workflows
```

### Available Test Commands
| Command | Duration | Purpose |
|---------|----------|---------|
| `make test-quick` | ~1 min | Basic smoke test |
| `make test-architecture` | ~3-5 min | Architecture validation |
| `make test-connectors` | ~2-3 min | Connector isolation & functionality |
| `make test-performance` | ~5-10 min | Performance benchmarks |
| `make test-integration` | ~3-5 min | End-to-end scenarios |
| `make test-all` | ~15-20 min | Complete validation with reporting |

## 📊 Performance Benefits

The new architecture provides significant performance improvements:

- **🔥 Faster Builds**: Selective connector compilation reduces build times
- **📦 Smaller Binaries**: Only include required connectors in final binaries
- **⚡ Runtime Efficiency**: Zero-dependency execution core
- **🔄 Incremental Compilation**: Improved build caching and parallelization
- **🧩 Modular Testing**: Independent testing of components

## 🎛️ Configuration Examples

### Basic HTTP API Call
```yaml
version: "1.0"

connections:
  httpbin:
    kind: http
    base_url: https://httpbin.org
    authorization: none

actions:
  get_ip:
    connection: httpbin
    kind: http
    method: GET
    path: /ip
    description: "Get current IP address"
```

### Database Query
```yaml
version: "1.0"

connections:
  postgres_db:
    kind: postgres
    host: localhost
    port: 5432
    database: "${DATABASE_NAME}"
    user: "${DATABASE_USER}"
    password: "${DATABASE_PASSWORD}"

actions:
  user_count:
    connection: postgres_db
    kind: postgres
    statement: "SELECT COUNT(*) as user_count FROM users"
    description: "Count total users"
```

### Multi-Connector Workflow
```yaml
version: "1.0"

connections:
  api_service:
    kind: http
    base_url: https://api.example.com
    authorization: api_key
    api_key: "${API_KEY}"
    
  analytics_db:
    kind: postgres
    host: analytics.example.com
    port: 5432
    database: analytics
    user: "${ANALYTICS_USER}"
    password: "${ANALYTICS_PASSWORD}"

actions:
  fetch_metrics:
    connection: api_service
    kind: http
    method: GET
    path: /metrics/daily
    
  store_metrics:
    connection: analytics_db
    kind: postgres
    statement: |
      INSERT INTO daily_metrics (date, value) 
      VALUES (CURRENT_DATE, $1)
```

## 🔐 Authentication Support

OpenAct features a powerful **AuthFlow workflow-based authentication system** that handles complex authentication scenarios automatically.

### AuthFlow Workflow Engine

The AuthFlow system provides sophisticated authentication workflows:

```yaml
connections:
  github_oauth:
    kind: http
    base_url: https://api.github.com
    authorization: oauth2_authorization_code
    authflow:
      client_id: "${GITHUB_CLIENT_ID}"
      client_secret: "${GITHUB_CLIENT_SECRET}"
      authorization_url: https://github.com/login/oauth/authorize
      token_url: https://github.com/login/oauth/access_token
      callback_url: http://localhost:8080/auth/callback
      scopes: ["user", "repo"]
      # AuthFlow handles the complete OAuth2 flow automatically
```

### AuthFlow Features

- **🎯 State Machine Workflows**: AWS Step Functions-inspired state machine for complex auth flows
- **🔄 Automatic Token Refresh**: Handles token expiration and refresh cycles automatically
- **🌐 OAuth2 Authorization Code Flow**: Complete PKCE-enabled OAuth2 implementation with callback handling
- **🔒 Secure Storage**: Encrypted credential storage with master key encryption
- **📝 Template Engine**: Powerful Jinja2-style templating for dynamic workflow configuration
- **🔧 Multi-step Workflows**: Support for complex authentication sequences with state transitions
- **⚡ Real-time Events**: WebSocket notifications for authentication state changes
- **🛡️ PKCE Support**: Proof Key for Code Exchange for enhanced OAuth2 security

### Supported Authentication Methods

#### Simple Authentication
- **None**: `authorization: none`
- **API Key**: `authorization: api_key` + `api_key: "your-key"`
- **Bearer Token**: `authorization: bearer_token` + `token: "your-token"`
- **Basic Auth**: `authorization: basic` + `username`/`password`

#### OAuth2 Flows (via AuthFlow)
- **Authorization Code**: Full OAuth2 flow with callback handling
- **Client Credentials**: Machine-to-machine authentication
- **Token Refresh**: Automatic token management

#### Advanced AuthFlow Examples

**GitHub OAuth2 Workflow** (Complete State Machine):

AuthFlow uses sophisticated state machine workflows for complex authentication scenarios. Here's the complete GitHub OAuth2 flow:

```json
{
  "version": "1.0",
  "provider": {
    "name": "github",
    "providerType": "oauth2",
    "flows": {
      "OAuth": {
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
              "token_type": "{% result.body.token_type %}"
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
                "Accept": "application/vnd.github+json"
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
              "provider": "github",
              "user_id": "{% $user_login %}",
              "access_token": "{% $access_token %}",
              "refresh_token": "{% $refresh_token %}",
              "token_type": "{% $token_type %}"
            },
            "end": true
          }
        }
      }
    }
  }
}
```

**Simplified YAML Configuration for Users**:
```yaml
connections:
  github:
    kind: http
    base_url: https://api.github.com
    authorization: oauth2_authflow
    authflow:
      provider: github
      client_id: "${GITHUB_CLIENT_ID}"
      client_secret: "${GITHUB_CLIENT_SECRET}"
      scopes: ["user", "repo"]
      # AuthFlow handles the complete workflow automatically

actions:
  get_user_repos:
    connection: github
    kind: http
    method: GET
    path: /user/repos
    # AuthFlow automatically injects valid OAuth2 token
```

**API Key with Custom Headers**:
```yaml
connections:
  custom_api:
    kind: http
    base_url: https://api.example.com
    authorization: api_key
    authflow:
      api_key: "${API_SECRET}"
      header_name: "X-Custom-Auth"
      prefix: "CustomAuth "
```

### AuthFlow REST API

The AuthFlow system provides REST endpoints for authentication management:

```bash
# Start OAuth2 flow
curl -X POST http://localhost:8080/auth/oauth2/start \
  -H "Content-Type: application/json" \
  -d '{
    "provider": "github",
    "client_id": "your-client-id",
    "scopes": ["user", "repo"]
  }'

# Check authentication status
curl http://localhost:8080/auth/status/github

# Refresh tokens
curl -X POST http://localhost:8080/auth/refresh/github
```

### Database Authentication
- **Username/Password**: Standard database credentials
- **Connection String**: Full connection string support
- **Environment Variables**: Secure credential injection with AuthFlow encryption

## 🏢 Architecture Deep Dive

### Core Components

1. **`openact-runtime`**: Connector-agnostic execution engine
   - File and inline configuration parsing
   - Action execution orchestration
   - Environment variable injection
   - Data sanitization

2. **`openact-authflow`**: Workflow-based authentication engine
   - OAuth2 authorization code flow implementation
   - Automatic token refresh and management
   - Secure credential storage with encryption
   - Multi-step authentication workflows
   - Real-time authentication event streaming

3. **`openact-plugins`**: Dynamic plugin registration system
   - Connector factory management
   - Runtime registry building
   - Type-safe plugin interfaces

4. **`openact-connectors`**: Isolated connector implementations
   - HTTP client with full feature support
   - PostgreSQL driver with connection pooling
   - AuthFlow integration for automatic authentication
   - Extensible architecture for new connectors

5. **`openact-config`**: Configuration management
   - YAML/JSON parsing and validation
   - Schema validation and type safety
   - Environment variable resolution
   - AuthFlow configuration support

6. **`openact-store`**: Persistent storage layer
   - Encrypted credential storage
   - Authentication state management
   - Connection and task configuration
   - SQLite backend with optional encryption

7. **`xtask`**: Optimized build system
   - Selective connector compilation
   - Feature flag management
   - Build optimization

### Execution Flow

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Entry Point   │───▶│  Runtime Core    │───▶│    AuthFlow     │───▶│   Connectors    │
│  (CLI/REST)     │    │                  │    │                 │    │                 │
├─────────────────┤    ├──────────────────┤    ├─────────────────┤    ├─────────────────┤
│ • execute-file  │    │ • Config parsing │    │ • OAuth2 flows  │    │ • HTTP client   │
│ • execute-inline│    │ • Action execution│    │ • Token refresh │    │ • PostgreSQL    │
│ • REST endpoint │    │ • Data sanitization│   │ • Secure storage│    │ • Auth injection│
│ • WebSocket     │    │ • Event streaming│    │ • Multi-step auth│   │ • [extensible]  │
└─────────────────┘    └──────────────────┘    └─────────────────┘    └─────────────────┘
                                 │                        │
                                 ▼                        ▼
                       ┌──────────────────┐    ┌─────────────────┐
                       │  Plugin System   │    │ Encrypted Store │
                       │                  │    │                 │
                       ├──────────────────┤    ├─────────────────┤
                       │ • Dynamic loading│    │ • Credentials   │
                       │ • Factory pattern│    │ • Auth state    │
                       │ • Type safety    │    │ • Configuration │
                       └──────────────────┘    └─────────────────┘
```

## 🛠️ Development Guide

### Building from Source
```bash
# Quick development build
cargo run -p xtask -- build -p openact-cli

# Optimized release build
cargo run -p xtask -- build -p openact-cli --release

# Build with specific connectors
echo '[connectors]
http = true
postgresql = true' > connectors.toml
cargo run -p xtask -- build -p openact-cli
```

### Running Tests
```bash
# Unit tests
cargo test

# Integration tests with real databases/APIs
cargo test --features integration-tests

# Architecture validation
./scripts/test_architecture.sh

# Performance benchmarks
./scripts/test_performance.sh
```

### Adding New Connectors

1. **Create connector module**:
   ```bash
   mkdir -p crates/openact-connectors/src/my_connector
   ```

2. **Implement factory pattern**:
   ```rust
   // crates/openact-connectors/src/my_connector/factory.rs
   use openact_core::connection::{Connection, ConnectorFactory};
   
   pub struct MyConnectorFactory;
   
   impl ConnectorFactory for MyConnectorFactory {
       fn create_connection(&self, config: &serde_json::Value) -> Result<Box<dyn Connection>, Box<dyn std::error::Error>> {
           // Implementation
       }
   }
   ```

3. **Register in plugin system**:
   ```rust
   // crates/openact-plugins/src/lib.rs
   #[cfg(feature = "my_connector")]
   registrars.push(Box::new(openact_connectors::my_connector::MyConnectorRegistrar));
   ```

4. **Update build configuration**:
   ```toml
   # connectors.toml
   [connectors]
   my_connector = true
   ```

## 📚 API Documentation

When built with server support, OpenAct provides comprehensive API documentation:

- **Interactive Swagger UI**: `http://localhost:8080/docs`
- **OpenAPI Specification**: `http://localhost:8080/api-docs/openapi.json`

## 🐳 Deployment

### Docker
```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo run -p xtask -- build -p openact-server --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/openact-server /usr/local/bin/
EXPOSE 8080
CMD ["openact-server"]
```

### Environment Variables
| Variable | Default | Description |
|----------|---------|-------------|
| `OPENACT_DB_URL` | `sqlite:./data/openact.db` | Database connection |
| `OPENACT_LOG_LEVEL` | `info` | Logging level |
| `OPENACT_PORT` | `8080` | Server port |

## 🔍 Troubleshooting

### Common Issues

**Build Issues**:
```bash
# Clean and rebuild
cargo clean
cargo run -p xtask -- build -p openact-cli
```

**Performance Issues**:
```bash
# Profile build times
./scripts/test_performance.sh

# Check binary sizes
ls -lh target/debug/openact*
```

**Connector Issues**:
```bash
# Test specific connector
cargo test -p openact-connectors --features http

# Validate configuration
./target/debug/openact execute-file --config config.yaml --action test --dry-run
```

### Getting Help

1. **Run diagnostics**: `make test-quick`
2. **Check logs**: `RUST_LOG=debug ./target/debug/openact ...`
3. **Validate config**: Use `--dry-run` flag
4. **Review test reports**: Generated by `make test-all`

## 🤝 Contributing

We welcome contributions! The modular architecture makes it easy to:

- Add new connectors
- Improve performance
- Enhance testing
- Update documentation

See our architecture tests in `scripts/` for validation guidelines.

## 📄 License

MIT License

---

**OpenAct** - *Simple, powerful, unified API execution platform*