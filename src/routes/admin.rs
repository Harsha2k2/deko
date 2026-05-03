use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::error::{AppError, Result};
use crate::models::{Action, ActionStatus, AuditLog, Verdict};

#[derive(Debug, Deserialize)]
pub struct AdminLoginRequest {
    pub password: String,
}

pub async fn admin_login(
    _state: State<SqlitePool>,
    Json(req): Json<AdminLoginRequest>,
) -> Result<Json<serde_json::Value>> {
    let config = crate::config::Config::from_env().map_err(|_| AppError::Internal)?;
    if req.password != config.admin_password {
        return Err(AppError::Unauthorized("Invalid password".into()));
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct AdminActionDetail {
    pub id: String,
    pub agent_name: String,
    pub intent: String,
    pub status: String,
    pub verdict_decision: Option<String>,
    pub verdict_reason: Option<String>,
    pub risk_level: Option<String>,
    pub created_at: String,
}

pub async fn dashboard(State(pool): State<SqlitePool>) -> Result<Json<serde_json::Value>> {
    let total_actions: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM actions")
        .fetch_one(&pool)
        .await
        .map_err(AppError::Database)?;

    let pending: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM actions WHERE status = 'pending'")
            .fetch_one(&pool)
            .await
            .map_err(AppError::Database)?;

    let denied: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM actions WHERE status = 'denied'")
            .fetch_one(&pool)
            .await
            .map_err(AppError::Database)?;

    let total_agents: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM agents WHERE active = 1")
            .fetch_one(&pool)
            .await
            .map_err(AppError::Database)?;

    let total_policies: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM policies WHERE active = 1")
            .fetch_one(&pool)
            .await
            .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({
        "total_actions": total_actions.0,
        "pending_actions": pending.0,
        "denied_actions": denied.0,
        "active_agents": total_agents.0,
        "active_policies": total_policies.0,
    })))
}

pub async fn list_admin_actions(
    State(pool): State<SqlitePool>,
    Query(params): Query<serde_json::Value>,
) -> Result<Json<Vec<AdminActionDetail>>> {
    let status = params.get("status").and_then(|s| s.as_str());

    let mut query = "SELECT a.id, ag.name as agent_name, a.intent, a.status, v.decision as verdict_decision, v.reason as verdict_reason, v.risk_level, a.created_at FROM actions a JOIN agents ag ON a.agent_id = ag.id LEFT JOIN verdicts v ON a.id = v.action_id WHERE 1=1".to_string();

    if let Some(s) = status {
        query.push_str(" AND a.status = '");
        query.push_str(s);
        query.push('\'');
    }

    query.push_str(" ORDER BY a.created_at DESC LIMIT 100");

    let rows: Vec<(String, String, String, String, Option<String>, Option<String>, Option<String>, String)> =
        sqlx::query_as(&query)
            .fetch_all(&pool)
            .await
            .map_err(AppError::Database)?;

    let actions = rows
        .into_iter()
        .map(|r| AdminActionDetail {
            id: r.0,
            agent_name: r.1,
            intent: r.2,
            status: r.3,
            verdict_decision: r.4,
            verdict_reason: r.5,
            risk_level: r.6,
            created_at: r.7,
        })
        .collect();

    Ok(Json(actions))
}

pub async fn get_admin_action_detail(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let action = sqlx::query_as::<_, Action>(
        "SELECT id, agent_id, intent, payload, screenshot_base64, metadata, status, target_url, target_method, created_at, updated_at FROM actions WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("Action not found".into()))?;

    let agent = sqlx::query_as::<_, crate::models::Agent>(
        "SELECT id, name, api_key_hash, active, created_at FROM agents WHERE id = ?",
    )
    .bind(&action.agent_id)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Database)?;

    let verdict = sqlx::query_as::<_, Verdict>(
        "SELECT id, action_id, decision, reason, risk_level, policy_matched, llm_raw_response, created_at FROM verdicts WHERE action_id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Database)?;

    let audit_logs: Vec<AuditLog> = sqlx::query_as(
        "SELECT id, action_id, event_type, details, created_at FROM audit_log WHERE action_id = ? ORDER BY created_at ASC",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({
        "action": {
            "id": action.id,
            "intent": action.intent,
            "payload": action.payload,
            "status": action.status,
            "target_url": action.target_url,
            "target_method": action.target_method,
            "metadata": action.metadata,
            "created_at": action.created_at,
            "updated_at": action.updated_at,
        },
        "agent": agent.map(|a| serde_json::json!({
            "id": a.id,
            "name": a.name,
            "active": a.active,
        })),
        "verdict": verdict.map(|v| serde_json::json!({
            "decision": v.decision,
            "reason": v.reason,
            "risk_level": v.risk_level,
            "llm_raw_response": v.llm_raw_response,
        })),
        "audit_log": audit_logs.iter().map(|l| serde_json::json!({
            "event_type": l.event_type,
            "details": l.details,
            "created_at": l.created_at,
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Debug, Deserialize)]
pub struct OverrideRequest {
    pub reason: String,
}

pub async fn override_action(
    State(pool): State<SqlitePool>,
    Path(id): Path<String>,
    Json(req): Json<OverrideRequest>,
) -> Result<Json<serde_json::Value>> {
    if req.reason.trim().is_empty() {
        return Err(AppError::BadRequest("reason is required".into()));
    }

    let action = sqlx::query_as::<_, Action>(
        "SELECT id, agent_id, intent, payload, screenshot_base64, metadata, status, target_url, target_method, created_at, updated_at FROM actions WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("Action not found".into()))?;

    if action.status != ActionStatus::Denied && action.status != ActionStatus::Escalated {
        return Err(AppError::BadRequest(
            "Can only override denied or escalated actions".into(),
        ));
    }

    let mut tx = pool.begin().await.map_err(AppError::Database)?;

    sqlx::query(
        "UPDATE actions SET status = 'approved', updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    sqlx::query(
        "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&id)
    .bind("admin_override")
    .bind(serde_json::json!({
        "previous_status": action.status,
        "reason": req.reason,
    }))
    .execute(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    tx.commit().await.map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({
        "overridden": true,
        "action_id": id,
        "new_status": "approved",
    })))
}
