use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{AppError, Result};

type DbPool = crate::db::DbPool;

#[derive(Deserialize)]
pub struct ActionsQuery {
    pub status: Option<String>,
}

#[derive(serde::Serialize)]
pub struct ActionJson {
    pub id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub intent: String,
    pub payload: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub target_url: Option<String>,
    pub target_method: Option<String>,
    pub risk_level: Option<String>,
    pub verdict_decision: Option<String>,
    pub verdict_reason: Option<String>,
}

#[derive(serde::Serialize)]
pub struct AgentJson {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub created_at: String,
    pub deactivated_reason: Option<String>,
    pub deactivated_at: Option<String>,
}

#[derive(serde::Serialize)]
pub struct VerdictJson {
    pub id: String,
    pub action_id: String,
    pub decision: String,
    pub reason: String,
    pub risk_level: String,
    pub policy_matched: Option<String>,
    pub llm_raw_response: Option<String>,
    pub created_at: String,
}

#[derive(serde::Serialize)]
pub struct PolicyJson {
    pub id: String,
    pub name: String,
    pub description: String,
    pub rules: serde_json::Value,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(serde::Serialize)]
pub struct AuditLogJson {
    pub id: String,
    pub action_id: Option<String>,
    pub event_type: String,
    pub details: Option<serde_json::Value>,
    pub created_at: String,
}

type ActionRow = (String, String, String, Option<String>, String, String, String, Option<String>, Option<String>, String);

pub async fn dashboard(State(pool): State<DbPool>) -> Result<Json<serde_json::Value>> {
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM actions")
        .fetch_one(&pool).await.map_err(AppError::Database)?;
    let pending: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM actions WHERE status = 'pending'")
        .fetch_one(&pool).await.map_err(AppError::Database)?;
    let approved: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM actions WHERE status = 'approved'")
        .fetch_one(&pool).await.map_err(AppError::Database)?;
    let denied: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM actions WHERE status = 'denied'")
        .fetch_one(&pool).await.map_err(AppError::Database)?;
    let escalated: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM actions WHERE status = 'escalated'")
        .fetch_one(&pool).await.map_err(AppError::Database)?;
    let agents: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agents WHERE active = 1")
        .fetch_one(&pool).await.map_err(AppError::Database)?;
    let policies: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM policies WHERE active = 1")
        .fetch_one(&pool).await.map_err(AppError::Database)?;

    Ok(Json(json!({
        "total_actions": total.0,
        "pending_actions": pending.0,
        "approved_actions": approved.0,
        "denied_actions": denied.0,
        "escalated_actions": escalated.0,
        "active_agents": agents.0,
        "active_policies": policies.0,
    })))
}

pub async fn action_timeline(State(pool): State<DbPool>) -> Result<Json<serde_json::Value>> {
    let rows: Vec<(String, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT date(created_at) as day, \
         COUNT(*) as total, \
         SUM(CASE WHEN status = 'approved' THEN 1 ELSE 0 END) as approved, \
         SUM(CASE WHEN status = 'denied' THEN 1 ELSE 0 END) as denied, \
         SUM(CASE WHEN status = 'escalated' THEN 1 ELSE 0 END) as escalated \
         FROM actions WHERE created_at >= date('now', '-30 days') \
         GROUP BY date(created_at) ORDER BY day"
    )
    .fetch_all(&pool).await.map_err(AppError::Database)?;

    Ok(Json(json!(rows.into_iter().map(|r| json!({
        "date": r.0,
        "total": r.1,
        "approved": r.2,
        "denied": r.3,
        "escalated": r.4,
    })).collect::<Vec<_>>())))
}

pub async fn list_actions(
    State(pool): State<DbPool>,
    Query(params): Query<ActionsQuery>,
) -> Result<Json<Vec<ActionJson>>> {
    let mut sql = "SELECT a.id, ag.name, a.intent, a.payload, a.status, a.created_at, a.updated_at, a.target_url, a.target_method, a.agent_id FROM actions a JOIN agents ag ON a.agent_id = ag.id WHERE 1=1".to_string();

    if let Some(ref s) = params.status {
        sql.push_str(" AND a.status = '");
        sql.push_str(s);
        sql.push('\'');
    }
    sql.push_str(" ORDER BY a.created_at DESC LIMIT 100");

    let rows: Vec<ActionRow> = sqlx::query_as(&sql)
        .fetch_all(&pool).await.map_err(AppError::Database)?;

    let mut result = Vec::new();
    for r in rows {
        let verdict: Option<(String, String, String)> = sqlx::query_as(
            "SELECT decision, reason, risk_level FROM verdicts WHERE action_id = ? ORDER BY created_at DESC LIMIT 1"
        )
        .bind(&r.0)
        .fetch_optional(&pool).await.unwrap_or(None);

        let (v_dec, v_reason, v_risk) = match verdict {
            Some(v) => (Some(v.0), Some(v.1), Some(v.2)),
            None => (None, None, None),
        };

        result.push(ActionJson {
            id: r.0,
            agent_id: r.9,
            agent_name: r.1,
            intent: r.2,
            payload: r.3,
            status: r.4,
            created_at: r.5,
            updated_at: r.6,
            target_url: r.7,
            target_method: r.8,
            risk_level: v_risk,
            verdict_decision: v_dec,
            verdict_reason: v_reason,
        });
    }
    Ok(Json(result))
}

pub async fn get_action(
    State(pool): State<DbPool>,
    Path(id): Path<String>,
) -> Result<Json<ActionJson>> {
    let row: Option<ActionRow> = sqlx::query_as(
        "SELECT a.id, ag.name, a.intent, a.payload, a.status, a.created_at, a.updated_at, a.target_url, a.target_method, a.agent_id FROM actions a JOIN agents ag ON a.agent_id = ag.id WHERE a.id = ?"
    )
    .bind(&id)
    .fetch_optional(&pool).await.map_err(AppError::Database)?;

    let r = row.ok_or_else(|| AppError::NotFound("Action not found".into()))?;

    let verdict: Option<(String, String, String)> = sqlx::query_as(
        "SELECT decision, reason, risk_level FROM verdicts WHERE action_id = ? ORDER BY created_at DESC LIMIT 1"
    )
    .bind(&id)
    .fetch_optional(&pool).await.unwrap_or(None);

    let (v_dec, v_reason, v_risk) = match verdict {
        Some(v) => (Some(v.0), Some(v.1), Some(v.2)),
        None => (None, None, None),
    };

    Ok(Json(ActionJson {
        id: r.0,
        agent_id: r.9,
        agent_name: r.1,
        intent: r.2,
        payload: r.3,
        status: r.4,
        created_at: r.5,
        updated_at: r.6,
        target_url: r.7,
        target_method: r.8,
        risk_level: v_risk,
        verdict_decision: v_dec,
        verdict_reason: v_reason,
    }))
}

#[derive(Deserialize)]
pub struct OverrideBody {
    pub decision: String,
    pub reason: String,
}

#[derive(Serialize)]
pub struct OverrideResponse {
    pub success: bool,
    pub new_status: String,
}

pub async fn override_action(
    State(pool): State<DbPool>,
    Path(id): Path<String>,
    Json(body): Json<OverrideBody>,
) -> Result<Json<OverrideResponse>> {
    if body.reason.trim().is_empty() {
        return Err(AppError::BadRequest("reason is required".into()));
    }

    let action = sqlx::query_as::<_, (String, String)>(
        "SELECT id, status FROM actions WHERE id = ?"
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("Action not found".into()))?;

    let current_status = action.1.as_str();

    let new_status = match body.decision.as_str() {
        "approved" => {
            if current_status != "denied" && current_status != "escalated" && current_status != "pending" {
                return Err(AppError::BadRequest("Can only override denied, escalated, or pending actions".into()));
            }
            "approved"
        }
        "denied" => {
            if current_status != "pending" && current_status != "escalated" {
                return Err(AppError::BadRequest("Can only deny pending or escalated actions".into()));
            }
            "denied"
        }
        "escalated" => {
            if current_status != "pending" {
                return Err(AppError::BadRequest("Can only escalate pending actions".into()));
            }
            "escalated"
        }
        _ => return Err(AppError::BadRequest("decision must be 'approved', 'denied', or 'escalated'".into())),
    };

    sqlx::query("UPDATE actions SET status = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(new_status)
        .bind(&id)
        .execute(&pool)
        .await
        .map_err(AppError::Database)?;

    sqlx::query(
        "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, ?, ?, ?)"
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&id)
    .bind("admin_override")
    .bind(json!({
        "previous_status": current_status,
        "decision": body.decision,
        "reason": body.reason,
    }).to_string())
    .execute(&pool)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(OverrideResponse {
        success: true,
        new_status: new_status.to_string(),
    }))
}

pub async fn list_agents(State(pool): State<DbPool>) -> Result<Json<Vec<AgentJson>>> {
    let rows: Vec<(String, String, bool, String)> = sqlx::query_as(
        "SELECT id, name, active, created_at FROM agents ORDER BY created_at DESC"
    )
    .fetch_all(&pool).await.map_err(AppError::Database)?;

    Ok(Json(rows.into_iter().map(|r| AgentJson {
        id: r.0,
        name: r.1,
        active: r.2,
        created_at: r.3,
        deactivated_reason: None,
        deactivated_at: None,
    }).collect()))
}

pub async fn list_verdicts(State(pool): State<DbPool>) -> Result<Json<Vec<VerdictJson>>> {
    let rows: Vec<(String, String, String, String, String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT id, action_id, decision, reason, risk_level, policy_matched, llm_raw_response, created_at FROM verdicts ORDER BY created_at DESC LIMIT 100"
    )
    .fetch_all(&pool).await.map_err(AppError::Database)?;

    Ok(Json(rows.into_iter().map(|r| VerdictJson {
        id: r.0,
        action_id: r.1,
        decision: r.2,
        reason: r.3,
        risk_level: r.4,
        policy_matched: r.5,
        llm_raw_response: r.6,
        created_at: r.7,
    }).collect()))
}

pub async fn list_policies(State(pool): State<DbPool>) -> Result<Json<Vec<PolicyJson>>> {
    let rows: Vec<(String, String, String, String, bool, String, String)> = sqlx::query_as(
        "SELECT id, name, description, rules, active, created_at, updated_at FROM policies ORDER BY created_at DESC"
    )
    .fetch_all(&pool).await.map_err(AppError::Database)?;

    Ok(Json(rows.into_iter().map(|r| PolicyJson {
        id: r.0,
        name: r.1,
        description: r.2,
        rules: serde_json::from_str(&r.3).unwrap_or(json!({})),
        active: r.4,
        created_at: r.5,
        updated_at: r.6,
    }).collect()))
}

#[derive(Deserialize)]
pub struct AuditListQuery {
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn list_audit_log(
    State(pool): State<DbPool>,
    Query(params): Query<AuditListQuery>,
) -> Result<Json<Vec<AuditLogJson>>> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.limit.unwrap_or(50).min(200);
    let offset = (page - 1) * per_page;

    let rows: Vec<(String, Option<String>, String, String, String)> = sqlx::query_as(
        "SELECT id, action_id, event_type, details, created_at FROM audit_log ORDER BY created_at DESC LIMIT ? OFFSET ?"
    )
    .bind(per_page)
    .bind(offset)
    .fetch_all(&pool).await.map_err(AppError::Database)?;

    Ok(Json(rows.into_iter().map(|r| AuditLogJson {
        id: r.0,
        action_id: r.1,
        event_type: r.2,
        details: serde_json::from_str(&r.3).ok(),
        created_at: r.4,
    }).collect()))
}
