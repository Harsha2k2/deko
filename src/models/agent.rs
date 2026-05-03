use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub api_key_hash: String,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
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
