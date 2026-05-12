use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::models::ActionStatus;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct Action {
    pub id: String,
    pub agent_id: String,
    pub intent: String,
    pub payload: Option<String>,
    pub screenshot_base64: Option<String>,
    pub metadata: Option<String>,
    pub status: ActionStatus,
    pub target_url: Option<String>,
    pub target_method: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub idempotency_key: Option<String>,
    pub priority: i32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateActionRequest {
    pub intent: String,
    pub payload: Option<String>,
    pub screenshot_base64: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub target_url: Option<String>,
    pub target_method: Option<String>,
    pub idempotency_key: Option<String>,
    pub priority: Option<i32>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActionResponse {
    pub id: String,
    pub agent_id: String,
    pub intent: String,
    pub status: ActionStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub verdict: Option<crate::models::VerdictResponse>,
}
