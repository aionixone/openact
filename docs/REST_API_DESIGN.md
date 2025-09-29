## OpenAct REST API Design (for Workflow Integration)

### Overview
- Base path: `/api/v1`
- Purpose: Provide discoverable, callable actions to workflow engines
- Scope: Read-only discovery + action execution (no workflow orchestration here)
- Auth: `Authorization: Bearer <token>` or `X-API-Key: <key>` (implementation-defined)
- Tenant: `X-Tenant: default` (header) or `?tenant=default` (query) — header wins
- Idempotency: optional `Idempotency-Key` header for execute endpoints
- Trace: optional `X-Request-Id` header; echoed back in responses

### Response Envelope
- Success:
```json
{ "success": true, "data": { /* payload */ }, "metadata": { "request_id": "...", "execution_time_ms": 123 } }
```
- Error:
```json
{ "success": false, "error": { "code": "INVALID_INPUT", "message": "...", "details": { } }, "metadata": { "request_id": "..." } }
```

### P0 Endpoints (Minimal Viable)

#### 1) GET /api/v1/kinds
- Description: List supported connector kinds (http, postgres, mysql, redis, ...)
- Response (200):
```json
{
  "success": true,
  "data": {
    "kinds": [
      { "name": "http", "description": "HTTP REST API connector", "category": "web" },
      { "name": "postgres", "description": "PostgreSQL database connector", "category": "database" }
    ]
  },
  "metadata": { "request_id": "..." }
}
```

#### 2) GET /api/v1/actions
- Query: `?kind=http&connection=test-api&q=keyword&page=1&page_size=50`
- Description: List available actions, filterable by kind/connection, with pagination
- Response (200):
```json
{
  "success": true,
  "data": {
    "actions": [
      {
        "name": "httpbin.get",
        "connector": "http",
        "connection": "test-api",
        "description": "GET /get",
        "action_trn": "trn:openact:default:action/http/httpbin.get@v1",
        "mcp_enabled": true,
        "input_schema_digest": "sha256:abcd..."
      }
    ],
    "page": 1,
    "page_size": 50,
    "total": 1
  },
  "metadata": { "request_id": "..." }
}
```

#### 3) GET /api/v1/actions/{action}/schema
- Description: Get input/output schema and examples for a named action
- Response (200):
```json
{
  "success": true,
  "data": {
    "input_schema": {
      "type": "object",
      "properties": {
        "headers": { "type": "object", "additionalProperties": { "type": "string" } },
        "query":   { "type": "object", "additionalProperties": { "type": "string" } },
        "body":    { "type": "object" },
        "path":    { "type": "object", "additionalProperties": { "type": "string" } }
      }
    },
    "output_schema": {
      "type": "object",
      "properties": {
        "status_code": { "type": "integer" },
        "headers":     { "type": "object" },
        "body":        {},
        "execution_time_ms": { "type": "integer" }
      }
    },
    "examples": [
      { "name": "basic-get", "input": { "query": { "user_id": "123" } } }
    ]
  },
  "metadata": { "request_id": "..." }
}
```

#### 4) POST /api/v1/actions/{action}/execute
- Version selection is required when `{action}` is a named form like `connector.action` or `connector/action`:
  - Use query `?version=latest` to select the newest, or `?version=<integer>` to select a specific version.
  - Alternatively, provide a fully qualified `action_trn` (including `@vN`) as the `{action}` path segment.
- Body:
```json
{ "input": { /* connector-specific */ }, "options": { "timeout_ms": 30000, "dry_run": false } }
```
- Notes (HTTP kind merge rules):
  - headers/query/cookies: shallow-merge, precedence `connection < action < input`, `null` deletes key
  - body/auth/timeout: whole-object replacement
  - GET/HEAD ignores body (warning recorded in `metadata.warnings`)
  - `options.dry_run = true` performs validation only and echoes `{ "dry_run": true, "input": ... }` in the response; a warning (`dry_run=true`) is added to `metadata.warnings`
  - `options.timeout_ms` is clamped to the governance timeout; requests exceeding the effective deadline return `TIMEOUT`
- Response (200):
```json
{
  "success": true,
  "data": {
    "result": {
      "status_code": 200,
      "headers": { "content-type": "application/json" },
      "body": { "args": { "user_id": "123" }, "url": "https://httpbin.org/get?user_id=123" }
    }
  },
  "metadata": {
    "request_id": "req_abc",
    "execution_time_ms": 842,
    "action_trn": "trn:openact:default:action/http/httpbin.get@v1"
  }
}
```

Examples (curl)

- Execute by name with explicit version
```
curl -s -X POST \
  "http://127.0.0.1:3000/api/v1/actions/http.get-ip/execute?version=1" \
  -H 'Content-Type: application/json' \
  -d '{"input":{}}' | jq .
```

- Execute by name with latest
```
curl -s -X POST \
  "http://127.0.0.1:3000/api/v1/actions/http.get-ip/execute?version=latest" \
  -H 'Content-Type: application/json' \
  -d '{"input":{}}' | jq .
```

#### 4b) POST /api/v1/execute (by TRN)
- Body:
```json
{ "action_trn": "trn:openact:default:action/http/httpbin.get@v1", "input": {}, "options": { } }
```
- Response: 同上（支持 `dry_run` 与 `timeout_ms` 行为，与路径执行一致）

Examples (curl)
```
curl -s -X POST \
  "http://127.0.0.1:3000/api/v1/execute" \
  -H 'Content-Type: application/json' \
  -d '{"action_trn":"trn:openact:default:action/http/httpbin.get@v1","input":{}}' | jq .
```

---

### P1 Enhancements
- Kinds details & schemas
  - GET /api/v1/kinds/{kind}
  - GET /api/v1/kinds/{kind}/connection-schema
  - GET /api/v1/kinds/{kind}/action-schema
- Connections (read-only)
  - GET /api/v1/connections?kind=http&page=&page_size=
  - GET /api/v1/connections/{connection}
- System
  - GET /api/v1/health
  - GET /api/v1/version
- OpenAPI & Docs
  - GET /openapi.json
  - GET /docs (Swagger UI)

### Error Model
- 400 INVALID_INPUT — schema validation failed / missing parameters
- 404 NOT_FOUND — action/connection not found
- 408 TIMEOUT — execution timed out
- 409 CONFLICT — idempotency conflict
- 429 RATE_LIMITED — governance throttling
- 5xx INTERNAL/UPSTREAM_ERROR — internal failure or upstream connector error

### Governance & Security
- Allow/Deny lists on `{connector}.{action}` patterns
- Max concurrency per server / per tenant (optional)
- Timeout default & caps; per-request override via `options.timeout_ms`
- CORS policy configurable; default deny-all (enable per deployment)

### MCP Interop
- REST actions list aligns with MCP tools/list (names & aliases一致)
- Execute semantics identical to MCP `openact.execute`
- Tenancy & governance are shared between REST & MCP

### Implementation Notes
- Reuse existing `ConnectorRegistry` for discovery & execution
- Resolve action by `{action}` name or explicit `action_trn`
- For HTTP connector, apply the documented merge precedence and null-delete
- Include `input_schema_digest` to help workflow UIs cache schemas

DOC
