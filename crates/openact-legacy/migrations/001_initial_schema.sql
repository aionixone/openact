-- Initial schema migration
-- Creates all core tables for openact

-- Auth connections table for OAuth tokens
CREATE TABLE IF NOT EXISTS auth_connections (
    trn TEXT PRIMARY KEY,
    tenant TEXT NOT NULL,
    provider TEXT NOT NULL,
    user_id TEXT NOT NULL,
    access_token_encrypted TEXT NOT NULL,
    access_token_nonce TEXT NOT NULL,
    refresh_token_encrypted TEXT,
    refresh_token_nonce TEXT,
    expires_at DATETIME,
    token_type TEXT DEFAULT 'Bearer',
    scope TEXT,
    extra_data_encrypted TEXT,
    extra_data_nonce TEXT,
    key_version INTEGER DEFAULT 1,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    version INTEGER DEFAULT 1
);

-- Auth connection history for audit trail
CREATE TABLE IF NOT EXISTS auth_connection_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    trn TEXT NOT NULL,
    operation TEXT NOT NULL,
    old_data_encrypted TEXT,
    old_data_nonce TEXT,
    new_data_encrypted TEXT,
    new_data_nonce TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    reason TEXT,
    FOREIGN KEY (trn) REFERENCES auth_connections(trn) ON DELETE CASCADE
);

-- Connection configurations
CREATE TABLE IF NOT EXISTS connections (
    trn TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    authorization_type TEXT NOT NULL,
    auth_params_encrypted TEXT NOT NULL,
    auth_params_nonce TEXT NOT NULL,
    auth_ref TEXT,
    default_headers_json TEXT,
    default_query_params_json TEXT,
    default_body_json TEXT,
    network_config_json TEXT,
    timeout_config_json TEXT,
    http_policy_json TEXT,
    key_version INTEGER DEFAULT 1,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    version INTEGER DEFAULT 1
);

-- Task configurations
CREATE TABLE IF NOT EXISTS tasks (
    trn TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    connection_trn TEXT NOT NULL,
    api_endpoint TEXT NOT NULL,
    method TEXT NOT NULL,
    headers_json TEXT,
    query_params_json TEXT,
    request_body_json TEXT,
    timeout_config_json TEXT,
    network_config_json TEXT,
    http_policy_json TEXT,
    response_policy_json TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    version INTEGER DEFAULT 1,
    FOREIGN KEY (connection_trn) REFERENCES connections (trn) ON DELETE CASCADE
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_auth_connections_tenant_provider ON auth_connections(tenant, provider);
CREATE INDEX IF NOT EXISTS idx_auth_connections_expires_at ON auth_connections(expires_at);
CREATE INDEX IF NOT EXISTS idx_connections_authorization_type ON connections(authorization_type);
CREATE INDEX IF NOT EXISTS idx_connections_auth_ref ON connections(auth_ref);
CREATE INDEX IF NOT EXISTS idx_connections_name ON connections(name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_connections_trn ON connections(trn);
CREATE INDEX IF NOT EXISTS idx_tasks_connection_trn ON tasks(connection_trn);
CREATE INDEX IF NOT EXISTS idx_tasks_method ON tasks(method);
CREATE INDEX IF NOT EXISTS idx_tasks_name ON tasks(name);
