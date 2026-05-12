-- Migration 013: Add execute_at for scheduled actions
ALTER TABLE actions ADD COLUMN execute_at TEXT DEFAULT NULL;
