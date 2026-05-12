-- Migration 011: Add webhook_url to agents for per-agent webhooks
ALTER TABLE agents ADD COLUMN webhook_url TEXT DEFAULT NULL;
