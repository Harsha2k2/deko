mod actions;
mod admin;
mod auth;
mod health;
mod policies;

use axum::Router;
use sqlx::SqlitePool;
use tower_http::trace::TraceLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::config::Config;

#[derive(OpenApi)]
#[openapi(
    info(title = "Deko API", version = "0.1.0"),
    paths(
        auth::register_agent,
        auth::revoke_agent,
        auth::list_agents,
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
        policies::CreatePolicyRequest,
        policies::UpdatePolicyRequest,
        health::HealthResponse,
    )),
    security(("ApiKey" = []))
)]
pub struct ApiDoc;

pub fn create_router(config: &Config, pool: SqlitePool) -> anyhow::Result<Router> {
    info!("Setting up router");

    let cors = tower_http::cors::CorsLayer::very_permissive();

    let body_limit = tower_http::limit::RequestBodyLimitLayer::new(
        config.max_request_body_kb * 1024,
    );

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
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            crate::middleware::auth::auth_middleware,
        ));

    let admin_routes = Router::new()
        .route("/admin", axum::routing::get(admin::dashboard))
        .route("/admin/login", axum::routing::post(admin::admin_login))
        .route("/admin/actions", axum::routing::get(admin::list_admin_actions))
        .route("/admin/actions/{id}", axum::routing::get(admin::get_admin_action_detail))
        .route("/admin/actions/{id}/override", axum::routing::post(admin::override_action))
        .route("/admin/agents", axum::routing::get(auth::list_agents))
        .route("/admin/agents/register", axum::routing::post(auth::register_agent))
        .route("/admin/agents/revoke", axum::routing::post(auth::revoke_agent))
        .route("/admin/policies", axum::routing::get(policies::list_policies))
        .route("/admin/policies", axum::routing::post(policies::create_policy))
        .route("/admin/policies/{id}", axum::routing::put(policies::update_policy))
        .route("/admin/policies/{id}", axum::routing::delete(policies::delete_policy))
        .layer(axum::middleware::from_fn(admin_auth_middleware));

    let app = Router::new()
        .route("/health", axum::routing::get(health::health))
        .route("/health/ready", axum::routing::get(health::readiness))
        .route("/health/live", axum::routing::get(health::liveness))
        .merge(admin_routes)
        .merge(protected_routes)
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(body_limit)
        .with_state(pool);

    Ok(app)
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

    let is_admin = !admin_password.is_empty() && auth_header == admin_password;

    let mut request = request;
    request.extensions_mut().insert(is_admin);

    next.run(request).await
}
