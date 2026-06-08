-- Migration 016: Add reasoning_chain column to verdicts
ALTER TABLE verdicts ADD COLUMN reasoning_chain TEXT;
