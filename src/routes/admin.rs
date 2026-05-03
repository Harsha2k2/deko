use axum::{http::StatusCode, response::IntoResponse};

pub async fn dashboard() -> impl IntoResponse {
    (StatusCode::OK, "Admin dashboard - coming soon")
}
