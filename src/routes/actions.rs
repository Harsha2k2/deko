use axum::{http::StatusCode, response::IntoResponse};

pub async fn list_actions() -> impl IntoResponse {
    (StatusCode::OK, "Actions list endpoint - coming soon")
}
