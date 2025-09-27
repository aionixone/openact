-- OpenAct Multi-Connector Schema
-- Unified schema supporting multiple connector types

-- 1) Auth connections (PRESERVED - AuthFlow core; do not modify)
CREATE TABLE auth_connections (
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

-- 2) Auth connection history (PRESERVED - AuthFlow core; do not modify)
CREATE TABLE auth_connection_history (
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

-- 3) Connections - universal connection configuration (JSON-based)
CREATE TABLE connections (
    trn TEXT PRIMARY KEY,                 -- trn:openact:{tenant}:connection/{connector}/{name}@v{n}
    connector TEXT NOT NULL,              -- http | postgresql | mysql | redis | mongodb | mcp | grpc | sqlite | ...
    name TEXT NOT NULL,                   -- connection name
    config_json TEXT NOT NULL,            -- connector-specific configuration (JSON)
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    version INTEGER DEFAULT 1
);

-- 4) Actions - universal action configuration (replaces legacy tasks)
CREATE TABLE actions (
    trn TEXT PRIMARY KEY,                 -- trn:openact:{tenant}:action/{connector}/{name}@v{n}
    connector TEXT NOT NULL,              -- must match the connector of the referenced connection
    name TEXT NOT NULL,                   -- action name
    connection_trn TEXT NOT NULL,         -- references a row in connections via TRN
    config_json TEXT NOT NULL,            -- action-specific configuration (JSON)
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    version INTEGER DEFAULT 1,
    FOREIGN KEY (connection_trn) REFERENCES connections(trn) ON DELETE CASCADE
);

-- 5) Run checkpoints - AuthFlow runtime checkpoints (for resume)
CREATE TABLE run_checkpoints (
    run_id TEXT PRIMARY KEY,
    paused_state TEXT NOT NULL,
    context_json TEXT NOT NULL,
    await_meta_json TEXT,
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    expires_at DATETIME
);

-- Indexes
-- AuthFlow-related indexes (preserved)
CREATE INDEX idx_auth_connections_tenant_provider ON auth_connections(tenant, provider);
CREATE INDEX idx_auth_connections_expires_at ON auth_connections(expires_at);

-- New table indexes
CREATE UNIQUE INDEX idx_connections_trn ON connections(trn);
CREATE UNIQUE INDEX idx_connections_connector_name ON connections(connector, name);
CREATE INDEX idx_connections_created_at ON connections(created_at);

CREATE UNIQUE INDEX idx_actions_trn ON actions(trn);
CREATE INDEX idx_actions_connector_conn ON actions(connector, connection_trn);
CREATE UNIQUE INDEX idx_actions_conn_name ON actions(connection_trn, name);
CREATE INDEX idx_actions_connector_name ON actions(connector, name);
CREATE INDEX idx_actions_created_at ON actions(created_at);

CREATE INDEX idx_run_checkpoints_created_at ON run_checkpoints(created_at);
CREATE INDEX idx_run_checkpoints_expires_at ON run_checkpoints(expires_at);

-- Automatic timestamp triggers
CREATE TRIGGER update_connections_timestamp
AFTER UPDATE ON connections
FOR EACH ROW
BEGIN
    UPDATE connections SET updated_at = CURRENT_TIMESTAMP WHERE trn = NEW.trn;
END;

CREATE TRIGGER update_actions_timestamp
AFTER UPDATE ON actions
FOR EACH ROW
BEGIN
    UPDATE actions SET updated_at = CURRENT_TIMESTAMP WHERE trn = NEW.trn;
END;

CREATE TRIGGER update_run_checkpoints_timestamp
AFTER UPDATE ON run_checkpoints
FOR EACH ROW
BEGIN
    UPDATE run_checkpoints SET updated_at = CURRENT_TIMESTAMP WHERE run_id = NEW.run_id;
END;

-- Data integrity constraints
CREATE TRIGGER validate_action_connection_consistency
BEFORE INSERT ON actions
FOR EACH ROW
WHEN (SELECT connector FROM connections WHERE trn = NEW.connection_trn) != NEW.connector
BEGIN
    SELECT RAISE(ABORT, 'Action connector must match connection connector');
END;