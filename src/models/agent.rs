use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// A registered AI agent that can submit actions to Deko.
///
/// Each agent has a unique API key (SHA-256 hashed before storage) and can be
/// revoked independently.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub api_key_hash: String,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub deactivated_reason: Option<String>,
    pub deactivated_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateAgentRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateAgentResponse {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
