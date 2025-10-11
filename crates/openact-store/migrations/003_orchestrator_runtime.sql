-- Orchestrator runtime support: track external command executions and outbox events

CREATE TABLE IF NOT EXISTS orchestrator_runs (
    run_id TEXT PRIMARY KEY,
    command_id TEXT UNIQUE NOT NULL,
    tenant TEXT NOT NULL,
    action_trn TEXT NOT NULL,
    status TEXT NOT NULL,
    phase TEXT,
    trace_id TEXT NOT NULL,
    correlation_id TEXT,
    heartbeat_at DATETIME NOT NULL,
    deadline_at DATETIME,
    status_ttl_seconds INTEGER,
    next_poll_at DATETIME,
    poll_attempts INTEGER NOT NULL DEFAULT 0,
    external_ref TEXT,
    result_json TEXT,
    error_json TEXT,
    metadata_json TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_orchestrator_runs_tenant ON orchestrator_runs (tenant);
CREATE INDEX IF NOT EXISTS idx_orchestrator_runs_next_poll ON orchestrator_runs (next_poll_at);
CREATE INDEX IF NOT EXISTS idx_orchestrator_runs_heartbeat ON orchestrator_runs (heartbeat_at);

CREATE TABLE IF NOT EXISTS orchestrator_outbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT,
    protocol TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at DATETIME NOT NULL,
    last_error TEXT,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    delivered_at DATETIME,
    FOREIGN KEY (run_id) REFERENCES orchestrator_runs(run_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_orchestrator_outbox_next_attempt
    ON orchestrator_outbox (next_attempt_at);
CREATE INDEX IF NOT EXISTS idx_orchestrator_outbox_run
    ON orchestrator_outbox (run_id);
