use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct Policy {
    pub id: String,
    pub name: String,
    pub description: String,
    pub rules: serde_json::Value,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatePolicyRequest {
    pub name: String,
    pub description: String,
    pub rules: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdatePolicyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub rules: Option<serde_json::Value>,
    pub active: Option<bool>,
}
