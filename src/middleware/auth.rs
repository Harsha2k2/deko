use crate::db::DbPool;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tracing::warn;

use crate::middleware::jwt::JwtState;
use crate::models::Agent;

/// State required by the API key authentication middleware.
#[derive(Clone)]
pub struct AgentState {
    pub pool: DbPool,
    pub api_key_secret: String,
}

/// Combined state for the unified middleware.
#[derive(Clone)]
pub struct AuthState {
    pub agent: AgentState,
    pub jwt: JwtState,
}

/// Unified agent authentication middleware.
///
/// Accepts either credential, tried in order:
/// 1. `Authorization: Bearer <jwt>` — validated against the configured secret,
///    agent re-loaded from db so revocation takes effect immediately.
/// 2. `X-API-Key` — hashed and looked up in `api_keys` (multi-key) with
///    fallback to the legacy `agents.api_key_hash` column.
///
/// Requests carrying neither credential are rejected with 401.
pub async fn agent_auth_middleware(State(state): State<AuthState>, request: Request<Body>, next: Next) -> Response {
    let bearer = request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_owned)
        .or_else(|| {
            // browser WebSocket cannot set headers — allow ?token=<jwt> query fallback
            request.uri().query().and_then(|q| {
                for pair in q.split('&') {
                    if let Some(t) = pair.strip_prefix("token=") {
                        if !t.is_empty() {
                            return Some(t.to_string());
                        }
                    }
                }
                None
            })
        });

    if let Some(token) = bearer {
        match crate::middleware::jwt::validate_token(&token, &state.jwt.jwt_secret) {
            Ok(claims) => match load_active_agent(&state.jwt.pool, &claims.sub).await {
                Ok(Some(agent)) => {
                    let mut request = request;
                    request.extensions_mut().insert(agent);
                    return next.run(request).await;
                }
                Ok(None) => return unauthorized("Agent not found or deactivated"),
                Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response(),
            },
            Err(e) => {
                warn!("JWT validation failed: {}", e);
                return unauthorized("Invalid or expired token");
            }
        }
    }

    auth_middleware(State(state.agent), request, next).await
}

async fn load_active_agent(pool: &DbPool, agent_id: &str) -> std::result::Result<Option<Agent>, sqlx::Error> {
    sqlx::query_as::<_, Agent>(
        "SELECT id, name, api_key_hash, active, created_at, deactivated_reason, deactivated_at, api_key_expires_at FROM agents WHERE id = ? AND active = 1",
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await
}

/// Middleware that authenticates agents via API key.
///
/// Extracts the `X-API-Key` header, hashes it with SHA-256, and looks up the
/// hash in the database. If found and active, the `Agent` is inserted into the
/// request extensions for downstream handlers.
///
/// Returns `401 Unauthorized` for missing, invalid, or revoked keys.
pub async fn auth_middleware(State(state): State<AgentState>, request: Request<Body>, next: Next) -> Response {
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

    crate::services::audit::record(
        &state.pool,
        Some(&agent.id),
        "api_key_used",
        &serde_json::json!({
            "agent_name": agent.name,
            "path": request.uri().path(),
        }),
    )
    .await
    .ok();

    let mut request = request;
    request.extensions_mut().insert(agent);

    next.run(request).await
}

fn unauthorized(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({ "error": msg })),
    )
        .into_response()
}

use sha2::{Digest, Sha256};

pub fn hash_api_key(key: &str, secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{key}:{secret}"));
    hex::encode(hasher.finalize())
}
