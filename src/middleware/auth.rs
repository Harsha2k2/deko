use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use crate::db::DbPool;
use tracing::warn;

use crate::models::Agent;

/// State required by the API key authentication middleware.
#[derive(Clone)]
pub struct AgentState {
    pub pool: DbPool,
    pub api_key_secret: String,
}

/// Middleware that authenticates agents via API key.
///
/// Extracts the `X-API-Key` header, hashes it with SHA-256, and looks up the
/// hash in the database. If found and active, the `Agent` is inserted into the
/// request extensions for downstream handlers.
///
/// Returns `401 Unauthorized` for missing, invalid, or revoked keys.
pub async fn auth_middleware(
    State(state): State<AgentState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let api_key = match request.headers().get("X-API-Key") {
        Some(key) => match key.to_str() {
            Ok(k) => k,
            Err(_) => return unauthorized("Invalid API key format"),
        },
        None => return unauthorized("Missing X-API-Key header"),
    };

    let hashed = hash_api_key(api_key, &state.api_key_secret);

    // Check api_keys table first (multi-key support)
    let key_record = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT agent_id, label, expires_at FROM api_keys WHERE key_hash = ? AND active = 1",
    )
    .bind(&hashed)
    .fetch_optional(&state.pool)
    .await;

    let agent = if let Ok(Some((agent_id, _label, expires_at))) = &key_record {
        if let Some(expires) = expires_at {
            if let Ok(expires_dt) = chrono::DateTime::parse_from_rfc3339(expires) {
                if expires_dt < chrono::Utc::now() {
                    return unauthorized("API key has expired");
                }
            }
        }
        match sqlx::query_as::<_, Agent>(
            "SELECT id, name, api_key_hash, active, created_at, deactivated_reason, deactivated_at, api_key_expires_at FROM agents WHERE id = ? AND active = 1",
        )
        .bind(agent_id)
        .fetch_optional(&state.pool)
        .await
        {
            Ok(Some(a)) => a,
            _ => return unauthorized("Agent not found or deactivated"),
        }
    } else {
        // Fallback: direct agent key lookup (legacy)
        match sqlx::query_as::<_, Agent>(
            "SELECT id, name, api_key_hash, active, created_at, deactivated_reason, deactivated_at, api_key_expires_at FROM agents WHERE api_key_hash = ? AND active = 1",
        )
        .bind(&hashed)
        .fetch_optional(&state.pool)
        .await
        {
            Ok(Some(a)) => {
                if let Some(expires) = &a.api_key_expires_at {
                    if *expires < chrono::Utc::now() {
                        return unauthorized("API key has expired");
                    }
                }
                a
            }
            Ok(None) => return unauthorized("Invalid or revoked API key"),
            Err(e) => {
                warn!("Database error during auth: {}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
            }
        }
    };

    sqlx::query(
        "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&agent.id)
    .bind("api_key_used")
    .bind(serde_json::json!({
        "agent_name": agent.name,
        "path": request.uri().path(),
    }))
    .execute(&state.pool)
    .await
    .ok();

    let mut request = request;
    request.extensions_mut().insert(agent);

    next.run(request).await
}

fn unauthorized(msg: &str) -> Response {
    (StatusCode::UNAUTHORIZED, axum::Json(serde_json::json!({ "error": msg }))).into_response()
}

use sha2::{Digest, Sha256};

pub fn hash_api_key(key: &str, secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{key}:{secret}"));
    hex::encode(hasher.finalize())
}
