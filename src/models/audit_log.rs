use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct AuditLog {
    pub id: String,
    pub action_id: Option<String>,
    pub event_type: String,
    pub details: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AuditLogEntry {
    pub action_id: Option<String>,
    pub event_type: String,
    pub details: serde_json::Value,
}
