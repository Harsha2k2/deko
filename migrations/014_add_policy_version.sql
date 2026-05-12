-- Migration 014: Add version tracking to policies
ALTER TABLE policies ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
