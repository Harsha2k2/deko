-- Migration 007: Add API key expiration to agents
ALTER TABLE agents ADD COLUMN api_key_expires_at TEXT DEFAULT NULL;
