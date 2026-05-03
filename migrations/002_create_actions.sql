-- Migration 002: Create actions table
CREATE TABLE IF NOT EXISTS actions (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    intent TEXT NOT NULL,
    payload TEXT,
    screenshot_base64 TEXT,
    metadata TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    target_url TEXT,
    target_method TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (agent_id) REFERENCES agents(id)
);
