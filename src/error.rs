use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tracing::error;

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
    Gemini(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("Action locked: {0}")]
    Locked(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::Database(e) => {
                error!("Database error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string()),
            AppError::OpenAI(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AppError::Gemini(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
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
