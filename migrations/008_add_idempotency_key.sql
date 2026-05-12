-- Migration 008: Add idempotency key for action deduplication
ALTER TABLE actions ADD COLUMN idempotency_key TEXT DEFAULT NULL;
CREATE UNIQUE INDEX idx_actions_idempotency ON actions(agent_id, idempotency_key) WHERE idempotency_key IS NOT NULL;
