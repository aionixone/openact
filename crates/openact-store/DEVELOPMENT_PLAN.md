# OpenAct Multi-Connector Development Plan

## Overview
Transform OpenAct from HTTP-only to multi-connector architecture supporting HTTP, PostgreSQL, MySQL, Redis, MongoDB, MCP, etc.

## Architecture Goals
- **Universal Storage**: JSON-based config in unified tables (connections/actions)
- **Dual Configuration**: File-based (YAML/JSON) + Database-based management
- **Pluggable Executors**: Registry pattern for connector-specific execution
- **Backward Compatibility**: Existing AuthFlow + API unchanged

## Development Phases

### Phase A: Storage Foundation
**Goal**: Establish robust storage layer with migration support

- [x] **Schema Design**: Multi-connector tables with JSON config
- [ ] **A1**: Fix openact-store memory implementation compilation
- [ ] **A2**: Add SqlStore: connection pool + migration runner + load 001_schema.sql
- [ ] **A3**: Implement ConnectionStore for SQLite (upsert/get/delete/list_by_connector)
- [ ] **A4**: Implement ActionRepository for SQLite (upsert/get/delete/list_by_connection)
- [ ] **A5**: Implement RunStore for SQLite (put/get/delete)
- [ ] **A6**: Integration tests: temp DB + migration + full CRUD operations

**Deliverable**: Fully functional SQLite storage backend

### Phase B: Configuration Management + HTTP Connector
**Goal**: File-based config + first connector implementation

- [ ] **B1**: Config loader: parse YAML/JSON files → ConnectionRecord/ActionRecord
- [ ] **B2**: ConfigManager: load_from_file(), sync_to_db(), export_from_db()
- [ ] **B3**: Define HTTP connector JSON schemas (connection + action config structs)
- [ ] **B4**: Create executor-http: reqwest-based HTTP action execution
- [ ] **B5**: Variable substitution: ${ENV_VAR} replacement in config files

**File Config Example**:
```yaml
# config/connectors.yaml
connectors:
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
        headers:
          Accept: "application/vnd.github.v3+json"
```

**Deliverable**: File → DB config sync + working HTTP connector

### Phase C: Integration + Tooling
**Goal**: Complete framework with registry, server integration, CLI

- [ ] **C1**: ConnectorRegistry: map connector name → executor factory
- [ ] **C2**: Server adapter: read from store → invoke registry → return results
- [ ] **C3**: Maintain API/DTO compatibility with new storage backend
- [ ] **C4**: CLI commands: migrate, sync, import, export, list
- [ ] **C5**: Documentation: schema overview, file formats, examples, migration guide

**CLI Usage**:
```bash
openact migrate --db sqlite://data.db
openact sync --config config/connectors.yaml --db sqlite://data.db  
openact list connections --connector http
openact export --output backup.yaml
```

**Deliverable**: Production-ready multi-connector system

## Configuration Strategy

### Dual Configuration Sources
1. **File-based** (Development/GitOps):
   - YAML/JSON files in version control
   - Environment variable substitution
   - Batch import/export via CLI

2. **Database-based** (Runtime/API):
   - Dynamic creation via REST API
   - Audit trail and versioning
   - Production environment management

### Configuration Priority
```
CLI args > Environment variables > File config > Database defaults
```

## Key Design Decisions

### 1. Storage Schema
- **Preserve AuthFlow**: auth_connections/auth_connection_history unchanged
- **JSON Configuration**: connector-specific config in single JSON column
- **TRN-based Identity**: trn:openact:{tenant}:{type}/{connector}/{name}@v{version}

### 2. Connector Interface
```rust
trait ActionExecutor {
    async fn execute(&self, action: &ActionRecord, input: Value) -> Result<Value>;
}

trait ConnectorFactory {
    fn create_executor(&self, connection: &ConnectionRecord) -> Box<dyn ActionExecutor>;
}
```

### 3. Migration Strategy
- **No breaking changes**: legacy endpoints work unchanged
- **Incremental adoption**: new connectors added via config files
- **Graceful fallback**: if new storage fails, log error but don't crash

## Success Metrics
- [ ] HTTP connector fully replaces legacy implementation
- [ ] Config file changes sync to DB without service restart
- [ ] New connector types (PostgreSQL/Redis) add with <50 lines of config
- [ ] Existing AuthFlow continues working without code changes
- [ ] Performance: <10ms latency overhead vs current implementation

## Risk Mitigation
- **Database corruption**: Always backup before migration
- **Config validation**: Schema validation for all JSON configs
- **Rollback plan**: Keep legacy tables until Phase C complete
- **Testing**: Every phase has integration tests with real databases
