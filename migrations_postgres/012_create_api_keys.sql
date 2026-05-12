-- Migration 012: Multi-key per agent
CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    label TEXT NOT NULL DEFAULT 'default',
    active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at TEXT DEFAULT NULL,
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
);
