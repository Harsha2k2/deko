-- Migration 003: Create verdicts table
CREATE TABLE IF NOT EXISTS verdicts (
    id TEXT PRIMARY KEY,
    action_id TEXT NOT NULL,
    decision TEXT NOT NULL,
    reason TEXT NOT NULL,
    risk_level TEXT NOT NULL,
    policy_matched TEXT,
    llm_raw_response TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (action_id) REFERENCES actions(id)
);
