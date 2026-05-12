-- Migration 010: Add priority to actions for priority queuing
ALTER TABLE actions ADD COLUMN priority INTEGER NOT NULL DEFAULT 5;
