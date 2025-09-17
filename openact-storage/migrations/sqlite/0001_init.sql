-- AuthFlow tokens (preserved)
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

CREATE INDEX IF NOT EXISTS idx_auth_connections_tenant_provider
  ON auth_connections(tenant, provider);

CREATE INDEX IF NOT EXISTS idx_auth_connections_expires_at
  ON auth_connections(expires_at);

-- Optional audit
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

-- OpenAct connections (DB as authority)
CREATE TABLE IF NOT EXISTS openact_connections (
  trn TEXT PRIMARY KEY,
  tenant TEXT NOT NULL,
  provider TEXT NOT NULL,
  name TEXT,
  auth_kind TEXT NOT NULL,
  auth_ref TEXT,
  network_config_json TEXT,
  tls_config_json TEXT,
  http_policy_json TEXT,
  secrets_encrypted TEXT,
  secrets_nonce TEXT,
  key_version INTEGER DEFAULT 1,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  version INTEGER DEFAULT 1,
  FOREIGN KEY (auth_ref) REFERENCES auth_connections(trn) ON DELETE SET NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_openact_connections_trn
  ON openact_connections(trn);

CREATE INDEX IF NOT EXISTS idx_openact_connections_tenant_provider
  ON openact_connections(tenant, provider);

CREATE INDEX IF NOT EXISTS idx_openact_connections_auth_ref
  ON openact_connections(auth_ref);

-- OpenAct tasks (DB as authority)
CREATE TABLE IF NOT EXISTS openact_tasks (
  trn TEXT PRIMARY KEY,
  tenant TEXT NOT NULL,
  connection_trn TEXT NOT NULL,
  api_endpoint TEXT NOT NULL,
  method TEXT NOT NULL,
  headers_json TEXT,
  query_params_json TEXT,
  request_body_json TEXT,
  pagination_json TEXT,
  http_policy_json TEXT,
  response_policy_json TEXT,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  version INTEGER DEFAULT 1,
  FOREIGN KEY (connection_trn) REFERENCES openact_connections(trn) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_openact_tasks_trn
  ON openact_tasks(trn);

CREATE INDEX IF NOT EXISTS idx_openact_tasks_tenant
  ON openact_tasks(tenant);

CREATE INDEX IF NOT EXISTS idx_openact_tasks_connection
  ON openact_tasks(connection_trn);
