-- Migration 017: tamper-evident audit chain
-- each entry stores the hash of its predecessor plus a hash over its own
-- canonical content; editing or deleting any row breaks every subsequent link.
ALTER TABLE audit_log ADD COLUMN prev_hash TEXT;
ALTER TABLE audit_log ADD COLUMN entry_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_audit_log_created_at ON audit_log(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_log_event_type ON audit_log(event_type);
