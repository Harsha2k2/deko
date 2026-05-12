mod actions;
mod admin;
mod auth;
mod health;
mod policies;

use axum::Router;
use std::sync::Arc;
use crate::db::{DbPool, DbPoolSet};
use tower_http::trace::TraceLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::config::Config;
use crate::services::metrics::{MetricsCollector, RateLimiter};

#[derive(OpenApi)]
#[openapi(
    info(title = "Deko API", version = "0.1.0"),
    paths(
        auth::register_agent,
        auth::revoke_agent,
        auth::list_agents,
        auth::rotate_agent_key,
        actions::create_action,
        actions::get_action,
        actions::get_action_status,
        actions::list_actions,
        actions::forward_action,
        policies::create_policy,
        policies::list_policies,
        policies::update_policy,
        policies::delete_policy,
        health::health,
        health::readiness,
        health::liveness,
    ),
    components(schemas(
        crate::models::ActionStatus,
        crate::models::RiskLevel,
        crate::models::VerdictDecision,
        crate::models::Agent,
        crate::models::CreateAgentRequest,
        crate::models::CreateAgentResponse,
        crate::models::Policy,
        crate::models::CreatePolicyRequest,
        crate::models::UpdatePolicyRequest,
        actions::CreateActionRequest,
        actions::CreateActionResponse,
        actions::ActionDetailResponse,
        actions::ListActionsResponse,
        crate::models::VerdictResponse,
        auth::RegisterAgentRequest,
        auth::ListAgentsResponse,
        auth::AgentSummary,
        auth::RotateApiKeyRequest,
        auth::RotateApiKeyResponse,
        policies::CreatePolicyRequest,
        policies::UpdatePolicyRequest,
        health::HealthResponse,
    )),
    security(("ApiKey" = []))
)]
pub struct ApiDoc;

pub fn create_router(config: &Config, pool: DbPool, pool_set: Arc<DbPoolSet>) -> anyhow::Result<Router> {
    info!("Setting up router");

    let cors = tower_http::cors::CorsLayer::very_permissive();

    let body_limit = tower_http::limit::RequestBodyLimitLayer::new(
        config.max_request_body_kb * 1024,
    );

    let metrics = MetricsCollector::new();
    let rate_limiter = RateLimiter::new(config.rate_limit_per_minute, 60);

    let auth_state = crate::middleware::auth::AgentState {
        pool: pool.clone(),
        api_key_secret: config.api_key_secret.clone(),
    };

    let protected_routes = Router::new()
        .route("/action", axum::routing::post(actions::create_action))
        .route("/action/{id}", axum::routing::get(actions::get_action))
        .route("/action/{id}/status", axum::routing::get(actions::get_action_status))
        .route("/action/{id}/forward", axum::routing::post(actions::forward_action))
        .route("/actions", axum::routing::get(actions::list_actions))
        .route("/actions/batch", axum::routing::post(actions::batch_create_actions))
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            crate::middleware::auth::auth_middleware,
        ))
        .layer(axum::middleware::from_fn(
            crate::services::rate_limit_middleware,
        ))
        .layer(axum::Extension(rate_limiter.clone()))
        .layer(axum::Extension(pool_set));

    let admin_routes = Router::new()
        .route("/admin", axum::routing::get(admin::dashboard))
        .route("/admin/actions", axum::routing::get(admin::list_admin_actions))
        .route("/admin/actions/{id}", axum::routing::get(admin::get_admin_action_detail))
        .route("/admin/actions/{id}/override", axum::routing::post(admin::override_action))
        .route("/admin/agents", axum::routing::get(admin::agent_management))
        .route("/admin/agents/register", axum::routing::post(auth::register_agent))
        .route("/admin/agents/revoke", axum::routing::post(auth::revoke_agent))
        .route("/admin/agents/rotate-key", axum::routing::post(auth::rotate_agent_key))
        .route("/admin/policies", axum::routing::get(admin::policy_management).post(policies::create_policy))
        .route("/admin/policies/{id}", axum::routing::put(policies::update_policy))
        .route("/admin/policies/{id}", axum::routing::delete(policies::delete_policy))
        .route("/admin/verdicts", axum::routing::get(admin::verdict_history))
        .route("/admin/audit", axum::routing::get(admin::audit_log_viewer))
        .layer(axum::middleware::from_fn(admin_auth_middleware));

    let app = Router::new()
        .route("/health", axum::routing::get(health::health))
        .route("/health/ready", axum::routing::get(health::readiness))
        .route("/health/live", axum::routing::get(health::liveness))
        .route("/metrics", axum::routing::get(metrics_endpoint))
        .route("/admin/login", axum::routing::get(admin::admin_login_page).post(admin::admin_login))
        .route("/admin/logout", axum::routing::post(admin::admin_logout))
        .merge(admin_routes)
        .merge(protected_routes)
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .nest_service("/static", tower_http::services::ServeDir::new("static"))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(body_limit)
        .layer(axum::middleware::from_fn(crate::services::request_metrics_middleware))
        .layer(axum::Extension(metrics))
        .with_state(pool);

    Ok(app)
}

pub async fn metrics_endpoint(
    axum::Extension(metrics): axum::Extension<MetricsCollector>,
) -> axum::Json<serde_json::Value> {
    axum::Json(metrics.to_json())
}

async fn admin_auth_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let admin_password = std::env::var("DEKO_ADMIN_PASSWORD").unwrap_or_default();
    let auth_header = request
        .headers()
        .get("X-Admin-Password")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let cookie_password = request
        .headers()
        .get("Cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|c| {
            c.split(';')
                .find(|part| part.trim().starts_with("deko_admin="))
                .and_then(|part| part.trim().split_once('='))
                .map(|(_, value)| value.to_string())
        })
        .unwrap_or_default();

    let is_admin = if admin_password.is_empty() {
        false
    } else {
        let valid_passwords: Vec<&str> = admin_password.split(',').map(|s| s.trim()).collect();
        valid_passwords.iter().any(|p| *p == auth_header || *p == cookie_password)
    };

    if !is_admin {
        return (StatusCode::FORBIDDEN, axum::Json(serde_json::json!({"error": "Admin access required"}))).into_response();
    }

    let mut request = request;
    request.extensions_mut().insert(is_admin);

    next.run(request).await
}
