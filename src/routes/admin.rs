use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{AppError, Result};
use crate::models::{Action, ActionStatus};

#[derive(Deserialize)]
pub struct AdminLoginRequest {
    pub password: String,
}

pub async fn admin_logout() -> impl axum::response::IntoResponse {
    use axum::http::header::{SET_COOKIE, HeaderValue};
    let cookie = "deko_admin=; Path=/; HttpOnly; Max-Age=0";
    let mut resp = axum::Json(serde_json::json!({ "ok": true })).into_response();
    resp.headers_mut().insert(SET_COOKIE, HeaderValue::from_str(cookie).unwrap());
    resp
}

static LOGIN_RATE_LIMIT: AtomicU64 = AtomicU64::new(0);
const MAX_LOGIN_ATTEMPTS: u64 = 5;

fn check_login_rate_limit() -> Result<()> {
    let attempts = LOGIN_RATE_LIMIT.load(Ordering::Relaxed);
    if attempts >= MAX_LOGIN_ATTEMPTS {
        return Err(AppError::RateLimited);
    }
    Ok(())
}

fn record_login_attempt() {
    LOGIN_RATE_LIMIT.fetch_add(1, Ordering::Relaxed);
}

pub async fn admin_login(
    State(_pool): State<crate::db::DbPool>,
    axum::Form(req): axum::Form<AdminLoginRequest>,
) -> Result<axum::response::Response> {
    check_login_rate_limit()?;
    record_login_attempt();
    use axum::http::header::{SET_COOKIE, HeaderValue};

    let config = crate::config::Config::from_env().map_err(|_| AppError::Internal)?;
    if req.password != config.admin_password {
        return Err(AppError::Unauthorized("Invalid password".into()));
    }

    let cookie_value = format!(
        "deko_admin={}; Path=/; HttpOnly; SameSite=Strict; Max-Age=28800",
        req.password
    );
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(SET_COOKIE, HeaderValue::from_str(&cookie_value).map_err(|_| AppError::Internal)?);

    let mut response = axum::Json(serde_json::json!({ "ok": true })).into_response();
    *response.headers_mut() = headers;
    Ok(response)
}

#[derive(Deserialize)]
pub struct OverrideRequest {
    pub reason: String,
}

pub async fn override_action(
    State(pool): State<crate::db::DbPool>,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
    axum::Form(req): axum::Form<OverrideRequest>,
) -> Result<axum::response::Redirect> {
    if req.reason.trim().is_empty() {
        return Err(AppError::BadRequest("reason is required".into()));
    }

    if headers.get("X-Admin-Confirm").and_then(|v| v.to_str().ok()) != Some("yes") {
        return Err(AppError::BadRequest("Confirmation required: set X-Admin-Confirm: yes header".into()));
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
        return Err(AppError::BadRequest("Can only override denied or escalated actions".into()));
    }

    let mut tx = pool.begin().await.map_err(AppError::Database)?;

    sqlx::query("UPDATE actions SET status = 'approved', updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Database)?;

    sqlx::query("INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, ?, ?, ?)")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&id)
        .bind("admin_override")
        .bind(serde_json::json!({ "previous_status": format!("{:?}", action.status).to_lowercase(), "reason": req.reason }).to_string())
        .execute(&mut *tx)
        .await
        .map_err(AppError::Database)?;

    tx.commit().await.map_err(AppError::Database)?;

    Ok(axum::response::Redirect::to(&format!("/admin/actions/{}", id)))
}

#[derive(Deserialize)]
pub struct BulkOverrideRequest {
    pub action_ids: Vec<String>,
    pub reason: String,
}

pub async fn bulk_override_actions(
    State(pool): State<crate::db::DbPool>,
    headers: axum::http::HeaderMap,
    Json(req): Json<BulkOverrideRequest>,
) -> Result<Json<serde_json::Value>> {
    if headers.get("X-Admin-Confirm").and_then(|v| v.to_str().ok()) != Some("yes") {
        return Err(AppError::BadRequest("Confirmation required: set X-Admin-Confirm: yes header".into()));
    }
    if req.reason.trim().is_empty() {
        return Err(AppError::BadRequest("reason is required".into()));
    }
    if req.action_ids.len() > 100 {
        return Err(AppError::BadRequest("Maximum 100 actions per bulk operation".into()));
    }

    let mut overridden = 0;
    for action_id in &req.action_ids {
        let result = sqlx::query("UPDATE actions SET status = 'approved', updated_at = CURRENT_TIMESTAMP WHERE id = ? AND status IN ('denied', 'escalated')")
            .bind(action_id)
            .execute(&pool)
            .await
            .map_err(AppError::Database)?;
        if result.rows_affected() > 0 {
            sqlx::query(
                "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, ?, 'action_overridden', ?)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(action_id)
            .bind(serde_json::json!({"reason": req.reason, "bulk": true}))
            .execute(&pool)
            .await
            .ok();
            overridden += 1;
        }
    }

    Ok(Json(serde_json::json!({ "overridden": overridden, "total": req.action_ids.len() })))
}

pub async fn export_actions_csv(
    State(pool): State<crate::db::DbPool>,
    Query(params): Query<serde_json::Value>,
) -> Result<String> {
    let status_filter = params.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let mut query = "SELECT a.id, ag.name, a.intent, a.status, v.decision, v.risk_level, a.created_at FROM actions a JOIN agents ag ON a.agent_id = ag.id LEFT JOIN verdicts v ON a.id = v.action_id".to_string();
    if !status_filter.is_empty() {
        query.push_str(&format!(" WHERE a.status = '{}'", status_filter));
    }
    query.push_str(" ORDER BY a.created_at DESC LIMIT 1000");

    let rows: Vec<(String, String, String, String, Option<String>, Option<String>, String)> = sqlx::query_as(&query)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

    let mut csv = "id,agent,intent,status,decision,risk_level,created_at\n".to_string();
    for row in rows {
        csv.push_str(&format!("\"{}\",\"{}\",\"{}\",{},{},{},{}\n",
            row.0, row.1, row.2.replace('"', "\"\""), row.3,
            row.4.unwrap_or_default(), row.5.unwrap_or_default(), row.6));
    }

    Ok(csv)
}

#[derive(Deserialize)]
pub struct AuditExportQuery {
    pub event_type: Option<String>,
    pub limit: Option<i64>,
}

pub async fn export_audit_log(
    State(pool): State<crate::db::DbPool>,
    Query(params): Query<AuditExportQuery>,
) -> Result<String> {
    let limit = params.limit.unwrap_or(1000).min(10000);
    let mut query = "SELECT id, action_id, event_type, details, created_at FROM audit_log".to_string();
    if let Some(ref et) = params.event_type {
        query.push_str(&format!(" WHERE event_type = '{}'", et));
    }
    query.push_str(&format!(" ORDER BY created_at DESC LIMIT {}", limit));

    let rows: Vec<(String, Option<String>, String, String, String)> = sqlx::query_as(&query)
        .fetch_all(&pool)
        .await
        .unwrap_or_default();

    let mut csv = "id,action_id,event_type,details,created_at\n".to_string();
    for row in rows {
        csv.push_str(&format!("\"{}\",\"{}\",\"{}\",\"{}\",{}\n",
            row.0, row.1.unwrap_or_default(), row.2, row.3.replace('"', "\"\""), row.4));
    }

    Ok(csv)
}

#[derive(Deserialize)]
pub struct AuditSearchQuery {
    pub q: String,
    pub limit: Option<i64>,
}

pub async fn search_audit_log(
    State(pool): State<crate::db::DbPool>,
    Query(params): Query<AuditSearchQuery>,
) -> Result<String> {
    let limit = params.limit.unwrap_or(50).min(500);
    let search_term = format!("%{}%", params.q);

    let rows: Vec<(String, Option<String>, String, String, String)> = sqlx::query_as(
        "SELECT id, action_id, event_type, details, created_at FROM audit_log \
         WHERE event_type LIKE ? OR details LIKE ? OR action_id LIKE ? \
         ORDER BY created_at DESC LIMIT ?",
    )
    .bind(&search_term)
    .bind(&search_term)
    .bind(&search_term)
    .bind(limit)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();

    let mut results = Vec::new();
    for row in rows {
        results.push(serde_json::json!({
            "id": row.0,
            "action_id": row.1,
            "event_type": row.2,
            "details": row.3,
            "created_at": row.4,
        }));
    }

    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "results": results,
        "total": results.len(),
        "query": params.q,
    })).unwrap_or_default())
}
