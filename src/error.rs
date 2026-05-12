use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::error;

static FIRST_INTERNAL: AtomicBool = AtomicBool::new(true);

fn log_first_backtrace() {
    if FIRST_INTERNAL.swap(false, Ordering::Relaxed) {
        let bt = std::backtrace::Backtrace::force_capture();
        error!(
            error.type = "internal",
            error.backtrace = %bt,
            "First internal error occurred"
        );
    }
}

/// Unified error type for Deko.
///
/// Maps to appropriate HTTP status codes via [`IntoResponse`]:
///
/// | Variant | HTTP Status | Use Case |
/// |---|---|---|
/// | `Database` | 500 | SQLx errors |
/// | `NotFound` | 404 | Missing resources |
/// | `Unauthorized` | 401 | Bad API key or session |
/// | `Forbidden` | 403 | Action denied by policy |
/// | `BadRequest` | 400 | Malformed input |
/// | `Internal` | 500 | Unexpected failures |
/// | `OpenAI` | 500 | OpenAI API errors |
/// | `Gemini` | 500 | Gemini API errors |
/// | `Validation` | 422 | Input validation failures |
/// | `RateLimited` | 429 | Too many requests |
/// | `Locked` | 423 | Escalated action not yet resolved |
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal server error")]
    Internal,

    #[error("OpenAI API error: {0}")]
    OpenAI(String),

    #[error("Gemini API error: {0}")]
    #[allow(dead_code)]
    Gemini(String),

    #[allow(dead_code)]
    #[error("Validation error: {0}")]
    Validation(String),

    #[allow(dead_code)]
    #[error("Rate limited")]
    RateLimited,

    #[error("Action locked: {0}")]
    Locked(String),
}

fn is_production() -> bool {
    std::env::var("DEKO_ENV").as_deref() == Ok("prod")
}

fn sanitize_for_prod(msg: &str) -> String {
    if is_production() {
        "An internal error occurred".to_string()
    } else {
        msg.to_string()
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Database(e) => {
                error!(
                    error.type = "database",
                    error.message = %e,
                    "Database error"
                );
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Internal => {
                log_first_backtrace();
                (StatusCode::INTERNAL_SERVER_ERROR, sanitize_for_prod("Internal server error"))
            }
            AppError::OpenAI(msg) => {
                log_first_backtrace();
                (StatusCode::INTERNAL_SERVER_ERROR, sanitize_for_prod(msg))
            }
            AppError::Gemini(msg) => {
                log_first_backtrace();
                (StatusCode::INTERNAL_SERVER_ERROR, sanitize_for_prod(msg))
            }
            AppError::Validation(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg.clone()),
            AppError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "Rate limited".to_string()),
            AppError::Locked(msg) => (StatusCode::LOCKED, msg.clone()),
        };

        let body = serde_json::json!({
            "error": message,
        });

        (status, axum::Json(body)).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
