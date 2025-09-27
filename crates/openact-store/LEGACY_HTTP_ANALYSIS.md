# Legacy HTTP Configuration Analysis

## Current HTTP Architecture

### 1. Connection Configuration (connections 表)
```rust
struct ConnectionConfig {
    trn: String,                    // TRN identifier
    name: String,                   // Connection name
    authorization_type: AuthorizationType,  // api_key | basic | oauth2_*
    auth_parameters: AuthParameters,         // Encrypted auth params
    auth_ref: Option<String>,               // Reference to auth_connections
    
    // Connection-level defaults
    invocation_http_parameters: Option<InvocationHttpParameters>, // Default headers/query/body
    network_config: Option<NetworkConfig>,   // Proxy, TLS, connection pool
    timeout_config: Option<TimeoutConfig>,   // connect/read/total timeouts
    http_policy: Option<HttpPolicy>,         // Request/response handling
    retry_policy: Option<RetryPolicy>,       // Retry strategy
}

// Auth types supported
enum AuthorizationType {
    ApiKey,                        // X-API-Key header
    Basic,                         // Basic username:password
    OAuth2ClientCredentials,       // OAuth2 client flow
    OAuth2AuthorizationCode,       // OAuth2 authorization flow
}

// Connection-level HTTP defaults
struct InvocationHttpParameters {
    header_parameters: Vec<HttpParameter>,      // Default headers
    query_string_parameters: Vec<HttpParameter>, // Default query params  
    body_parameters: Vec<HttpParameter>,        // Default body params
}
```

### 2. Action Configuration (legacy tasks → actions)
```rust
struct ActionConfig {
    trn: String,                   // TRN identifier
    name: String,                  // Action name
    connection_trn: String,        // References connection
    api_endpoint: String,          // URL path/endpoint
    method: String,                // GET, POST, PUT, DELETE, etc.
    
    // Action-specific overrides
    headers: Option<HashMap<String, MultiValue>>,         // Override headers
    query_params: Option<HashMap<String, MultiValue>>,    // Override query params
    request_body: Option<serde_json::Value>,              // Request body
    
    // Action-level policy overrides
    timeout_config: Option<TimeoutConfig>,     // Override timeouts
    network_config: Option<NetworkConfig>,     // Override network
    http_policy: Option<HttpPolicy>,           // Override HTTP behavior
    response_policy: Option<ResponsePolicy>,   // Response handling
    retry_policy: Option<RetryPolicy>,         // Override retry strategy
}
```

### 3. Configuration Hierarchy
```
Action Override → Connection Defaults → System Defaults
```

**Merge Logic**:
1. Start with connection-level defaults
2. Apply action-specific overrides
3. Merge headers, query params, timeouts
4. Authentication always comes from connection

### 4. Storage Schema (SQLite)
```sql
-- Connection table (one row per connection)
connections:
  - authorization_type: TEXT          -- Enum string
  - auth_params_encrypted: TEXT       -- Encrypted AuthParameters JSON
  - default_headers_json: TEXT        -- InvocationHttpParameters JSON
  - default_query_params_json: TEXT   -- Query defaults
  - default_body_json: TEXT           -- Body defaults  
  - network_config_json: TEXT         -- NetworkConfig JSON
  - timeout_config_json: TEXT         -- TimeoutConfig JSON
  - http_policy_json: TEXT            -- HttpPolicy JSON
  - retry_policy_json: TEXT           -- RetryPolicy JSON
  - auth_ref: TEXT                    -- Link to auth_connections

-- Action table (NEW: replaces legacy tasks table completely)  
actions:
  - connector: TEXT                   -- 'http' for HTTP actions
  - connection_trn: TEXT              -- FK to connections  
  - config_json: TEXT                 -- All action config in JSON:
                                      -- { "method": "GET", "path": "/user", 
                                      --   "headers": {...}, "query_params": {...},
                                      --   "body": {...}, "timeout": {...} }
```

## Key Insights for New Architecture

### 1. Configuration Structure Must Preserve
- **Connection as Base**: Authentication, defaults, policies
- **Action as Override**: Endpoint-specific customization  
- **Hierarchical Merge**: Action overrides connection defaults

### 2. New JSON Schema Mapping
```yaml
# HTTP Connection Config JSON
{
  "base_url": "https://api.github.com",           # New: explicit base URL
  "authorization": {
    "type": "bearer",                             # Simplified from AuthorizationType
    "token": "${GITHUB_TOKEN}",                   # Direct env var support
    # OR other auth types:
    # "type": "api_key", "header": "X-API-Key", "value": "..."
    # "type": "basic", "username": "...", "password": "..."
    # "type": "oauth2", "client_id": "...", "client_secret": "...", "token_url": "..."
  },
  "defaults": {
    "headers": {"User-Agent": "OpenAct/1.0"},     # Default headers
    "query_params": {"format": "json"},           # Default query params
    "timeout": {"connect_ms": 10000, "read_ms": 30000, "total_ms": 60000},
    "retry": {"max_attempts": 3, "backoff_ms": 1000}
  }
}

# HTTP Action Config JSON  
{
  "method": "GET",                                # HTTP method
  "path": "/user",                                # API endpoint path
  "headers": {"Accept": "application/vnd.github.v3+json"},  # Override headers
  "query_params": {"per_page": 50},               # Override query params
  "body": {"key": "value"},                       # Request body (POST/PUT)
  "timeout": {"total_ms": 120000}                 # Override timeouts
}
```

### 3. Execution Flow Compatibility
```rust
// Current flow:
// 1. Load connection by TRN
// 2. Load action by TRN (legacy: task)
// 3. Merge configs (action overrides connection)
// 4. Apply authentication from connection
// 5. Execute HTTP request with merged config

// New flow (must be identical):
// 1. Load connection by TRN → parse config_json
// 2. Load action by TRN → parse config_json  
// 3. Merge configs (action overrides connection)
// 4. Apply authentication from connection
// 5. Execute HTTP request with merged config
```

### 4. Migration Strategy (Legacy → New)
```sql
-- NEW ARCHITECTURE: No migration needed, fresh start
-- Create new tables directly from 001_initial_schema.sql:

-- 1) Keep auth_connections (unchanged)
-- 2) Create new connections table with JSON config
-- 3) Create new actions table with JSON config (NO tasks table)

-- Data migration from legacy (if needed):
INSERT INTO connections (trn, connector, name, config_json, created_at, updated_at, version)
SELECT 
  trn, 
  'http' as connector,
  name,
  json_object(
    'authorization', json_object('type', authorization_type, ...),
    'defaults', json_object(
      'headers', COALESCE(default_headers_json, '{}'),
      'query_params', COALESCE(default_query_params_json, '{}'),
      'timeout', COALESCE(timeout_config_json, '{}')
    )
  ) as config_json,
  created_at, updated_at, version
FROM legacy_connections;

INSERT INTO actions (trn, connector, name, connection_trn, config_json, created_at, updated_at, version)
SELECT 
  trn,
  'http' as connector, 
  name,
  connection_trn,
  json_object(
    'method', method,
    'path', api_endpoint,
    'headers', COALESCE(headers_json, '{}'),
    'query_params', COALESCE(query_params_json, '{}'),
    'body', COALESCE(request_body_json, '{}')
  ) as config_json,
  created_at, updated_at, version
FROM legacy_tasks;
```

## Compatibility Requirements

1. **Auth Flow Integration**: auth_connections table unchanged
2. **Configuration Hierarchy**: Connection defaults + Action overrides  
3. **HTTP Features**: All current features preserved (proxy, TLS, retry, etc.)
4. **API Compatibility**: Existing DTO structures still work
5. **Variable Substitution**: Environment variable support enhanced
