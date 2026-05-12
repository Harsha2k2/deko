use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use sqlx::SqlitePool;
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
    State(pool): State<SqlitePool>,
    axum::Extension(_admin): axum::Extension<bool>,
    Json(req): Json<CreatePolicyRequest>,
) -> Result<Json<serde_json::Value>> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let rules_str = serde_json::to_string(&req.rules)
        .map_err(|e| AppError::BadRequest(format!("Invalid rules JSON: {}", e)))?;

    sqlx::query(
        "INSERT INTO policies (id, name, description, rules, active) VALUES (?, ?, ?, ?, 1)",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&rules_str)
    .execute(&pool)
    .await
    .map_err(AppError::Database)?;

    sqlx::query(
        "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, NULL, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind("policy_created")
    .bind(serde_json::json!({ "policy_id": id, "name": req.name }).to_string())
    .execute(&pool)
    .await
    .map_err(AppError::Database)?;

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
    State(pool): State<SqlitePool>,
    axum::Extension(_admin): axum::Extension<bool>,
) -> Result<Json<serde_json::Value>> {
    let policies: Vec<(String, String, String, String, bool, String, String)> = sqlx::query_as(
        "SELECT id, name, description, rules, active, created_at, updated_at FROM policies ORDER BY created_at DESC",
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::Database)?;

    let policies = policies.into_iter().map(|p| {
        serde_json::json!({
            "id": p.0,
            "name": p.1,
            "description": p.2,
            "rules": serde_json::from_str::<serde_json::Value>(&p.3).unwrap_or(serde_json::Value::Null),
            "active": p.4,
            "created_at": p.5,
            "updated_at": p.6,
        })
    }).collect::<Vec<_>>();

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
    State(pool): State<SqlitePool>,
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

    fields.push("updated_at = datetime('now')".to_string());

    let set_clause = fields.join(", ");
    let query = format!("UPDATE policies SET {} WHERE id = ?", set_clause);

    let mut q = sqlx::query(&query);
    for v in &values {
        q = q.bind(v);
    }
    q = q.bind(&id);
    q.execute(&pool).await.map_err(AppError::Database)?;

    sqlx::query(
        "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, NULL, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind("policy_updated")
    .bind(serde_json::json!({ "policy_id": id }).to_string())
    .execute(&pool)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({ "updated": true, "id": id })))
}

#[utoipa::path(
    delete,
    path = "/admin/policies/{id}",
    responses((status = 200, description = "Policy deactivated")),
    security(("AdminPassword" = []))
)]
pub async fn delete_policy(
    State(pool): State<SqlitePool>,
    axum::Extension(_admin): axum::Extension<bool>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let result = sqlx::query("UPDATE policies SET active = 0, updated_at = datetime('now') WHERE id = ?")
        .bind(&id)
        .execute(&pool)
        .await
        .map_err(AppError::Database)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Policy not found".into()));
    }

    sqlx::query(
        "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, NULL, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind("policy_deleted")
    .bind(serde_json::json!({ "policy_id": id }).to_string())
    .execute(&pool)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}
