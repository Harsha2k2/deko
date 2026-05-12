use askama::Template;
use axum::extract::{Path, Query, State};
use axum::response::{Html, IntoResponse};
use axum::Json;
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::models::{Action, ActionStatus, AuditLog, Verdict};

type ActionRowResult = (String, String, String, String, Option<String>, Option<String>, Option<String>, String);

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    total_actions: i64,
    pending_actions: i64,
    denied_actions: i64,
    active_agents: i64,
    active_policies: i64,
    recent_actions: Vec<AdminActionRow>,
}

#[derive(Template)]
#[template(path = "actions_list.html")]
struct ActionsListTemplate {
    actions: Vec<AdminActionRow>,
}

#[derive(Template)]
#[template(path = "action_detail.html")]
struct ActionDetailTemplate {
    action: ActionDetailView,
    agent: Option<AgentView>,
    verdict: Option<VerdictView>,
    audit_log: Vec<AuditLogView>,
}

#[derive(Debug, Clone)]
struct AdminActionRow {
    id: String,
    agent_name: String,
    intent: String,
    status: String,
    verdict_decision: Option<String>,
    #[allow(dead_code)]
    verdict_reason: Option<String>,
    risk_level: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone)]
struct ActionDetailView {
    id: String,
    intent: String,
    payload: Option<String>,
    status: String,
    target_url: Option<String>,
    target_method: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone)]
struct AgentView {
    id: String,
    name: String,
    active: bool,
}

#[derive(Debug, Clone)]
struct VerdictView {
    decision: String,
    reason: String,
    risk_level: String,
}

#[derive(Debug, Clone)]
struct AuditLogView {
    event_type: String,
    details: String,
    created_at: String,
}

impl LoginTemplate {
    fn to_html(&self) -> String {
        self.render().unwrap_or_else(|e| format!("Template error: {}", e))
    }
}

impl DashboardTemplate {
    fn to_html(&self) -> String {
        self.render().unwrap_or_else(|e| format!("Template error: {}", e))
    }
}

impl ActionsListTemplate {
    fn to_html(&self) -> String {
        self.render().unwrap_or_else(|e| format!("Template error: {}", e))
    }
}

impl ActionDetailTemplate {
    fn to_html(&self) -> String {
        self.render().unwrap_or_else(|e| format!("Template error: {}", e))
    }
}

impl AuditLogTemplate {
    fn to_html(&self) -> String {
        self.render().unwrap_or_else(|e| format!("Template error: {}", e))
    }
}

impl AgentManagementTemplate {
    fn to_html(&self) -> String {
        self.render().unwrap_or_else(|e| format!("Template error: {}", e))
    }
}

impl PolicyManagementTemplate {
    fn to_html(&self) -> String {
        self.render().unwrap_or_else(|e| format!("Template error: {}", e))
    }
}

impl VerdictHistoryTemplate {
    fn to_html(&self) -> String {
        self.render().unwrap_or_else(|e| format!("Template error: {}", e))
    }
}

#[derive(Deserialize)]
pub struct AdminLoginRequest {
    pub password: String,
}

pub async fn admin_logout() -> axum::response::Redirect {
    axum::response::Redirect::to("/admin/login")
}

pub async fn admin_login_page() -> Html<String> {
    Html(LoginTemplate { error: None }.to_html())
}

use std::sync::atomic::{AtomicU64, Ordering};

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
    Json(req): Json<AdminLoginRequest>,
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

pub async fn dashboard(State(pool): State<crate::db::DbPool>) -> Html<String> {
    let total_actions: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM actions")
        .fetch_one(&pool).await.map_err(AppError::Database).unwrap_or((0,));

    let pending: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM actions WHERE status = 'pending'")
        .fetch_one(&pool).await.map_err(AppError::Database).unwrap_or((0,));

    let denied: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM actions WHERE status = 'denied'")
        .fetch_one(&pool).await.map_err(AppError::Database).unwrap_or((0,));

    let total_agents: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM agents WHERE active = 1")
        .fetch_one(&pool).await.map_err(AppError::Database).unwrap_or((0,));

    let total_policies: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM policies WHERE active = 1")
        .fetch_one(&pool).await.map_err(AppError::Database).unwrap_or((0,));

    let rows: Vec<ActionRowResult> = sqlx::query_as(
        "SELECT a.id, ag.name, a.intent, a.status, v.decision, v.reason, v.risk_level, a.created_at FROM actions a JOIN agents ag ON a.agent_id = ag.id LEFT JOIN verdicts v ON a.id = v.action_id ORDER BY a.created_at DESC LIMIT 20",
    )
    .fetch_all(&pool).await.unwrap_or_default();

    let recent_actions = rows.into_iter().map(|r| AdminActionRow {
        id: r.0,
        agent_name: r.1,
        intent: r.2,
        status: r.3,
        verdict_decision: r.4,
        verdict_reason: r.5,
        risk_level: r.6,
        created_at: r.7,
    }).collect();

    Html(DashboardTemplate {
        total_actions: total_actions.0,
        pending_actions: pending.0,
        denied_actions: denied.0,
        active_agents: total_agents.0,
        active_policies: total_policies.0,
        recent_actions,
    }.to_html())
}

#[derive(Deserialize)]
pub struct ActionsQuery {
    pub status: Option<String>,
}

pub async fn list_admin_actions(
    State(pool): State<crate::db::DbPool>,
    Query(params): Query<ActionsQuery>,
) -> Html<String> {
    let status_filter = params.status.clone();

    let mut query = "SELECT a.id, ag.name, a.intent, a.status, v.decision, v.reason, v.risk_level, a.created_at FROM actions a JOIN agents ag ON a.agent_id = ag.id LEFT JOIN verdicts v ON a.id = v.action_id WHERE 1=1".to_string();

    if let Some(s) = &status_filter {
        query.push_str(" AND a.status = '");
        query.push_str(s);
        query.push('\'');
    }

    query.push_str(" ORDER BY a.created_at DESC LIMIT 100");

    let rows: Vec<ActionRowResult> = sqlx::query_as(&query)
        .fetch_all(&pool).await.unwrap_or_default();

    let actions = rows.into_iter().map(|r| AdminActionRow {
        id: r.0,
        agent_name: r.1,
        intent: r.2,
        status: r.3,
        verdict_decision: r.4,
        verdict_reason: r.5,
        risk_level: r.6,
        created_at: r.7,
    }).collect();

    Html(ActionsListTemplate { actions }.to_html())
}

pub async fn get_admin_action_detail(
    State(pool): State<crate::db::DbPool>,
    Path(id): Path<String>,
) -> Html<String> {
    let action = sqlx::query_as::<_, Action>(
        "SELECT id, agent_id, intent, payload, screenshot_base64, metadata, status, target_url, target_method, created_at, updated_at FROM actions WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten();

    let action = match action {
        Some(a) => a,
        None => {
            return Html("<h1>Action not found</h1><a href=\"/admin/actions\">Back</a>".to_string());
        }
    };

    let agent = sqlx::query_as::<_, crate::models::Agent>(
        "SELECT id, name, api_key_hash, active, created_at, deactivated_reason, deactivated_at, api_key_expires_at FROM agents WHERE id = ?",
    )
    .bind(&action.agent_id)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten()
    .map(|a| AgentView {
        id: a.id,
        name: a.name,
        active: a.active,
    });

    let verdict = sqlx::query_as::<_, Verdict>(
        "SELECT id, action_id, decision, reason, risk_level, policy_matched, llm_raw_response, created_at FROM verdicts WHERE action_id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten()
    .map(|v| VerdictView {
        decision: format!("{:?}", v.decision).to_lowercase(),
        reason: v.reason,
        risk_level: format!("{:?}", v.risk_level).to_lowercase(),
    });

    let audit_logs: Vec<AuditLog> = sqlx::query_as(
        "SELECT id, action_id, event_type, details, created_at FROM audit_log WHERE action_id = ? ORDER BY created_at ASC",
    )
    .bind(&id)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();

    let audit_log = audit_logs.into_iter().map(|l| AuditLogView {
        event_type: l.event_type,
        details: l.details.to_string(),
        created_at: l.created_at.to_string(),
    }).collect();

    Html(ActionDetailTemplate {
        action: ActionDetailView {
            id: action.id,
            intent: action.intent,
            payload: action.payload,
            status: format!("{:?}", action.status).to_lowercase(),
            target_url: action.target_url,
            target_method: action.target_method,
            created_at: action.created_at.to_string(),
            updated_at: action.updated_at.to_string(),
        },
        agent,
        verdict,
        audit_log,
    }.to_html())
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

#[derive(Template)]
#[template(path = "audit_log.html")]
struct AuditLogTemplate {
    entries: Vec<AuditLogRow>,
    total: i64,
}

#[derive(Debug, Clone)]
struct AuditLogRow {
    #[allow(dead_code)]
    id: String,
    action_id: Option<String>,
    event_type: String,
    details: String,
    created_at: String,
}

pub async fn audit_log_viewer(
    State(pool): State<crate::db::DbPool>,
    Query(params): Query<AuditQuery>,
) -> Html<String> {
    let page = params.page.unwrap_or(1);
    let per_page = 50;
    let offset = (page - 1) * per_page;

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool).await.map_err(AppError::Database).unwrap_or((0,));

    let mut query = "SELECT id, action_id, event_type, details, created_at FROM audit_log WHERE 1=1".to_string();

    if let Some(action_id) = &params.action_id {
        query.push_str(" AND action_id = '");
        query.push_str(action_id);
        query.push('\'');
    }

    if let Some(event_type) = &params.event_type {
        query.push_str(" AND event_type = '");
        query.push_str(event_type);
        query.push('\'');
    }

    query.push_str(&format!(" ORDER BY created_at DESC LIMIT {} OFFSET {}", per_page, offset));

    let rows: Vec<(String, Option<String>, String, String, String)> = sqlx::query_as(&query)
        .fetch_all(&pool).await.unwrap_or_default();

    let entries = rows.into_iter().map(|r| AuditLogRow {
        id: r.0,
        action_id: r.1,
        event_type: r.2,
        details: r.3,
        created_at: r.4,
    }).collect();

    Html(AuditLogTemplate { entries, total: total.0 }.to_html())
}

#[derive(Deserialize)]
pub struct AuditQuery {
    pub action_id: Option<String>,
    pub event_type: Option<String>,
    pub page: Option<i64>,
}

#[derive(Template)]
#[template(path = "agent_management.html")]
struct AgentManagementTemplate {
    agents: Vec<AgentRow>,
}

#[derive(Debug, Clone)]
struct AgentRow {
    id: String,
    name: String,
    active: bool,
    created_at: String,
}

#[derive(Template)]
#[template(path = "policy_management.html")]
struct PolicyManagementTemplate {
    policies: Vec<PolicyRow>,
}

#[derive(Debug, Clone)]
struct PolicyRow {
    name: String,
    description: String,
    rules: String,
    active: bool,
    created_at: String,
}

#[derive(Template)]
#[template(path = "verdict_history.html")]
struct VerdictHistoryTemplate {
    verdicts: Vec<VerdictRow>,
}

#[derive(Debug, Clone)]
struct VerdictRow {
    action_id: String,
    decision: String,
    risk_level: Option<String>,
    reason: String,
    policy_matched: Option<String>,
    created_at: String,
}

pub async fn agent_management(State(pool): State<crate::db::DbPool>) -> Html<String> {
    let agents: Vec<(String, String, bool, String)> = sqlx::query_as(
        "SELECT id, name, active, created_at, deactivated_reason, deactivated_at, api_key_expires_at FROM agents ORDER BY created_at DESC",
    )
    .fetch_all(&pool).await.unwrap_or_default();

    let agents = agents.into_iter().map(|a| AgentRow {
        id: a.0,
        name: a.1,
        active: a.2,
        created_at: a.3,
    }).collect();

    Html(AgentManagementTemplate { agents }.to_html())
}

pub async fn policy_management(State(pool): State<crate::db::DbPool>) -> Html<String> {
    let policies: Vec<(String, String, String, String, bool, String, String)> = sqlx::query_as(
        "SELECT id, name, description, rules, active, created_at, updated_at FROM policies ORDER BY created_at DESC",
    )
    .fetch_all(&pool).await.unwrap_or_default();

    let policies = policies.into_iter().map(|p| PolicyRow {
        name: p.1,
        description: p.2,
        rules: p.3,
        active: p.4,
        created_at: p.5,
    }).collect();

    Html(PolicyManagementTemplate { policies }.to_html())
}

#[derive(Deserialize)]
pub struct VerdictsQuery {
    pub decision: Option<String>,
}

pub async fn verdict_history(State(pool): State<crate::db::DbPool>, Query(params): Query<VerdictsQuery>) -> Html<String> {
    let mut query = "SELECT action_id, decision, risk_level, reason, policy_matched, created_at FROM verdicts WHERE 1=1".to_string();

    if let Some(decision) = &params.decision {
        query.push_str(" AND decision = '");
        query.push_str(decision);
        query.push('\'');
    }

    query.push_str(" ORDER BY created_at DESC LIMIT 100");

    let rows: Vec<(String, String, String, String, Option<String>, String)> = sqlx::query_as(&query)
        .fetch_all(&pool).await.unwrap_or_default();

    let verdicts = rows.into_iter().map(|v| VerdictRow {
        action_id: v.0,
        decision: v.1,
        risk_level: Some(v.2),
        reason: v.3,
        policy_matched: v.4,
        created_at: v.5,
    }).collect();

    Html(VerdictHistoryTemplate { verdicts }.to_html())
}
