use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::models::Agent;

/// JWT claims for Deko authentication.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Agent ID (sub)
    pub sub: String,
    /// Agent name
    pub name: String,
    /// Issued at (Unix timestamp)
    pub iat: i64,
    /// Expiration (Unix timestamp)
    pub exp: i64,
}

/// Create a signed JWT for an agent.
pub fn create_token(agent_id: &str, agent_name: &str, secret: &str, expiry_secs: i64) -> Result<String, jsonwebtoken::errors::Error> {
    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        sub: agent_id.to_string(),
        name: agent_name.to_string(),
        iat: now,
        exp: now + expiry_secs,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Validate a JWT and return the claims.
fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(token_data.claims)
}

/// State needed by the JWT authentication middleware.
#[derive(Clone)]
pub struct JwtState {
    pub jwt_secret: String,
}

/// Middleware that authenticates agents via JWT Bearer token.
///
/// Extracts the `Authorization: Bearer <token>` header, validates the JWT,
/// looks up the agent in the database, and inserts the `Agent` into request
/// extensions for downstream handlers.
pub async fn jwt_middleware(
    State(state): State<JwtState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let token = match request
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        Some(t) => t,
        None => return unauthorized("Missing or invalid Authorization header"),
    };

    let claims = match validate_token(token, &state.jwt_secret) {
        Ok(c) => c,
        Err(e) => {
            warn!("JWT validation failed: {}", e);
            return unauthorized("Invalid or expired token");
        }
    };

    // Look up the agent to ensure it still exists and is active
    let pool = match request.extensions().get::<crate::db::DbPool>() {
        Some(p) => p.clone(),
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response(),
    };

    let agent = match sqlx::query_as::<_, Agent>(
        "SELECT id, name, api_key_hash, active, created_at, deactivated_reason, deactivated_at, api_key_expires_at FROM agents WHERE id = ? AND active = 1",
    )
    .bind(&claims.sub)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(a)) => a,
        Ok(None) => return unauthorized("Agent not found or deactivated"),
        Err(e) => {
            warn!("Database error during JWT auth: {}", e);
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
