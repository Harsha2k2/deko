-- Migration 019: database-level tamper guards for the audit trail
--
-- the hash chain makes edits detectable; these triggers make them refused.
-- updates are allowed only while an entry has no hash yet (the startup
-- backfill filling legacy rows); once chained, the row is frozen.
-- deleting history is refused unconditionally.
--
-- a dba can still DROP these triggers explicitly — but that is a loud,
-- deliberate act against a named object, not a quiet UPDATE.

CREATE TRIGGER IF NOT EXISTS audit_log_guard_update
BEFORE UPDATE ON audit_log
WHEN OLD.entry_hash IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'audit_log is append-only: chained entries cannot be modified');
END;

CREATE TRIGGER IF NOT EXISTS audit_log_guard_delete
BEFORE DELETE ON audit_log
BEGIN
    SELECT RAISE(ABORT, 'audit_log is append-only: entries cannot be deleted');
END;
