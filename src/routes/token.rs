use axum::{extract::State, Extension, Json};

use crate::error::{AppError, Result};
use crate::middleware::auth::hash_api_key;
use crate::middleware::jwt::create_token;
use crate::config::Config;

type DbPool = crate::db::DbPool;

/// Exchange an API key for a JWT token.
///
/// Accepts `X-API-Key` header (legacy) or `Authorization: Bearer <api_key>` header.
/// Returns a signed JWT valid for the configured duration.
pub async fn exchange_token(
    State(pool): State<DbPool>,
    Extension(config): Extension<Config>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>> {
    let api_key = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| {
            headers
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(|s| s.to_string())
        });

    let api_key = api_key.ok_or_else(|| AppError::Unauthorized("Missing API key".into()))?;

    let hashed = hash_api_key(&api_key, &config.api_key_secret);

    // Try api_keys table first (multi-key support), fallback to agents table
    let agent = match try_find_key(&pool, &hashed).await {
        Ok(Some((id, name))) => Some((id, name)),
        _ => {
            // Fallback to legacy agent key
            sqlx::query_as::<_, (String, String)>(
                "SELECT id, name FROM agents WHERE api_key_hash = ? AND active = 1",
            )
            .bind(&hashed)
            .fetch_optional(&pool)
            .await
            .map_err(AppError::Database)?
        }
    };

    let (agent_id, agent_name) = agent
        .ok_or_else(|| AppError::Unauthorized("Invalid API key".into()))?;

    let token = create_token(&agent_id, &agent_name, &config.jwt_secret, config.jwt_expiry_secs)
        .map_err(|_| AppError::Internal)?;

    Ok(Json(serde_json::json!({
        "token": token,
        "expires_in": config.jwt_expiry_secs,
    })))
}

async fn try_find_key(pool: &DbPool, hashed: &str) -> Result<Option<(String, String)>> {
    // Check if api_keys table exists first
    let table_exists: (i64,) = match sqlx::query_as(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='api_keys'"
    )
    .fetch_one(pool)
    .await
    {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    if table_exists.0 == 0 {
        return Ok(None);
    }

    let result = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT k.agent_id, ag.name, k.expires_at FROM api_keys k JOIN agents ag ON k.agent_id = ag.id WHERE k.key_hash = ? AND k.active = 1 AND ag.active = 1",
    )
    .bind(hashed)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Database)?;

    match result {
        Some((id, name, expires_at)) => {
            if let Some(expires) = expires_at {
                if let Ok(expires_dt) = chrono::DateTime::parse_from_rfc3339(&expires) {
                    if expires_dt < chrono::Utc::now() {
                        return Ok(None);
                    }
                }
            }
            Ok(Some((id, name)))
        }
        None => Ok(None),
    }
}
