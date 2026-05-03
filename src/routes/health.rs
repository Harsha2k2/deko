use axum::{http::StatusCode, response::IntoResponse};
use serde_json::json;

pub async fn health() -> impl IntoResponse {
    let body = json!({
        "status": "healthy",
        "service": "deko",
        "version": env!("CARGO_PKG_VERSION"),
    });

    (StatusCode::OK, axum::Json(body))
}
