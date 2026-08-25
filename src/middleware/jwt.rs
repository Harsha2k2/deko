use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

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
pub fn create_token(
    agent_id: &str,
    agent_name: &str,
    secret: &str,
    expiry_secs: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
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
pub fn validate_token(token: &str, secret: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
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
    pub pool: crate::db::DbPool,
}
