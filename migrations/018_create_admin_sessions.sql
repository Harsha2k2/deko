-- Migration 018: server-side admin sessions
-- the cookie carries an opaque random token; only its hash is stored so a
-- database leak cannot be replayed as a valid session.
CREATE TABLE IF NOT EXISTS admin_sessions (
    id TEXT PRIMARY KEY,              -- sha256(token)
    identity TEXT NOT NULL,           -- 'password' or oauth email
    created_at TEXT NOT NULL DEFAULT (STRFTIME('%Y-%m-%d %H:%M:%S', 'now')),
    expires_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_admin_sessions_expires ON admin_sessions(expires_at);
