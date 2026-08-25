use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::error::{AppError, Result};
use crate::models::{Action, ActionStatus};

#[derive(Deserialize)]
pub struct AdminLoginRequest {
    pub password: String,
}

/// constant-time equality for secret comparison; same length strings only
/// (a length difference leaks nothing useful and short-circuits safely).
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes()
        .iter()
        .zip(b.as_bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// per-ip sliding login throttle. unlike the previous global counter this
/// cannot be exhausted by other users' traffic, and windows reset.
#[derive(Clone)]
struct LoginThrottle {
    attempts: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    max_attempts: usize,
    window: std::time::Duration,
}

impl LoginThrottle {
    fn new() -> Self {
        Self {
            attempts: Arc::new(Mutex::new(HashMap::new())),
            max_attempts: 10,
            window: std::time::Duration::from_secs(60),
        }
    }

    fn allow(&self, ip: &str) -> bool {
        let mut map = self.attempts.lock().unwrap();
        let now = Instant::now();
        let recent = map.entry(ip.to_string()).or_default();
        recent.retain(|t| now.duration_since(*t) < self.window);
        if recent.len() >= self.max_attempts {
            return false;
        }
        recent.push(now);
        true
    }
}

fn client_ip(headers: &axum::http::HeaderMap) -> String {
    // best-effort pilot identification; real enforcement belongs at the
    // ingress layer where x-forwarded-for is trustworthy
    headers
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "local".to_string())
}

fn extract_session_token(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie_header = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    for part in cookie_header.split(';') {
        let trimmed = part.trim();
        if let Some(value) = trimmed.strip_prefix("deko_session=") {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// validates admin credentials from either a server-side session cookie or
/// the bootstrap password header. shared by the middleware and endpoints.
pub async fn verify_admin(pool: &crate::db::DbPool, headers: &axum::http::HeaderMap) -> Result<String> {
    // 1. session cookie (preferred; revocable, expiring, hashed at rest)
    if let Some(token) = extract_session_token(headers) {
        if let Some(identity) = crate::services::session::validate(pool, &token).await? { return Ok(identity) }
    }

    // 2. bootstrap password header (for scripted api use before any login)
    let expected = std::env::var("DEKO_ADMIN_PASSWORD").unwrap_or_default();
    if !expected.is_empty() {
        if let Some(provided) = headers.get("X-Admin-Password").and_then(|v| v.to_str().ok()) {
            if constant_time_eq(provided, &expected) {
                return Ok("password".to_string());
            }
        }
    }

    Err(AppError::Forbidden("Admin access required".into()))
}

pub async fn admin_logout(
    State(pool): State<crate::db::DbPool>,
    headers: axum::http::HeaderMap,
) -> impl axum::response::IntoResponse {
    use axum::http::header::{HeaderValue, SET_COOKIE};
    // delete server-side so the token is dead even if the cookie lingers
    if let Some(token) = extract_session_token(&headers) {
        crate::services::session::delete(&pool, &token).await.ok();
    }
    let cookie = "deko_session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0";
    let mut resp = axum::Json(serde_json::json!({ "ok": true })).into_response();
    resp.headers_mut()
        .insert(SET_COOKIE, HeaderValue::from_str(cookie).unwrap());
    resp
}

static LOGIN_THROTTLE: std::sync::OnceLock<LoginThrottle> = std::sync::OnceLock::new();

pub async fn admin_login(
    State(pool): State<crate::db::DbPool>,
    headers: axum::http::HeaderMap,
    axum::Form(req): axum::Form<AdminLoginRequest>,
) -> Result<axum::response::Response> {
    use axum::http::header::{HeaderValue, SET_COOKIE};

    let throttle = LOGIN_THROTTLE.get_or_init(LoginThrottle::new);
    let ip = client_ip(&headers);
    if !throttle.allow(&ip) {
        return Err(AppError::RateLimited);
    }

    let expected = std::env::var("DEKO_ADMIN_PASSWORD").unwrap_or_default();
    if expected.is_empty() || !constant_time_eq(&req.password, &expected) {
        // uniform error whether empty or wrong; do not leak which
        return Err(AppError::Unauthorized("Invalid password".into()));
    }

    let token = crate::services::session::create(&pool, "password").await?;

    let cookie_value = format!(
        "deko_session={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
        token,
        crate::services::session::SESSION_TTL.as_secs()
    );
    let mut response = axum::Json(serde_json::json!({ "ok": true })).into_response();
    if let Ok(val) = HeaderValue::from_str(&cookie_value) {
        response.headers_mut().insert(SET_COOKIE, val);
    }
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
        return Err(AppError::BadRequest(
            "Confirmation required: set X-Admin-Confirm: yes header".into(),
        ));
    }

    let action = sqlx::query_as::<_, Action>(
        "SELECT id, agent_id, intent, payload, screenshot_base64, metadata, status, target_url, target_method, created_at, updated_at, idempotency_key, priority, execute_at FROM actions WHERE id = ?",
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

    sqlx::query("UPDATE actions SET status = 'approved', updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Database)?;

    crate::services::audit::insert_chained_tx(
        &mut tx,
        Some(&id),
        "admin_override",
        &serde_json::json!({ "previous_status": format!("{:?}", action.status).to_lowercase(), "reason": req.reason }),
    )
    .await?;

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
        return Err(AppError::BadRequest(
            "Confirmation required: set X-Admin-Confirm: yes header".into(),
        ));
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
            crate::services::audit::record(
                &pool,
                Some(action_id),
                "action_overridden",
                &serde_json::json!({"reason": req.reason, "bulk": true}),
            )
            .await
            .ok();
            overridden += 1;
        }
    }

    Ok(Json(
        serde_json::json!({ "overridden": overridden, "total": req.action_ids.len() }),
    ))
}

pub async fn export_actions_csv(
    State(pool): State<crate::db::DbPool>,
    Query(params): Query<serde_json::Value>,
) -> Result<String> {
    let status_filter = params.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let mut query = "SELECT a.id, ag.name, a.intent, a.status, v.decision, v.risk_level, a.created_at FROM actions a JOIN agents ag ON a.agent_id = ag.id LEFT JOIN verdicts v ON a.id = v.action_id".to_string();
    if !status_filter.is_empty() {
        query.push_str(" WHERE a.status = ?");
    }
    query.push_str(" ORDER BY a.created_at DESC LIMIT 1000");

    let mut q = sqlx::query_as::<_, (String, String, String, String, Option<String>, Option<String>, String)>(&query);
    if !status_filter.is_empty() {
        q = q.bind(status_filter);
    }
    let rows = q.fetch_all(&pool).await.unwrap_or_default();

    let mut csv = "id,agent,intent,status,decision,risk_level,created_at\n".to_string();
    for row in rows {
        csv.push_str(&format!(
            "\"{}\",\"{}\",\"{}\",{},{},{},{}\n",
            row.0,
            row.1,
            row.2.replace('"', "\"\""),
            row.3,
            row.4.unwrap_or_default(),
            row.5.unwrap_or_default(),
            row.6
        ));
    }

    Ok(csv)
}

/// walks the audit hash chain and reports whether it is intact.
pub async fn verify_audit_chain(
    State(pool): State<crate::db::DbPool>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Response> {
    // admin auth runs as a layer on this router, but double-check the
    // strongest signal explicitly since this endpoint is a security report
    let provided = headers
        .get("X-Admin-Password")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let expected = std::env::var("DEKO_ADMIN_PASSWORD").unwrap_or_default();
    if !expected.is_empty() && provided != expected {
        return Err(AppError::Forbidden("Admin access required".into()));
    }

    let report = crate::services::audit::verify_chain(&pool).await?;
    Ok(Json(report).into_response())
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
    if params.event_type.is_some() {
        query.push_str(" WHERE event_type = ?");
    }
    query.push_str(" ORDER BY created_at DESC LIMIT ?");

    let mut q = sqlx::query_as::<_, (String, Option<String>, String, String, String)>(&query);
    if let Some(ref et) = params.event_type {
        q = q.bind(et);
    }
    q = q.bind(limit);
    let rows = q.fetch_all(&pool).await.unwrap_or_default();

    let mut csv = "id,action_id,event_type,details,created_at\n".to_string();
    for row in rows {
        csv.push_str(&format!(
            "\"{}\",\"{}\",\"{}\",\"{}\",{}\n",
            row.0,
            row.1.unwrap_or_default(),
            row.2,
            row.3.replace('"', "\"\""),
            row.4
        ));
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
    }))
    .unwrap_or_default())
}
