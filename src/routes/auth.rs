use axum::extract::{State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::{AppError, Result};
use crate::middleware::auth::hash_api_key;
use crate::models::{Agent, CreateAgentResponse};

#[derive(Deserialize, ToSchema)]
pub struct RegisterAgentRequest {
    pub name: String,
}

#[utoipa::path(
    post,
    path = "/auth/register",
    request_body = RegisterAgentRequest,
    responses(
        (status = 201, description = "Agent registered", body = CreateAgentResponse),
        (status = 400, description = "Bad request"),
    )
)]
pub async fn register_agent(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(admin): axum::Extension<bool>,
    Json(req): Json<RegisterAgentRequest>,
) -> Result<(StatusCode, Json<CreateAgentResponse>)> {
    if !admin {
        return Err(AppError::Forbidden("Admin access required".into()));
    }

    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let api_key = uuid::Uuid::new_v4().to_string();

    let secret = std::env::var("DEKO_API_KEY_SECRET")
        .map_err(|_| AppError::Internal)?;
    let api_key_hash = hash_api_key(&api_key, &secret);

    sqlx::query(
        "INSERT INTO agents (id, name, api_key_hash, active) VALUES (?, ?, ?, 1)",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&api_key_hash)
    .execute(&pool)
    .await
    .map_err(AppError::Database)?;

    sqlx::query(
        "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind::<Option<String>>(None)
    .bind("agent_registered")
    .bind(serde_json::json!({ "agent_id": id, "agent_name": req.name }))
    .execute(&pool)
    .await
    .map_err(AppError::Database)?;

    let now = chrono::Utc::now();

    Ok((
        StatusCode::CREATED,
        Json(CreateAgentResponse {
            id,
            name: req.name,
            api_key,
            created_at: now,
        }),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct RevokeAgentRequest {
    pub agent_id: String,
    pub reason: Option<String>,
}

#[utoipa::path(
    post,
    path = "/auth/revoke",
    request_body = RevokeAgentRequest,
    responses(
        (status = 200, description = "Agent revoked"),
        (status = 404, description = "Agent not found"),
    )
)]
pub async fn revoke_agent(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(admin): axum::Extension<bool>,
    Json(req): Json<RevokeAgentRequest>,
) -> Result<Json<serde_json::Value>> {
    if !admin {
        return Err(AppError::Forbidden("Admin access required".into()));
    }

    let result = sqlx::query("UPDATE agents SET active = 0, deactivated_reason = ?, deactivated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(&req.reason)
        .bind(&req.agent_id)
        .execute(&pool)
        .await
        .map_err(AppError::Database)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Agent not found".into()));
    }

    sqlx::query(
        "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&req.agent_id)
    .bind("agent_revoked")
    .bind(serde_json::json!({"reason": req.reason}))
    .execute(&pool)
    .await
    .ok();

    Ok(Json(serde_json::json!({ "revoked": true, "agent_id": req.agent_id })))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListAgentsResponse {
    pub agents: Vec<AgentSummary>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AgentSummary {
    pub id: String,
    pub name: String,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    get,
    path = "/admin/agents",
    responses(
        (status = 200, description = "List agents", body = ListAgentsResponse),
    )
)]
#[allow(dead_code)]
pub async fn list_agents(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(admin): axum::Extension<bool>,
) -> Result<Json<ListAgentsResponse>> {
    if !admin {
        return Err(AppError::Forbidden("Admin access required".into()));
    }

    let agents: Vec<Agent> = sqlx::query_as(
        "SELECT id, name, api_key_hash, active, created_at, deactivated_reason, deactivated_at, api_key_expires_at FROM agents ORDER BY created_at DESC",
    )
    .fetch_all(&pool)
    .await
    .map_err(AppError::Database)?;

    let agents = agents
        .into_iter()
        .map(|a| AgentSummary {
            id: a.id,
            name: a.name,
            active: a.active,
            created_at: a.created_at,
        })
        .collect();

    Ok(Json(ListAgentsResponse { agents }))
}

#[derive(Deserialize, ToSchema)]
pub struct RotateApiKeyRequest {
    pub agent_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct RotateApiKeyResponse {
    pub agent_id: String,
    pub new_api_key: String,
    pub rotated_at: chrono::DateTime<chrono::Utc>,
}

#[utoipa::path(
    post,
    path = "/admin/agents/rotate-key",
    request_body = RotateApiKeyRequest,
    responses(
        (status = 200, description = "API key rotated", body = RotateApiKeyResponse),
        (status = 404, description = "Agent not found"),
    )
)]
pub async fn rotate_agent_key(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(admin): axum::Extension<bool>,
    Json(req): Json<RotateApiKeyRequest>,
) -> Result<(StatusCode, Json<RotateApiKeyResponse>)> {
    if !admin {
        return Err(AppError::Forbidden("Admin access required".into()));
    }

    let new_api_key = uuid::Uuid::new_v4().to_string();

    let secret = std::env::var("DEKO_API_KEY_SECRET")
        .map_err(|_| AppError::Internal)?;
    let new_hash = hash_api_key(&new_api_key, &secret);

    let result = sqlx::query("UPDATE agents SET api_key_hash = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ? AND active = 1")
        .bind(&new_hash)
        .bind(&req.agent_id)
        .execute(&pool)
        .await
        .map_err(AppError::Database)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Agent not found or inactive".into()));
    }

    sqlx::query(
        "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind::<Option<String>>(None)
    .bind("api_key_rotated")
    .bind(serde_json::json!({ "agent_id": req.agent_id }))
    .execute(&pool)
    .await
    .map_err(AppError::Database)?;

    let now = chrono::Utc::now();

    Ok((
        StatusCode::OK,
        Json(RotateApiKeyResponse {
            agent_id: req.agent_id,
            new_api_key,
            rotated_at: now,
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
    pub agent_id: String,
    pub label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
    pub api_key: String,
    pub label: String,
}

pub async fn create_api_key(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(admin): axum::Extension<bool>,
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>> {
    if !admin {
        return Err(AppError::Forbidden("Admin access required".into()));
    }

    let agent_exists: (bool,) = sqlx::query_as(
        "SELECT COUNT(*) > 0 FROM agents WHERE id = ? AND active = 1",
    )
    .bind(&req.agent_id)
    .fetch_one(&pool)
    .await
    .map_err(AppError::Database)?;

    if !agent_exists.0 {
        return Err(AppError::NotFound("Active agent not found".into()));
    }

    let api_key = uuid::Uuid::new_v4().to_string();
    let raw_key = format!("{}-{}", req.agent_id, api_key);
    let label = req.label.unwrap_or_else(|| "additional-key".to_string());
    let key_hash = crate::middleware::auth::hash_api_key(&raw_key, &std::env::var("DEKO_API_KEY_SECRET").unwrap_or_default());

    sqlx::query(
        "INSERT INTO api_keys (id, agent_id, key_hash, label) VALUES (?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&req.agent_id)
    .bind(&key_hash)
    .bind(&label)
    .execute(&pool)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(CreateApiKeyResponse { api_key: raw_key, label }))
}

#[derive(Debug, Deserialize)]
pub struct ListApiKeysRequest {
    pub agent_id: String,
}

#[derive(Debug, Serialize)]
pub struct ApiKeySummary {
    pub id: String,
    pub label: String,
    pub active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list_api_keys(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(admin): axum::Extension<bool>,
    Json(req): Json<ListApiKeysRequest>,
) -> Result<Json<Vec<ApiKeySummary>>> {
    if !admin {
        return Err(AppError::Forbidden("Admin access required".into()));
    }

    let keys: Vec<(String, String, bool, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT id, label, active, created_at FROM api_keys WHERE agent_id = ? ORDER BY created_at DESC",
    )
    .bind(&req.agent_id)
    .fetch_all(&pool)
    .await
    .map_err(AppError::Database)?;

    Ok(Json(keys.into_iter().map(|(id, label, active, created_at)| ApiKeySummary { id, label, active, created_at }).collect()))
}
