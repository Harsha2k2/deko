use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sqlx::SqlitePool;
use tracing::warn;

use crate::models::Agent;

#[derive(Clone)]
pub struct AgentState {
    pub pool: SqlitePool,
    pub api_key_secret: String,
}

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

    let agent = match sqlx::query_as::<_, Agent>(
        "SELECT id, name, api_key_hash, active, created_at FROM agents WHERE api_key_hash = ? AND active = 1",
    )
    .bind(&hashed)
    .fetch_optional(&state.pool)
    .await
    {
        Ok(Some(a)) => a,
        Ok(None) => return unauthorized("Invalid or revoked API key"),
        Err(e) => {
            warn!("Database error during auth: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

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

pub fn hash_api_key_with_secret(key: &str, secret: &str) -> String {
    hash_api_key(key, secret)
}
