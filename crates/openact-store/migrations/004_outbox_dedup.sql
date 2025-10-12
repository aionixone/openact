-- Deduplication keys for orchestrator outbox events
CREATE TABLE IF NOT EXISTS outbox_dedup_keys (
    key TEXT PRIMARY KEY,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_outbox_dedup_created_at
    ON outbox_dedup_keys(created_at);
