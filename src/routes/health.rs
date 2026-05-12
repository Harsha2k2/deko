use axum::extract::State;
use axum::Json;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::error::AppError;

#[derive(serde::Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
    pub database: String,
    pub llm: String,
}

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Health check", body = HealthResponse),
        (status = 503, description = "Service unhealthy"),
    )
)]
pub async fn health(State(pool): State<SqlitePool>) -> Result<Json<HealthResponse>, AppError> {
    let db_status = match sqlx::query("SELECT 1").fetch_one(&pool).await {
        Ok(_) => "healthy",
        Err(_) => "unhealthy",
    };

    let llm_status = check_llm().await;

    let response = HealthResponse {
        status: if db_status == "healthy" { "healthy" } else { "degraded" }.to_string(),
        service: "deko".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database: db_status.to_string(),
        llm: llm_status.to_string(),
    };

    if response.status == "healthy" {
        Ok(Json(response))
    } else {
        Err(AppError::Internal)
    }
}

#[utoipa::path(
    get,
    path = "/health/ready",
    responses(
        (status = 200, description = "Ready"),
        (status = 503, description = "Not ready"),
    )
)]
pub async fn readiness(State(pool): State<SqlitePool>) -> Result<Json<serde_json::Value>, AppError> {
    match sqlx::query("SELECT 1").fetch_one(&pool).await {
        Ok(_) => Ok(Json(serde_json::json!({ "status": "ready" }))),
        Err(_) => Err(AppError::Internal),
    }
}

#[utoipa::path(
    get,
    path = "/health/live",
    responses(
        (status = 200, description = "Alive"),
    )
)]
pub async fn liveness() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "alive" }))
}

async fn check_llm() -> &'static str {
    let gemini_key = std::env::var("GEMINI_API_KEY").ok();
    let openai_key = std::env::var("OPENAI_API_KEY").ok();

    if gemini_key.is_none() && openai_key.is_none() {
        return "not_configured";
    }

    if let Some(key) = gemini_key {
        if !key.is_empty() {
            let client = reqwest::Client::new();
            match client
                .get(format!(
                    "https://generativelanguage.googleapis.com/v1beta/models?key={}",
                    key
                ))
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => return "healthy",
                _ => {}
            }
        }
    }

    if let Some(key) = openai_key {
        if key.starts_with("sk-") {
            let client = reqwest::Client::new();
            match client
                .get("https://api.openai.com/v1/models")
                .header("Authorization", format!("Bearer {}", key))
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => return "healthy",
                _ => {}
            }
        }
    }

    "unhealthy"
}
