use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::error::{AppError, Result};

#[derive(Deserialize, ToSchema)]
pub struct CreatePolicyRequest {
    pub name: String,
    pub description: String,
    pub rules: serde_json::Value,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdatePolicyRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub rules: Option<serde_json::Value>,
    pub active: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/admin/policies",
    request_body = CreatePolicyRequest,
    responses((status = 201, description = "Policy created")),
    security(("AdminPassword" = []))
)]
pub async fn create_policy(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(_admin): axum::Extension<bool>,
    Json(req): Json<CreatePolicyRequest>,
) -> Result<Json<serde_json::Value>> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let rules_str =
        serde_json::to_string(&req.rules).map_err(|e| AppError::BadRequest(format!("Invalid rules JSON: {}", e)))?;

    sqlx::query("INSERT INTO policies (id, name, description, rules, active) VALUES (?, ?, ?, ?, 1)")
        .bind(&id)
        .bind(&req.name)
        .bind(&req.description)
        .bind(&rules_str)
        .execute(&pool)
        .await
        .map_err(AppError::Database)?;

    crate::services::audit::record(
        &pool,
        None,
        "policy_created",
        &serde_json::json!({ "policy_id": id, "name": req.name }),
    )
    .await?;

    Ok(Json(serde_json::json!({ "id": id, "name": req.name, "active": true })))
}

#[utoipa::path(
    get,
    path = "/admin/policies",
    responses((status = 200, description = "List policies")),
    security(("AdminPassword" = []))
)]
#[allow(dead_code)]
pub async fn list_policies(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(_admin): axum::Extension<bool>,
) -> Result<Json<serde_json::Value>> {
    let policies: Vec<(String, String, String, String, bool, String, String)> = sqlx::query_as(
        "SELECT id, name, description, rules, active, created_at, updated_at FROM policies ORDER BY created_at DESC",
    )
    .fetch_all(&pool)
    .await?;

    let policies = policies
        .into_iter()
        .map(|p| {
            serde_json::json!({
                "id": p.0,
                "name": p.1,
                "description": p.2,
                "rules": serde_json::from_str::<serde_json::Value>(&p.3).unwrap_or(serde_json::Value::Null),
                "active": p.4,
                "created_at": p.5,
                "updated_at": p.6,
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({ "policies": policies })))
}

#[utoipa::path(
    put,
    path = "/admin/policies/{id}",
    request_body = UpdatePolicyRequest,
    responses((status = 200, description = "Policy updated")),
    security(("AdminPassword" = []))
)]
pub async fn update_policy(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(_admin): axum::Extension<bool>,
    Path(id): Path<String>,
    Json(req): Json<UpdatePolicyRequest>,
) -> Result<Json<serde_json::Value>> {
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM policies WHERE id = ?")
        .bind(&id)
        .fetch_optional(&pool)
        .await
        .map_err(AppError::Database)?;

    if existing.is_none() {
        return Err(AppError::NotFound("Policy not found".into()));
    }

    let mut fields: Vec<String> = Vec::new();
    let mut values: Vec<String> = Vec::new();

    if let Some(name) = &req.name {
        fields.push("name = ?".to_string());
        values.push(name.clone());
    }
    if let Some(desc) = &req.description {
        fields.push("description = ?".to_string());
        values.push(desc.clone());
    }
    if let Some(rules) = &req.rules {
        fields.push("rules = ?".to_string());
        values.push(serde_json::to_string(rules).unwrap());
    }
    if let Some(active) = &req.active {
        fields.push("active = ?".to_string());
        values.push(if *active { "1".to_string() } else { "0".to_string() });
    }

    if fields.is_empty() {
        return Err(AppError::BadRequest("No fields to update".into()));
    }

    fields.push("updated_at = CURRENT_TIMESTAMP".to_string());

    let set_clause = fields.join(", ");
    let query = format!("UPDATE policies SET {} WHERE id = ?", set_clause);

    let mut q = sqlx::query(&query);
    for v in &values {
        q = q.bind(v);
    }
    q = q.bind(&id);
    q.execute(&pool).await.map_err(AppError::Database)?;

    crate::services::audit::record(&pool, None, "policy_updated", &serde_json::json!({  "policy_id": id })).await?;

    Ok(Json(serde_json::json!({ "updated": true, "id": id })))
}

#[utoipa::path(
    delete,
    path = "/admin/policies/{id}",
    responses((status = 200, description = "Policy deactivated")),
    security(("AdminPassword" = []))
)]
pub async fn delete_policy(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(_admin): axum::Extension<bool>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query("UPDATE policies SET active = 0, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(&id)
        .execute(&pool)
        .await
        .map_err(AppError::Database)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Policy not found".into()));
    }

    crate::services::audit::record(&pool, None, "policy_deleted", &serde_json::json!({  "policy_id": id })).await?;

    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}

#[derive(Deserialize)]
pub struct TestPolicyRequest {
    pub rules: serde_json::Value,
    pub intent: String,
    pub payload: Option<String>,
    pub target_url: Option<String>,
}

pub async fn test_policy(Json(req): Json<TestPolicyRequest>) -> Result<Json<serde_json::Value>> {
    let sample_action = crate::models::Action {
        id: "test".into(),
        agent_id: "test".into(),
        intent: req.intent,
        payload: req.payload,
        screenshot_base64: None,
        metadata: None,
        status: crate::models::ActionStatus::Pending,
        target_url: req.target_url,
        target_method: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        idempotency_key: None,
        priority: 5,
        execute_at: None,
    };

    let mut rules_value = req.rules.clone();
    let input = crate::services::policy_engine::RuleInput {
        intent: &sample_action.intent,
        payload: sample_action.payload.as_deref(),
        target_url: sample_action.target_url.as_deref(),
        target_method: sample_action.target_method.as_deref(),
        metadata: None,
    };

    let mut matched = false;
    let mut immediate_deny = false;
    let mut reason = "No rules matched".to_string();
    let mut risk_level: Option<&'static str> = None;

    if let Some(arr) = rules_value.as_array_mut() {
        crate::services::policy_engine::sort_rules(arr);
        for rule in arr.iter() {
            if let Some(outcome) = crate::services::policy_engine::evaluate_rule(rule, &input) {
                matched = true;
                immediate_deny |= outcome.immediate_deny;
                reason = outcome.message.clone();
                risk_level = Some(match outcome.risk_level {
                    crate::models::RiskLevel::Low => "low",
                    crate::models::RiskLevel::Medium => "medium",
                    crate::models::RiskLevel::High => "high",
                    crate::models::RiskLevel::Critical => "critical",
                });
                if outcome.immediate_deny {
                    break;
                }
            }
        }
    }

    Ok(Json(serde_json::json!({
        "matched": matched,
        "immediate_deny": immediate_deny,
        "reason": if matched { reason } else { "No rules matched".to_string() },
        "risk_level": risk_level,
    })))
}

#[derive(Deserialize)]
pub struct SimulateRequest {
    pub intent: String,
    pub payload: Option<String>,
    pub target_url: Option<String>,
}

pub async fn simulate_policies(
    State(pool): State<crate::db::DbPool>,
    Json(req): Json<SimulateRequest>,
) -> Result<Json<Vec<serde_json::Value>>> {
    let policies: Vec<crate::models::Policy> = sqlx::query_as(
        "SELECT id, name, description, rules, active, created_at, updated_at FROM policies WHERE active = 1",
    )
    .fetch_all(&pool)
    .await?;

    let mut results = Vec::new();

    for policy in &policies {
        let test_req = TestPolicyRequest {
            rules: policy.rules.clone(),
            intent: req.intent.clone(),
            payload: req.payload.clone(),
            target_url: req.target_url.clone(),
        };

        let result = test_policy_inner(&test_req);
        let (matched, immediate_deny, reason, risk_level) = match result {
            Some(r) => (r.matched, r.immediate_deny, r.reason, r.risk_level),
            None => (false, false, "No rules matched".to_string(), None),
        };

        results.push(serde_json::json!({
            "policy_id": policy.id,
            "policy_name": policy.name,
            "matched": matched,
            "immediate_deny": immediate_deny,
            "reason": reason,
            "risk_level": risk_level,
        }));
    }

    Ok(Json(results))
}

#[derive(serde::Serialize)]
struct SimulateRuleResult {
    matched: bool,
    immediate_deny: bool,
    reason: String,
    risk_level: Option<String>,
}

fn test_policy_inner(req: &TestPolicyRequest) -> Option<SimulateRuleResult> {
    let mut rules_value = req.rules.clone();
    let input = crate::services::policy_engine::RuleInput {
        intent: &req.intent,
        payload: req.payload.as_deref(),
        target_url: req.target_url.as_deref(),
        target_method: None,
        metadata: None,
    };

    let mut matched = false;
    let mut immediate_deny = false;
    let mut reason = String::new();
    let mut risk_level: Option<String> = None;

    if let Some(arr) = rules_value.as_array_mut() {
        crate::services::policy_engine::sort_rules(arr);
        for rule in arr.iter() {
            if let Some(outcome) = crate::services::policy_engine::evaluate_rule(rule, &input) {
                matched = true;
                immediate_deny |= outcome.immediate_deny;
                reason = outcome.message.clone();
                risk_level = Some(match outcome.risk_level {
                    crate::models::RiskLevel::Low => "low".into(),
                    crate::models::RiskLevel::Medium => "medium".into(),
                    crate::models::RiskLevel::High => "high".into(),
                    crate::models::RiskLevel::Critical => "critical".into(),
                });
                if outcome.immediate_deny {
                    break;
                }
            }
        }
    }

    if matched {
        Some(SimulateRuleResult {
            matched,
            immediate_deny,
            reason,
            risk_level,
        })
    } else {
        None
    }
}
