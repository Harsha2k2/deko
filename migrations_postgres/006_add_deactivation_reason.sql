-- Migration 006: Add deactivation reason to agents
ALTER TABLE agents ADD COLUMN deactivated_reason TEXT DEFAULT NULL;
ALTER TABLE agents ADD COLUMN deactivated_at TEXT DEFAULT NULL;
