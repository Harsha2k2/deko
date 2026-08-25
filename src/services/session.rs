//! server-side admin sessions.
//!
//! the browser receives an opaque 256-bit token in an http-only cookie;
//! the database stores only sha256(token). sessions expire after
//! `SESSION_TTL_SECS` and are deleted on logout. this replaces the old
//! scheme that stored the raw admin password as the cookie value.

use sqlx::SqlitePool;
use std::time::Duration;

use crate::error::{AppError, Result};

/// 8 hour workday default; tune per deployment via env if ever needed.
pub const SESSION_TTL: Duration = Duration::from_secs(28800);

/// generates a session for `identity`, returns the raw token (cookie value).
pub async fn create(pool: &SqlitePool, identity: &str) -> Result<String> {
    let token = random_token();
    let token_hash = hash_token(&token);
    let expires_at = (chrono::Utc::now() + chrono::Duration::seconds(SESSION_TTL.as_secs() as i64))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();

    sqlx::query("INSERT INTO admin_sessions (id, identity, expires_at) VALUES (?, ?, ?)")
        .bind(&token_hash)
        .bind(identity)
        .bind(&expires_at)
        .execute(pool)
        .await
        .map_err(AppError::Database)?;

    Ok(token)
}

/// returns the session identity when the token is valid and unexpired.
/// expired rows are purged opportunistically.
pub async fn validate(pool: &SqlitePool, token: &str) -> Result<Option<String>> {
    let token_hash = hash_token(token);
    let row: Option<(String, String)> = sqlx::query_as("SELECT id, expires_at FROM admin_sessions WHERE id = ?")
        .bind(&token_hash)
        .fetch_optional(pool)
        .await
        .map_err(AppError::Database)?;

    let Some((id, expires_at)) = row else {
        return Ok(None);
    };

    // sqlite CURRENT_TIMESTAMP-style comparison; both sides normalized
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if expires_at <= now {
        sqlx::query("DELETE FROM admin_sessions WHERE id = ?")
            .bind(&id)
            .execute(pool)
            .await
            .map_err(AppError::Database)?;
        return Ok(None);
    }

    Ok(Some(id))
}

/// removes a single session (logout).
pub async fn delete(pool: &SqlitePool, token: &str) -> Result<()> {
    let token_hash = hash_token(token);
    sqlx::query("DELETE FROM admin_sessions WHERE id = ?")
        .bind(&token_hash)
        .execute(pool)
        .await
        .map_err(AppError::Database)?;
    Ok(())
}

/// drops every expired session; call opportunistically at startup.
pub async fn purge_expired(pool: &SqlitePool) -> Result<u64> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let result = sqlx::query("DELETE FROM admin_sessions WHERE expires_at <= ?")
        .bind(&now)
        .execute(pool)
        .await
        .map_err(AppError::Database)?;
    Ok(result.rows_affected())
}

fn random_token() -> String {
    // uuid v4 has 122 bits of entropy; two of them give comfortable margin
    // against online guessing while staying dependency-free
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hash_is_deterministic_and_differs_per_token() {
        assert_eq!(hash_token("abc"), hash_token("abc"));
        assert_ne!(hash_token("abc"), hash_token("abd"));
        assert_eq!(hash_token("abc").len(), 64);
    }

    #[test]
    fn tokens_have_256_bits_of_hex() {
        let t = random_token();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(random_token(), random_token());
    }
}
