use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::models::{RiskLevel, VerdictDecision};

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct Verdict {
    pub id: String,
    pub action_id: String,
    pub decision: VerdictDecision,
    pub reason: String,
    pub risk_level: RiskLevel,
    pub policy_matched: Option<String>,
    pub llm_raw_response: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerdictResponse {
    pub id: String,
    pub action_id: String,
    pub decision: VerdictDecision,
    pub reason: String,
    pub risk_level: RiskLevel,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
