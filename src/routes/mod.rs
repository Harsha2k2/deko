mod actions;
mod admin;
mod health;

use axum::Router;
use sqlx::SqlitePool;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::config::Config;

#[derive(OpenApi)]
#[openapi(info(title = "Deko API", version = "0.1.0"))]
pub struct ApiDoc;

pub fn create_router(_config: &Config, pool: SqlitePool) -> anyhow::Result<Router> {
    info!("Setting up router");

    let cors = CorsLayer::new();

    let app = Router::new()
        .route("/health", axum::routing::get(health::health))
        .route("/admin", axum::routing::get(admin::dashboard))
        .route("/actions", axum::routing::get(actions::list_actions))
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(pool);

    Ok(app)
}
