-- Migration 009: Add scheduled activation/deactivation for policies
ALTER TABLE policies ADD COLUMN activate_at TEXT DEFAULT NULL;
ALTER TABLE policies ADD COLUMN deactivate_at TEXT DEFAULT NULL;
