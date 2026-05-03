use serde::{Deserialize, Serialize};
use sqlx::Type;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Type)]
#[sqlx(type_name = "risk_level", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "low"),
            RiskLevel::Medium => write!(f, "medium"),
            RiskLevel::High => write!(f, "high"),
            RiskLevel::Critical => write!(f, "critical"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Type)]
#[sqlx(type_name = "verdict_decision", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum VerdictDecision {
    Approved,
    Denied,
    Escalate,
}

impl std::fmt::Display for VerdictDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerdictDecision::Approved => write!(f, "approved"),
            VerdictDecision::Denied => write!(f, "denied"),
            VerdictDecision::Escalate => write!(f, "escalate"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Type)]
#[sqlx(type_name = "action_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    Pending,
    Processing,
    Approved,
    Denied,
    Escalated,
    Forwarded,
}

impl std::fmt::Display for ActionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionStatus::Pending => write!(f, "pending"),
            ActionStatus::Processing => write!(f, "processing"),
            ActionStatus::Approved => write!(f, "approved"),
            ActionStatus::Denied => write!(f, "denied"),
            ActionStatus::Escalated => write!(f, "escalated"),
            ActionStatus::Forwarded => write!(f, "forwarded"),
        }
    }
}
