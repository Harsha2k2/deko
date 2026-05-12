-- Migration 005: Create audit_log table (immutable)
CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY,
    action_id TEXT,
    event_type TEXT NOT NULL,
    details TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    FOREIGN KEY (action_id) REFERENCES actions(id)
);
