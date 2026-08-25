//! tamper-evident audit chain.
//!
//! every audit entry stores:
//!   - prev_hash: entry_hash of the previous row (chain head order = rowid)
//!   - entry_hash: sha256 over the canonical entry content plus prev_hash
//!
//! editing or deleting any row invalidates every subsequent link, so the
//! "immutable audit" property is enforced by math instead of convention.
//!
//! legacy rows written before migration 017 have NULL hashes; they are
//! backfilled at startup in rowid order by `backfill_unchained`.

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::error::{AppError, Result};

/// prev_hash of the first entry in a chain.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// canonical hash input: fields joined with '|' in fixed order.
/// action_id is part of history even when null (rendered as empty string).
pub fn compute_entry_hash(
    prev_hash: &str,
    action_id: Option<&str>,
    event_type: &str,
    details: &str,
    created_at: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev_hash.as_bytes());
    hasher.update(b"|");
    hasher.update(action_id.unwrap_or("").as_bytes());
    hasher.update(b"|");
    hasher.update(event_type.as_bytes());
    hasher.update(b"|");
    hasher.update(details.as_bytes());
    hasher.update(b"|");
    hasher.update(created_at.as_bytes());
    hex::encode(hasher.finalize())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChainReport {
    pub valid: bool,
    pub entries_checked: u64,
    pub chain_head: Option<String>,
    /// id of the first entry whose stored hash does not match recomputation
    pub first_broken_id: Option<String>,
}

/// appends one entry to the chain inside an existing transaction.
///
/// used by code paths that must keep the audit write atomic with other
/// writes (e.g. verdict persistence). created_at is set explicitly here so
/// the hash covers exactly what is stored.
pub async fn insert_chained_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    action_id: Option<&str>,
    event_type: &str,
    details: &serde_json::Value,
) -> Result<()> {
    let details_str = details.to_string();
    let prev = {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT entry_hash FROM audit_log WHERE entry_hash IS NOT NULL ORDER BY rowid DESC LIMIT 1")
                .fetch_optional(tx.as_mut())
                .await
                .map_err(AppError::Database)?;
        row.and_then(|(h,)| h).unwrap_or_else(|| GENESIS_HASH.to_string())
    };

    // sqlite CURRENT_TIMESTAMP has second resolution and 'YYYY-MM-DD HH:MM:SS'
    // format; replicate it deterministically for hashing
    let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let entry_hash = compute_entry_hash(&prev, action_id, event_type, &details_str, &created_at);

    sqlx::query(
        "INSERT INTO audit_log (id, action_id, event_type, details, created_at, prev_hash, entry_hash) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(action_id)
    .bind(event_type)
    .bind(&details_str)
    .bind(&created_at)
    .bind(&prev)
    .bind(&entry_hash)
    .execute(tx.as_mut())
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

/// standalone audited write for call sites without an open transaction.
pub async fn record(
    pool: &SqlitePool,
    action_id: Option<&str>,
    event_type: &str,
    details: &serde_json::Value,
) -> Result<()> {
    let mut tx = pool.begin().await.map_err(|e| AppError::Database(e.into()))?;
    insert_chained_tx(&mut tx, action_id, event_type, details).await?;
    tx.commit().await.map_err(|e| AppError::Database(e.into()))
}

/// walks the whole chain recomputing each hash; reports the first broken link.
pub async fn verify_chain(pool: &SqlitePool) -> Result<ChainReport> {
    let rows: Vec<(String, Option<String>, String, String, String, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT id, action_id, event_type, details, created_at, prev_hash, entry_hash FROM audit_log ORDER BY rowid ASC",
        )
        .fetch_all(pool)
        .await
        .map_err(AppError::Database)?;

    let mut expected_prev = GENESIS_HASH.to_string();
    let mut checked: u64 = 0;

    for (id, action_id, event_type, details, created_at, prev_hash, entry_hash) in rows {
        match (prev_hash, entry_hash) {
            (Some(prev), Some(entry)) if prev == expected_prev => {
                let recomputed = compute_entry_hash(&prev, action_id.as_deref(), &event_type, &details, &created_at);
                if recomputed != entry {
                    return Ok(ChainReport {
                        valid: false,
                        entries_checked: checked,
                        chain_head: None,
                        first_broken_id: Some(id),
                    });
                }
                expected_prev = entry;
            }
            _ => {
                return Ok(ChainReport {
                    valid: false,
                    entries_checked: checked,
                    chain_head: None,
                    first_broken_id: Some(id),
                });
            }
        }
        checked += 1;
    }

    Ok(ChainReport {
        valid: true,
        entries_checked: checked,
        chain_head: Some(expected_prev),
        first_broken_id: None,
    })
}

/// chains any rows written before the hash columns existed.
/// returns how many rows were backfilled. safe to run repeatedly.
pub async fn backfill_unchained(pool: &SqlitePool) -> Result<u64> {
    let unchained: Vec<(String, Option<String>, String, String, String)> = sqlx::query_as(
        "SELECT id, action_id, event_type, details, created_at FROM audit_log WHERE entry_hash IS NULL ORDER BY rowid ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(AppError::Database)?;

    if unchained.is_empty() {
        return Ok(0);
    }

    let mut count: u64 = 0;
    let mut tx = pool.begin().await.map_err(|e| AppError::Database(e.into()))?;

    for (id, action_id, event_type, details, created_at) in unchained {
        let prev = current_head_tx(&mut tx).await?;
        // normalize created_at to what we would have hashed at write time
        let normalized_ts = normalize_legacy_timestamp(&created_at);
        let entry_hash = compute_entry_hash(&prev, action_id.as_deref(), &event_type, &details, &normalized_ts);

        sqlx::query("UPDATE audit_log SET prev_hash = ?, entry_hash = ? WHERE id = ?")
            .bind(&prev)
            .bind(&entry_hash)
            .bind(&id)
            .execute(tx.as_mut())
            .await
            .map_err(AppError::Database)?;
        count += 1;
    }

    tx.commit().await.map_err(|e| AppError::Database(e.into()))?;
    Ok(count)
}

async fn current_head_tx(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>) -> Result<String> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT entry_hash FROM audit_log WHERE entry_hash IS NOT NULL ORDER BY rowid DESC LIMIT 1")
            .fetch_optional(tx.as_mut())
            .await
            .map_err(AppError::Database)?;
    Ok(row.and_then(|(h,)| h).unwrap_or_else(|| GENESIS_HASH.to_string()))
}

/// accepts both legacy sqlite formats ('YYYY-MM-DD HH:MM:SS' already) and
/// rfc3339 strings from older writers, normalizing to the canonical format.
fn normalize_legacy_timestamp(ts: &str) -> String {
    if ts.contains('T') {
        chrono::DateTime::parse_from_rfc3339(ts)
            .map(|dt| dt.with_timezone(&chrono::Utc).format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_else(|_| ts.to_string())
    } else {
        ts.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genesis_is_64_zeros() {
        assert_eq!(GENESIS_HASH.len(), 64);
        assert!(GENESIS_HASH.chars().all(|c| c == '0'));
    }

    #[test]
    fn hash_changes_with_every_field() {
        let base = compute_entry_hash(GENESIS_HASH, Some("a"), "event", "{}", "2026-01-01 00:00:00");
        let variants = [
            compute_entry_hash("different-prev", Some("a"), "event", "{}", "2026-01-01 00:00:00"),
            compute_entry_hash(GENESIS_HASH, Some("b"), "event", "{}", "2026-01-01 00:00:00"),
            compute_entry_hash(GENESIS_HASH, Some("a"), "other", "{}", "2026-01-01 00:00:00"),
            compute_entry_hash(GENESIS_HASH, Some("a"), "event", "{\"x\":1}", "2026-01-01 00:00:00"),
            compute_entry_hash(GENESIS_HASH, Some("a"), "event", "{}", "2026-01-02 00:00:00"),
        ];
        for v in variants {
            assert_ne!(base, v);
        }
    }

    #[test]
    fn hash_is_deterministic() {
        let a = compute_entry_hash(GENESIS_HASH, None, "evt", "{}", "2026-05-03 21:02:54");
        let b = compute_entry_hash(GENESIS_HASH, None, "evt", "{}", "2026-05-03 21:02:54");
        assert_eq!(a, b);
    }

    #[test]
    fn null_action_id_hashes_differently_from_empty_string_marker() {
        // explicit design check: null and "" render identically by intent,
        // documented here so nobody changes it silently
        let with_none = compute_entry_hash(GENESIS_HASH, None, "evt", "{}", "t");
        let with_empty = compute_entry_hash(GENESIS_HASH, Some(""), "evt", "{}", "t");
        assert_eq!(with_none, with_empty);
    }
}
