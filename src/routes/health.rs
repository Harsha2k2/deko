use axum::extract::State;
use axum::http::StatusCode;
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
    pub openai: String,
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

    let openai_status = check_openai().await;

    let response = HealthResponse {
        status: if db_status == "healthy" { "healthy" } else { "degraded" }.to_string(),
        service: "deko".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database: db_status.to_string(),
        openai: openai_status.to_string(),
    };

    if response.status == "healthy" {
        Ok(Json(response))
    } else {
        Err(AppError::Internal)
    }
}

pub async fn readiness(State(pool): State<SqlitePool>) -> Result<Json<serde_json::Value>, AppError> {
    match sqlx::query("SELECT 1").fetch_one(&pool).await {
        Ok(_) => Ok(Json(serde_json::json!({ "status": "ready" }))),
        Err(_) => Err(AppError::Internal),
    }
}

pub async fn liveness() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "alive" }))
}

async fn check_openai() -> &'static str {
    let api_key = match std::env::var("OPENAI_API_KEY") {
        Ok(k) if k.starts_with("sk-") => k,
        _ => return "not_configured",
    };

    let client = reqwest::Client::new();
    match client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => "healthy",
        _ => "unhealthy",
    }
}
