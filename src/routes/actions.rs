use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::error::{AppError, Result};
use crate::models::{Action, ActionStatus, Agent, Verdict, VerdictDecision, VerdictResponse};

fn sanitize_input(input: &str, max_len: usize) -> String {
    let truncated = if input.len() > max_len {
        &input[..max_len]
    } else {
        input
    };
    truncated
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
        .replace('&', "&amp;")
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateActionRequest {
    pub intent: String,
    pub payload: Option<String>,
    pub screenshot_base64: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub target_url: Option<String>,
    pub target_method: Option<String>,
    pub idempotency_key: Option<String>,
    pub execute_at: Option<String>,
    pub priority: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateActionResponse {
    pub id: String,
    pub status: ActionStatus,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ActionDetailResponse {
    pub id: String,
    pub agent_id: String,
    pub intent: String,
    pub payload: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub status: ActionStatus,
    pub target_url: Option<String>,
    pub target_method: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub verdict: Option<VerdictResponse>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListActionsQuery {
    pub status: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListActionsResponse {
    pub actions: Vec<ActionDetailResponse>,
    pub total: i64,
}

/// Submit a new action for security review.
///
/// The action is saved with status `pending` and processed asynchronously.
/// The caller receives an `action_id` to poll for the verdict.
#[utoipa::path(
    post,
    path = "/action",
    tag = "actions",
    request_body = CreateActionRequest,
    responses(
        (status = 201, description = "Action created", body = CreateActionResponse),
        (status = 401, description = "Invalid or missing API key"),
    ),
    security(("ApiKey" = []))
)]
pub async fn create_action(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(agent): axum::Extension<Agent>,
    Json(req): Json<CreateActionRequest>,
) -> Result<(StatusCode, Json<CreateActionResponse>)> {
    if req.intent.trim().is_empty() {
        return Err(AppError::BadRequest("intent is required".into()));
    }

    let sanitized_intent = sanitize_input(&req.intent, 500);

    if let Some(ref screenshot) = req.screenshot_base64 {
        let size_bytes = screenshot.len();
        let max_bytes = 10 * 1024 * 1024;
        if size_bytes > max_bytes {
            return Err(AppError::BadRequest(
                format!("Screenshot too large: {} bytes exceeds {} MB limit", size_bytes, 10),
            ));
        }
    }

    if let Some(ref url) = req.target_url {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(AppError::BadRequest("target_url must start with http:// or https://".into()));
        }
    }

    if let Some(ref ik) = req.idempotency_key {
        let existing: Option<(String, String)> = sqlx::query_as(
            "SELECT id, status FROM actions WHERE agent_id = ? AND idempotency_key = ?",
        )
        .bind(&agent.id)
        .bind(ik)
        .fetch_optional(&pool)
        .await
        .map_err(AppError::Database)?;

        if let Some((existing_id, existing_status)) = existing {
            let status = match existing_status.as_str() {
                "pending" => ActionStatus::Pending,
                "processing" => ActionStatus::Processing,
                "approved" => ActionStatus::Approved,
                "denied" => ActionStatus::Denied,
                "escalated" => ActionStatus::Escalated,
                "forwarded" => ActionStatus::Forwarded,
                _ => ActionStatus::Pending,
            };
            return Ok((
                StatusCode::OK,
                Json(CreateActionResponse {
                    id: existing_id,
                    status,
                }),
            ));
        }
    }

    let id = uuid::Uuid::new_v4().to_string();

    let metadata_str = req.metadata.as_ref().map(|m| m.to_string());

    sqlx::query(
        "INSERT INTO actions (id, agent_id, intent, payload, screenshot_base64, metadata, target_url, target_method, status, idempotency_key, priority, execute_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&agent.id)
    .bind(&sanitized_intent)
    .bind(&req.payload)
    .bind(&req.screenshot_base64)
    .bind(&metadata_str)
    .bind(&req.target_url)
    .bind(&req.target_method)
    .bind(ActionStatus::Pending)
    .bind(&req.idempotency_key)
    .bind(req.priority.unwrap_or(5))
    .bind(&req.execute_at)
    .execute(&pool)
    .await
    .map_err(AppError::Database)?;

    sqlx::query(
        "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&id)
    .bind("action_created")
    .bind(serde_json::json!({
        "agent_id": agent.id,
        "agent_name": agent.name,
        "intent": sanitized_intent,
    }))
    .execute(&pool)
    .await
    .map_err(AppError::Database)?;

    Ok((
        StatusCode::CREATED,
        Json(CreateActionResponse {
            id,
            status: ActionStatus::Pending,
        }),
    ))
}

#[utoipa::path(
    get,
    path = "/action/{id}",
    params(
        ("id" = String, Path, description = "Action ID"),
    ),
    responses(
        (status = 200, description = "Action detail", body = ActionDetailResponse),
        (status = 404, description = "Action not found"),
    ),
    security(("ApiKey" = []))
)]
pub async fn get_action(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(agent): axum::Extension<Agent>,
    Path(id): Path<String>,
) -> Result<Json<ActionDetailResponse>> {
    let action = sqlx::query_as::<_, Action>(
        "SELECT id, agent_id, intent, payload, screenshot_base64, metadata, status, target_url, target_method, created_at, updated_at, idempotency_key, priority, execute_at FROM actions WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("Action not found".into()))?;

    if action.agent_id != agent.id {
        return Err(AppError::Forbidden("Action belongs to another agent".into()));
    }

    let verdict = sqlx::query_as::<_, Verdict>(
        "SELECT id, action_id, decision, reason, risk_level, policy_matched, llm_raw_response, created_at FROM verdicts WHERE action_id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Database)?
    .map(|v| VerdictResponse {
        id: v.id,
        action_id: v.action_id,
        decision: v.decision,
        reason: v.reason,
        risk_level: v.risk_level,
        created_at: v.created_at,
    });

    let metadata = action.metadata.and_then(|m| serde_json::from_str(&m).ok());

    Ok(Json(ActionDetailResponse {
        id: action.id,
        agent_id: action.agent_id,
        intent: action.intent,
        payload: action.payload,
        metadata,
        status: action.status,
        target_url: action.target_url,
        target_method: action.target_method,
        created_at: action.created_at,
        updated_at: action.updated_at,
        verdict,
    }))
}

#[utoipa::path(
    get,
    path = "/action/{id}/status",
    params(
        ("id" = String, Path, description = "Action ID"),
    ),
    responses(
        (status = 200, description = "Action status"),
        (status = 404, description = "Action not found"),
    ),
    security(("ApiKey" = []))
)]
pub async fn get_action_status(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(agent): axum::Extension<Agent>,
    Path(id): Path<String>,
) -> Result<axum::response::Response> {
    use axum::http::HeaderValue;

    let action = sqlx::query_as::<_, Action>(
        "SELECT id, agent_id, intent, payload, screenshot_base64, metadata, status, target_url, target_method, created_at, updated_at, idempotency_key, priority, execute_at FROM actions WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("Action not found".into()))?;

    if action.agent_id != agent.id {
        return Err(AppError::Forbidden("Action belongs to another agent".into()));
    }

    let verdict = sqlx::query_as::<_, Verdict>(
        "SELECT id, action_id, decision, reason, risk_level, policy_matched, llm_raw_response, created_at FROM verdicts WHERE action_id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Database)?;

    let body = if let Some(ref v) = verdict {
        serde_json::json!({
            "action_id": id,
            "status": action.status,
            "verdict": {
                "decision": v.decision,
                "reason": v.reason,
                "risk_level": v.risk_level,
            }
        })
    } else {
        serde_json::json!({
            "action_id": id,
            "status": "pending",
            "retry_after": 5,
        })
    };

    let mut response = axum::Json(body).into_response();
    if verdict.is_none() {
        response.headers_mut().insert(
            "Retry-After",
            HeaderValue::from_static("5"),
        );
    }

    Ok(response)
}

#[derive(Debug, Deserialize)]
pub struct BatchActionRequest {
    pub actions: Vec<CreateActionRequest>,
}

pub async fn batch_create_actions(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(agent): axum::Extension<Agent>,
    Json(req): Json<BatchActionRequest>,
) -> Result<(StatusCode, Json<Vec<serde_json::Value>>)> {
    if req.actions.len() > 50 {
        return Err(AppError::BadRequest("Maximum 50 actions per batch".into()));
    }

    let mut results = Vec::new();
    for action_req in req.actions {
        let id = uuid::Uuid::new_v4().to_string();
        let sanitized_intent = sanitize_input(&action_req.intent, 500);

        sqlx::query(
        "INSERT INTO actions (id, agent_id, intent, payload, screenshot_base64, metadata, target_url, target_method, status, idempotency_key, priority, execute_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&agent.id)
        .bind(&sanitized_intent)
        .bind(&action_req.payload)
        .bind(&action_req.screenshot_base64)
        .bind(&action_req.metadata.as_ref().map(|m| m.to_string()))
        .bind(&action_req.target_url)
        .bind(&action_req.target_method)
        .bind(ActionStatus::Pending)
        .bind(&action_req.idempotency_key)
        .bind(action_req.priority.unwrap_or(5))
        .execute(&pool)
        .await
        .map_err(AppError::Database)?;

        sqlx::query(
            "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&id)
        .bind("action_created")
        .bind(serde_json::json!({"agent_id": agent.id, "intent": sanitized_intent, "batch": true}))
        .execute(&pool)
        .await
        .ok();

        results.push(serde_json::json!({
            "id": id,
            "status": "pending",
            "intent": sanitized_intent,
        }));
    }

    Ok((StatusCode::CREATED, Json(results)))
}

#[utoipa::path(
    get,
    path = "/actions",
    tag = "actions",
    params(
        ("status" = Option<String>, Query, description = "Filter by status"),
        ("limit" = Option<i32>, Query, description = "Max results"),
        ("offset" = Option<i32>, Query, description = "Offset for pagination"),
    ),
    responses(
        (status = 200, description = "List actions", body = ListActionsResponse),
    ),
    security(("ApiKey" = []))
)]
pub async fn list_actions(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(agent): axum::Extension<Agent>,
    Query(params): Query<ListActionsQuery>,
) -> Result<Json<ListActionsResponse>> {
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = params.offset.unwrap_or(0);

    let mut query = "SELECT id, agent_id, intent, payload, screenshot_base64, metadata, status, target_url, target_method, created_at, updated_at, idempotency_key, priority, execute_at FROM actions WHERE agent_id = ?".to_string();
    let mut count_query = "SELECT COUNT(*) FROM actions WHERE agent_id = ?".to_string();
    let mut binds: Vec<&str> = vec![&agent.id];

    if let Some(status) = &params.status {
        query.push_str(" AND status = ?");
        count_query.push_str(" AND status = ?");
        binds.push(status);
    }

    query.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
    binds.push("0");

    let actions: Vec<Action> = sqlx::query_as::<_, Action>(&query)
        .bind(&agent.id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&pool)
        .await
        .map_err(AppError::Database)?;

    let total: (i64,) = sqlx::query_as(&count_query)
        .bind(&agent.id)
        .fetch_one(&pool)
        .await
        .map_err(AppError::Database)?;

    let actions = actions
        .into_iter()
        .map(|a| ActionDetailResponse {
            id: a.id,
            agent_id: a.agent_id,
            intent: a.intent,
            payload: a.payload,
            metadata: a.metadata.and_then(|m| serde_json::from_str(&m).ok()),
            status: a.status,
            target_url: a.target_url,
            target_method: a.target_method,
            created_at: a.created_at,
            updated_at: a.updated_at,
            verdict: None,
        })
        .collect();

    Ok(Json(ListActionsResponse {
        actions,
        total: total.0,
    }))
}

#[utoipa::path(
    post,
    path = "/action/{id}/forward",
    params(
        ("id" = String, Path, description = "Action ID"),
    ),
    responses(
        (status = 200, description = "Action forwarded"),
        (status = 403, description = "Action denied"),
        (status = 423, description = "Action locked - escalated"),
    ),
    security(("ApiKey" = []))
)]
pub async fn forward_action(
    State(pool): State<crate::db::DbPool>,
    axum::Extension(agent): axum::Extension<Agent>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let action = sqlx::query_as::<_, Action>(
        "SELECT id, agent_id, intent, payload, screenshot_base64, metadata, status, target_url, target_method, created_at, updated_at, idempotency_key, priority, execute_at FROM actions WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound("Action not found".into()))?;

    if action.agent_id != agent.id {
        return Err(AppError::Forbidden("Action belongs to another agent".into()));
    }

    let verdict = sqlx::query_as::<_, Verdict>(
        "SELECT id, action_id, decision, reason, risk_level, policy_matched, llm_raw_response, created_at FROM verdicts WHERE action_id = ?",
    )
    .bind(&id)
    .fetch_optional(&pool)
    .await
    .map_err(AppError::Database)?;

    let verdict = verdict.ok_or_else(|| AppError::BadRequest("No verdict available yet".into()))?;

    match verdict.decision {
        VerdictDecision::Approved => {
            sqlx::query("UPDATE actions SET status = ? WHERE id = ?")
                .bind(ActionStatus::Forwarded)
                .bind(&id)
                .execute(&pool)
                .await
                .map_err(AppError::Database)?;

            let response = if let (Some(url), Some(method)) = (&action.target_url, &action.target_method) {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                    .map_err(|_| AppError::Internal)?;

                let body = action.payload.clone().unwrap_or_default();
                let decision_str = format!("{}", verdict.decision);

                let mut last_error = None;
                let mut success = None;

                for attempt in 1..=3 {
                    let req_builder = match method.to_uppercase().as_str() {
                        "POST" => client.post(url).body(body.clone()),
                        "DELETE" => client.delete(url),
                        "PUT" => client.put(url).body(body.clone()),
                        "PATCH" => client.patch(url).body(body.clone()),
                        _ => client.get(url),
                    }
                    .header("X-Deko-Action-Id", &id)
                    .header("X-Deko-Agent-Id", &agent.id)
                    .header("X-Deko-Verdict", &decision_str);

                    match req_builder.send().await {
                        Ok(r) if r.status().is_success() => {
                            let status = r.status().as_u16();
                            let resp_body = r.text().await.unwrap_or_default();
                            success = Some((status, resp_body, attempt));
                            break;
                        }
                        Ok(r) => {
                            last_error = Some(format!("HTTP {}", r.status()));
                        }
                        Err(e) => {
                            last_error = Some(e.to_string());
                        }
                    }

                    if attempt < 3 {
                        tokio::time::sleep(std::time::Duration::from_millis(500 * attempt as u64)).await;
                    }
                }

                if let Some((status, resp_body, attempts)) = success {
                    serde_json::json!({
                        "forwarded": true,
                        "target_status": status,
                        "target_response": resp_body,
                        "forward_attempts": attempts,
                    })
                } else {
                    serde_json::json!({
                        "forwarded": true,
                        "forward_error": last_error.unwrap_or_else(|| "Max retries exceeded".to_string()),
                        "forward_attempts": 3,
                    })
                }
            } else {
                serde_json::json!({ "forwarded": true, "note": "No target URL configured" })
            };

            Ok(Json(response))
        }
        VerdictDecision::Denied => Err(AppError::Forbidden(format!("Action denied: {}", verdict.reason))),
        VerdictDecision::Escalate => Err(AppError::Locked(format!(
            "Action requires human review: {}",
            verdict.reason
        ))),
    }
}
