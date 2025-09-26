# OpenAct Multi-Action Architecture Design

> **Version**: 1.0  
> **Date**: 2025-01-23  
> **Status**: Design Phase  
> **Branch**: `feature/multi-action-support`

## 🎯 Overview

This document outlines the architectural design for transforming OpenAct from an HTTP-only action library to a comprehensive multi-action platform supporting various protocols including MCP (Model Context Protocol), databases, messaging systems, and more.

## 📋 Current State Analysis

### Existing Architecture
- **Single Protocol**: HTTP-only actions
- **Monolithic Design**: All functionality bundled together
- **Fixed Schema**: OpenAPI-centric design
- **Limited Extensibility**: Hard to add new action types

### Pain Points
1. Cannot extend to non-HTTP protocols (MCP, gRPC, database operations)
2. Large binary size with unnecessary dependencies
3. Tight coupling between execution engine and HTTP specifics
4. Difficult to support multiple client protocols simultaneously

## 🏗️ New Architecture Design

### Core Principles

1. **Kind-Based Organization**: Each action type (HTTP, PostgreSQL, Redis, MCP) is a separate "Kind"
2. **Conditional Compilation**: Users compile only needed Kinds via feature flags
3. **Dual Manifest System**: Actions support multiple client protocols (OpenAPI + MCP)
4. **Configuration Flexibility**: YAML + Database persistence with environment variable substitution
5. **Plugin Architecture**: Easy to add new Kinds without modifying core

### Three-Layer Architecture

```
Kind (Action Type)
  ├── Connections (Resource Configuration)
  │   ├── Action 1 (Specific Operations)
  │   ├── Action 2
  │   └── Action N
  └── Connection 2
      ├── Action 1
      └── Action 2
```

#### Layer Definitions

**1. Kind Layer** - Protocol/Service Types
- `http` - HTTP API calls
- `postgresql` - PostgreSQL database operations
- `mysql` - MySQL database operations
- `redis` - Redis cache operations
- `mongodb` - MongoDB operations
- `mcp` - Model Context Protocol tools
- `grpc` - gRPC service calls
- `kafka` - Kafka messaging
- `s3` - AWS S3 storage operations
- `elasticsearch` - Elasticsearch search operations

**2. Connection Layer** - Service Instances
- Each Kind can have multiple connections
- Examples:
  - HTTP Kind: `github-api`, `slack-api`, `internal-service`
  - PostgreSQL Kind: `prod-db`, `dev-db`, `analytics-db`
  - Redis Kind: `session-store`, `cache-store`

**3. Action Layer** - Specific Operations
- Each connection supports multiple actions
- Examples:
  - `github-api` connection: `get-user`, `create-issue`, `list-repos`
  - `prod-db` connection: `execute-sql`, `list-tables`, `backup-data`
  - `session-store` connection: `get-session`, `set-session`, `delete-session`

## 📁 Directory Structure

```
src/
├── kinds/
│   ├── mod.rs                    # Core interfaces & registry
│   ├── registry.rs               # Kind registration system
│   │
│   ├── http/                     # #[cfg(feature = "http")]
│   │   ├── mod.rs
│   │   ├── connection.rs         # HTTP connection management
│   │   └── actions/
│   │       ├── mod.rs
│   │       ├── get.rs            # HTTP GET action
│   │       ├── post.rs           # HTTP POST action
│   │       └── request.rs        # Generic HTTP request action
│   │
│   ├── postgresql/               # #[cfg(feature = "postgresql")]
│   │   ├── mod.rs
│   │   ├── connection.rs         # PostgreSQL connection pool
│   │   └── actions/
│   │       ├── mod.rs
│   │       ├── execute_sql.rs    # Execute SQL queries
│   │       ├── list_tables.rs    # List database tables
│   │       └── query.rs          # Parameterized queries
│   │
│   ├── redis/                    # #[cfg(feature = "redis")]
│   │   ├── mod.rs
│   │   ├── connection.rs         # Redis connection pool
│   │   └── actions/
│   │       ├── mod.rs
│   │       ├── get.rs            # Redis GET
│   │       ├── set.rs            # Redis SET
│   │       ├── del.rs            # Redis DEL
│   │       └── pub_sub.rs        # Redis Pub/Sub
│   │
│   ├── mcp/                      # #[cfg(feature = "mcp")]
│   │   ├── mod.rs
│   │   ├── connection.rs         # MCP server connection
│   │   └── actions/
│   │       ├── mod.rs
│   │       ├── call_tool.rs      # Call MCP tool
│   │       ├── get_resource.rs   # Get MCP resource
│   │       └── list_tools.rs     # List available tools
│   │
│   └── grpc/                     # #[cfg(feature = "grpc")]
│       ├── mod.rs
│       ├── connection.rs         # gRPC client connection
│       └── actions/
│           ├── mod.rs
│           └── unary_call.rs     # Unary gRPC calls
```

## 🔧 Core Interfaces

### Kind Registration System

```rust
// src/kinds/mod.rs
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;

/// Kind registry manages all action types
pub struct KindRegistry {
    connection_factories: HashMap<String, Box<dyn ConnectionFactory>>,
    action_factories: HashMap<String, Box<dyn ActionFactory>>,
}

/// Each Kind must implement this trait
pub trait Kind {
    fn kind_name() -> &'static str;
    fn register(registry: &mut KindRegistry);
}

/// Compile-time registration of all enabled Kinds
pub fn register_all_kinds() -> KindRegistry {
    let mut registry = KindRegistry::new();
    
    #[cfg(feature = "http")]
    http::HttpKind::register(&mut registry);
    
    #[cfg(feature = "postgresql")]
    postgresql::PostgreSQLKind::register(&mut registry);
    
    #[cfg(feature = "redis")]
    redis::RedisKind::register(&mut registry);
    
    #[cfg(feature = "mcp")]
    mcp::MCPKind::register(&mut registry);
    
    registry
}
```

### Connection Management

```rust
/// Connection factory for creating connections from config
#[async_trait]
pub trait ConnectionFactory: Send + Sync {
    async fn create_from_yaml(&self, config: &serde_yaml::Value) -> Result<Box<dyn Connection>>;
    async fn create_from_db(&self, config: &ConnectionDbConfig) -> Result<Box<dyn Connection>>;
}

/// Connection trait - all connection types implement this
#[async_trait]
pub trait Connection: Send + Sync {
    fn kind(&self) -> &str;
    fn name(&self) -> &str;
    async fn health_check(&self) -> Result<()>;
    async fn close(&mut self) -> Result<()>;
}
```

### Action Execution

```rust
/// Action factory for creating actions
#[async_trait]
pub trait ActionFactory: Send + Sync {
    async fn create_from_yaml(
        &self, 
        config: &serde_yaml::Value, 
        connection: Box<dyn Connection>
    ) -> Result<Box<dyn Action>>;
    
    async fn create_from_db(
        &self, 
        config: &ActionDbConfig, 
        connection: Box<dyn Connection>
    ) -> Result<Box<dyn Action>>;
}

/// Action trait with dual manifest support
#[async_trait]
pub trait Action: Send + Sync {
    fn kind(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    
    // Execution
    async fn execute(&self, params: ActionParams) -> Result<ActionResult>;
    
    // Dual manifest system for multiple protocols
    fn openapi_schema(&self) -> OpenApiActionSchema;  // For REST API
    fn mcp_manifest(&self) -> McpToolManifest;        // For MCP protocol
    
    // Parameter parsing
    fn parse_params(&self, raw: serde_json::Value) -> Result<ActionParams>;
}
```

## ⚙️ Conditional Compilation

### Feature Flags in Cargo.toml

```toml
[features]
default = ["http", "server"]

# Core features
server = ["axum", "tokio", "tower"]
openapi = ["utoipa", "utoipa-swagger-ui"]
metrics = ["prometheus"]

# Kind features - each Kind is optional
http = ["reqwest", "url"]
postgresql = ["sqlx/postgres", "sqlx/runtime-tokio-rustls"]
mysql = ["sqlx/mysql", "sqlx/runtime-tokio-rustls"]
redis = ["redis-rs", "tokio"]
mongodb = ["mongodb"]
mcp = ["tokio-process", "serde_json", "jsonrpc-core"]
grpc = ["tonic", "prost"]
sqlite = ["sqlx/sqlite", "sqlx/runtime-tokio-rustls"]

# Compound features for convenience
database = ["postgresql", "mysql", "redis", "mongodb", "sqlite"]
cloud = ["grpc"]
messaging = ["kafka", "mcp"]
all-kinds = ["http", "database", "cloud", "messaging"]
```

### Build Examples

```bash
# Minimal build (only HTTP + server)
cargo build --features "http,server"

# Database-focused build
cargo build --features "database,server"

# Full-featured build
cargo build --features "all-kinds,server,openapi,metrics"

# Custom combination
cargo build --features "http,postgresql,redis,mcp,server"

# Embedded/minimal build
cargo build --features "sqlite" --no-default-features
```

## 📄 Configuration System

### YAML Configuration Format

```yaml
# config/kinds.yaml
kinds:
  postgresql:
    connections:
      prod-db:
        host: "localhost"
        port: 5432
        database: "production"
        user: "${POSTGRES_USER}"
        password: "${POSTGRES_PASSWORD}"
        pool_max_size: 10
        
      dev-db:
        host: "localhost"
        port: 5432
        database: "development"
        user: "dev_user"
        password: "dev_pass"
        
    actions:
      execute-sql:
        connection: "prod-db"
        description: "Execute SQL queries on production database"
        allowed_operations: ["SELECT", "INSERT", "UPDATE"]
        
      list-tables:
        connection: "prod-db"
        description: "List all tables in database"

  http:
    connections:
      github-api:
        base_url: "https://api.github.com"
        timeout_ms: 30000
        auth:
          type: "bearer"
          token: "${GITHUB_TOKEN}"
          
    actions:
      get-user:
        connection: "github-api"
        method: "GET"
        path: "/user"
        description: "Get current user information"

  mcp:
    connections:
      claude-mcp:
        server_path: "/usr/local/bin/claude-mcp-server"
        args: ["--config", "/etc/claude-mcp.json"]
        timeout_ms: 60000
        
    actions:
      call-tool:
        connection: "claude-mcp"
        description: "Call any tool available in Claude MCP server"
```

### Database Schema Extension

```sql
-- New tables for Kind-based architecture
CREATE TABLE kind_connections (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trn TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    config_data TEXT NOT NULL, -- JSON configuration
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE kind_actions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trn TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    connection_trn TEXT NOT NULL,
    config_data TEXT NOT NULL, -- JSON configuration
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (connection_trn) REFERENCES kind_connections(trn)
);

CREATE TABLE kind_action_sets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trn TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    action_trns TEXT NOT NULL, -- JSON array of action TRNs
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

### TRN (Tenant Resource Name) Format

```
# New TRN format with Kind specification
trn:openact:{tenant}:connection/{kind}/{name}@v{version}
trn:openact:{tenant}:action/{kind}/{name}@v{version}

# Examples
trn:openact:demo:connection/postgresql/prod-db@v1
trn:openact:demo:action/postgresql/execute-sql@v1
trn:openact:demo:connection/http/github-api@v1
trn:openact:demo:action/http/get-user@v1
trn:openact:demo:connection/mcp/claude-mcp@v1
trn:openact:demo:action/mcp/call-tool@v1
```

## 🌐 Dual Manifest System

### Problem: Multiple Client Protocols

Different clients expect different schemas:
- **REST API clients**: OpenAPI 3.x schemas
- **MCP clients**: MCP protocol tool manifests
- **gRPC clients**: Protocol Buffer definitions

### Solution: Dual Manifest Pattern

```rust
// Each action supports multiple manifest formats
impl Action for PostgreSQLExecuteSQL {
    fn openapi_schema(&self) -> OpenApiActionSchema {
        OpenApiActionSchema {
            summary: "Execute SQL query",
            parameters: vec![
                Parameter {
                    name: "sql",
                    in_: "body",
                    schema: Schema::String { format: None },
                    required: true,
                }
            ],
            responses: hashmap! {
                200 => Response { 
                    description: "Query results",
                    content: json_content_type()
                }
            }
        }
    }
    
    fn mcp_manifest(&self) -> McpToolManifest {
        McpToolManifest {
            name: "postgresql-execute-sql",
            description: "Execute SQL query on PostgreSQL database",
            input_schema: McpSchema {
                type_: "object",
                properties: hashmap! {
                    "sql" => McpProperty {
                        type_: "string",
                        description: "SQL query to execute"
                    }
                },
                required: vec!["sql"]
            }
        }
    }
}
```

### Benefits

1. **Single Source of Truth**: One action definition, multiple client interfaces
2. **Protocol Agnostic**: Add new protocols without changing existing actions
3. **Type Safety**: Compile-time schema validation
4. **Automatic Conversion**: Framework handles protocol-specific serialization

## 🔌 MCP Integration Deep Dive

### Inspiration: GenAI Toolbox Architecture

Based on analysis of Google's GenAI Toolbox, every tool has dual manifests:

```go
// GenAI Toolbox pattern
type Tool interface {
    Invoke(context.Context, ParamValues, AccessToken) (any, error)
    Manifest() Manifest        // For REST API
    McpManifest() McpManifest  // For MCP protocol
    Authorized([]string) bool
}
```

### OpenAct MCP Implementation

```rust
// MCP server automatically exposes all actions as tools
pub struct McpServer {
    registry: Arc<KindRegistry>,
    version: String,
}

impl McpServer {
    pub async fn list_tools(&self) -> Vec<McpToolManifest> {
        self.registry
            .all_actions()
            .iter()
            .map(|action| action.mcp_manifest())
            .collect()
    }
    
    pub async fn call_tool(&self, name: &str, args: JsonValue) -> Result<JsonValue> {
        let action = self.registry.get_action(name)?;
        let params = action.parse_params(args)?;
        let result = action.execute(params).await?;
        Ok(serde_json::to_value(result)?)
    }
}
```

### MCP Protocol Support

- **Version Negotiation**: Support multiple MCP protocol versions
- **Tool Discovery**: Automatic exposure of all actions as MCP tools
- **Real-time Communication**: WebSocket/SSE for tool updates
- **Error Handling**: Proper MCP error responses

## 🚀 Migration Strategy

### Phase 1: Foundation (Week 1-2)
1. ✅ Create new directory structure
2. ✅ Define core traits and interfaces
3. ✅ Implement Kind registration system
4. ✅ Add conditional compilation support

### Phase 2: HTTP Migration (Week 3)
1. Migrate existing HTTP functionality to new `http` Kind
2. Ensure backward compatibility
3. Update tests for new architecture
4. Verify no regression in existing functionality

### Phase 3: Database Support (Week 4-5)
1. Implement `postgresql` Kind with basic actions
2. Implement `mysql` and `redis` Kinds
3. Add database connection pooling
4. Create SQL execution safety mechanisms

### Phase 4: MCP Integration (Week 6-7)
1. Implement MCP connection management
2. Add dual manifest system
3. Create MCP server endpoints
4. Test with MCP clients

### Phase 5: Enhanced Features (Week 8+)
1. Add remaining Kinds (gRPC, Kafka, S3, etc.)
2. Implement action sets/toolsets
3. Add advanced configuration features
4. Performance optimization

### Backward Compatibility

- Keep existing HTTP API endpoints working
- Provide migration utilities for old configurations
- Deprecation warnings with clear upgrade paths
- Documentation for migration process

## 📊 Benefits Analysis

### For Users

**Flexibility**
- Choose only needed action types
- Smaller binary sizes
- Reduced dependency conflicts
- Faster compilation times

**Extensibility**
- Easy to add custom Kinds
- Plugin-like architecture
- Community contributions encouraged
- Future-proof design

**Multi-Protocol Support**
- Use same actions via REST API, MCP, or gRPC
- Consistent behavior across protocols
- Single configuration, multiple interfaces

### For Developers

**Code Organization**
- Clear separation of concerns
- Modular development
- Independent testing
- Parallel development possible

**Maintenance**
- Isolated changes
- Reduced coupling
- Clear ownership boundaries
- Easier debugging

## 🎯 Success Metrics

### Technical Metrics
- **Binary Size Reduction**: 50%+ smaller binaries with minimal features
- **Compilation Time**: Faster builds with fewer dependencies
- **Memory Usage**: Reduced runtime memory footprint
- **Test Coverage**: Maintain >90% coverage across all Kinds

### Functional Metrics
- **Kind Ecosystem**: Support for 10+ different action types
- **Protocol Support**: REST API + MCP + future protocols
- **Configuration Flexibility**: YAML + DB + environment variables
- **Migration Success**: 100% backward compatibility during transition

### Community Metrics
- **Adoption Rate**: Track feature flag usage
- **Contribution Rate**: New Kind contributions from community
- **Documentation Quality**: Clear examples for each Kind
- **Support Load**: Reduced support requests due to better architecture

## 📝 Next Steps

1. **Review & Approval**: Stakeholder review of this design document
2. **Implementation Planning**: Detailed sprint planning for each phase
3. **Prototype Development**: Create minimal working example
4. **Community Feedback**: Gather input from potential users
5. **Implementation Start**: Begin Phase 1 development

---

**Document Status**: Draft for Review  
**Next Review Date**: TBD  
**Implementation Start**: Pending Approval
