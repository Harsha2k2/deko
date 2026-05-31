mod actions;
mod admin;
mod admin_ws;
mod api_admin;
mod attachments;
mod auth;
mod health;
mod oauth;
mod policies;
mod token;
mod ws;

use axum::Router;
use std::sync::Arc;
use crate::db::{DbPool, DbPoolSet};
use tower_http::trace::TraceLayer;
use tower_http::services::ServeDir;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::config::Config;
use crate::services::metrics::{MetricsCollector, RateLimiter};
use crate::services::ws_broadcaster::WsBroadcaster;

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

pub fn create_router(config: &Config, pool: DbPool, pool_set: Arc<DbPoolSet>, ws_broadcaster: Arc<WsBroadcaster>) -> anyhow::Result<Router> {
    info!("Setting up router");

    let cors = tower_http::cors::CorsLayer::very_permissive();

    let body_limit = tower_http::limit::RequestBodyLimitLayer::new(
        config.max_request_body_kb * 1024,
    );

    let metrics = MetricsCollector::new();
    let rate_limiter = RateLimiter::new(config.rate_limit_per_minute, 60);

    let api_key_secret = config.api_key_secret.clone();

    let auth_state = crate::middleware::auth::AgentState {
        pool: pool.clone(),
        api_key_secret: api_key_secret.clone(),
    };

    let jwt_state = crate::middleware::jwt::JwtState {
        jwt_secret: config.jwt_secret.clone(),
    };

    // Agent API routes — protected by both API key and JWT
    let protected_routes = Router::new()
        .route("/action", axum::routing::post(actions::create_action))
        .route("/action/{id}", axum::routing::get(actions::get_action))
        .route("/action/{id}/status", axum::routing::get(actions::get_action_status))
        .route("/action/{id}/forward", axum::routing::post(actions::forward_action))
        .route("/actions", axum::routing::get(actions::list_actions))
        .route("/actions/batch", axum::routing::post(actions::batch_create_actions))
        .route("/action/{id}/attachments", axum::routing::post(attachments::upload_attachment).get(attachments::list_attachments))
        .route("/action/{id}/attachments/{attachment_id}", axum::routing::get(attachments::download_attachment))
        .route("/action/{id}/ws", axum::routing::get(ws::action_ws_handler))
        .route_layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            crate::middleware::auth::auth_middleware,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
            jwt_state.clone(),
            crate::middleware::jwt::jwt_middleware,
        ))
        .layer(axum::middleware::from_fn(
            crate::services::rate_limit_middleware,
        ))
        .layer(axum::Extension(rate_limiter.clone()))
        .layer(axum::Extension(pool_set));

    // Login/logout routes — no auth middleware (login GET falls through to SPA)
    let auth_routes = Router::new()
        .route("/admin/login", axum::routing::post(admin::admin_login))
        .route("/admin/logout", axum::routing::get(admin::admin_logout).post(admin::admin_logout));

    // Admin API routes (POST endpoints that need admin password)
    let admin_api_routes = Router::new()
        .route("/admin/actions/export", axum::routing::get(admin::export_actions_csv))
        .route("/admin/actions/{id}/override", axum::routing::post(admin::override_action))
        .route("/admin/actions/bulk-override", axum::routing::post(admin::bulk_override_actions))
        .route("/admin/agents/register", axum::routing::post(auth::register_agent))
        .route("/admin/agents/revoke", axum::routing::post(auth::revoke_agent))
        .route("/admin/agents/rotate-key", axum::routing::post(auth::rotate_agent_key))
        .route("/admin/agents/create-api-key", axum::routing::post(auth::create_api_key))
        .route("/admin/agents/list-api-keys", axum::routing::post(auth::list_api_keys))
        .route("/admin/policies", axum::routing::post(policies::create_policy))
        .route("/admin/policies/test", axum::routing::post(policies::test_policy))
        .route("/admin/policies/simulate", axum::routing::post(policies::simulate_policies))
        .route("/admin/policies/{id}", axum::routing::put(policies::update_policy))
        .route("/admin/policies/{id}", axum::routing::delete(policies::delete_policy))
        .route("/admin/audit/export", axum::routing::get(admin::export_audit_log))
        .route("/admin/audit/search", axum::routing::get(admin::search_audit_log))
        .layer(axum::middleware::from_fn(admin_auth_middleware));

    // JSON API routes for the SPA (require admin auth)
    let json_api_routes = Router::new()
        .route("/api/admin/dashboard", axum::routing::get(api_admin::dashboard))
        .route("/api/admin/actions/timeline", axum::routing::get(api_admin::action_timeline))
        .route("/api/admin/actions", axum::routing::get(api_admin::list_actions))
        .route("/api/admin/actions/{id}", axum::routing::get(api_admin::get_action))
        .route("/api/admin/actions/{id}/override", axum::routing::post(api_admin::override_action))
        .route("/api/admin/agents", axum::routing::get(api_admin::list_agents))
        .route("/api/admin/verdicts", axum::routing::get(api_admin::list_verdicts))
        .route("/api/admin/policies", axum::routing::get(api_admin::list_policies))
        .route("/api/admin/audit", axum::routing::get(api_admin::list_audit_log))
        .route("/api/admin/ws", axum::routing::get(admin_ws::admin_ws_handler))
        .layer(axum::middleware::from_fn(admin_auth_middleware));

    // OAuth routes
    let oauth_config = crate::services::oauth::OAuthConfig::from_env();
    let oauth_state = oauth::OAuthState::new(oauth_config);
    let oauth_routes = Router::new()
        .route("/admin/auth/login", axum::routing::get(oauth::oauth_login))
        .route("/admin/auth/callback", axum::routing::get(oauth::oauth_callback))
        .route("/admin/auth/status", axum::routing::get(oauth::oauth_status))
        .with_state(oauth_state);

    // SPA — serve static files, fallback to index.html for client-side routing
    let spa = ServeDir::new("admin/dist")
        .not_found_service(
            tower_http::services::fs::ServeFile::new("admin/dist/index.html")
        );

    let app = Router::new()
        .route("/health", axum::routing::get(health::health))
        .route("/health/ready", axum::routing::get(health::readiness))
        .route("/health/live", axum::routing::get(health::liveness))
        .route("/metrics", axum::routing::get(metrics_endpoint))
        .route("/metrics/prometheus", axum::routing::get(metrics_prometheus_endpoint))
        .route("/auth/token", axum::routing::post(token::exchange_token))
        .merge(auth_routes)
        .merge(oauth_routes)
        .merge(admin_api_routes)
        .merge(json_api_routes)
        .merge(protected_routes)
        .nest_service("/admin", spa)
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .nest_service("/static", tower_http::services::ServeDir::new("static"))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .layer(body_limit)
        .layer(axum::middleware::from_fn(crate::services::request_metrics_middleware))
        .layer(axum::Extension(metrics))
        .layer(axum::Extension(ws_broadcaster))
        .layer(axum::Extension(config.clone()))
        .with_state(pool);

    Ok(app)
}

pub async fn metrics_endpoint(
    axum::Extension(metrics): axum::Extension<MetricsCollector>,
) -> axum::Json<serde_json::Value> {
    axum::Json(metrics.to_json())
}

pub async fn metrics_prometheus_endpoint(
    axum::Extension(metrics): axum::Extension<MetricsCollector>,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let json = metrics.to_json();
    let mut output = String::new();
    output.push_str("# HELP deko_actions_total Total actions processed\n");
    output.push_str("# TYPE deko_actions_total counter\n");
    output.push_str(&format!("deko_actions_total {}\n", json["actions"]["total"]));
    output.push_str(&format!("deko_actions_approved {}\n", json["actions"]["approved"]));
    output.push_str(&format!("deko_actions_denied {}\n", json["actions"]["denied"]));
    output.push_str(&format!("deko_actions_escalated {}\n", json["actions"]["escalated"]));
    output.push_str("# HELP deko_llm_calls_total Total LLM calls\n");
    output.push_str("# TYPE deko_llm_calls_total counter\n");
    output.push_str(&format!("deko_llm_calls_total {}\n", json["llm"]["calls_total"]));
    output.push_str("# HELP deko_errors_total Errors by type\n");
    output.push_str("# TYPE deko_errors_total counter\n");
    if let Some(errors) = json.get("errors") {
        output.push_str(&format!("deko_errors_database {}\n", errors["database"]));
        output.push_str(&format!("deko_errors_llm {}\n", errors["llm"]));
        output.push_str(&format!("deko_errors_auth {}\n", errors["auth"]));
    }
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(axum::http::header::CONTENT_TYPE, axum::http::HeaderValue::from_static("text/plain; version=0.0.4"));
    (StatusCode::OK, headers, output)
}

async fn admin_auth_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let admin_password = std::env::var("DEKO_ADMIN_PASSWORD").unwrap_or_default();
    let oauth_enabled = std::env::var("DEKO_OAUTH_ENABLED").ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);

    let auth_header = request
        .headers()
        .get("X-Admin-Password")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let cookie_value = request
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

    // IP allowlist check
    if let Some(ip) = request.headers().get("X-Forwarded-For").and_then(|v| v.to_str().ok()) {
        let allowed = std::env::var("DEKO_ADMIN_IP_ALLOWLIST").unwrap_or_default();
        if !allowed.is_empty() && !allowed.split(',').any(|a| ip.trim().starts_with(a.trim())) {
            return (StatusCode::FORBIDDEN, axum::Json(serde_json::json!({"error": "Admin access restricted by IP"}))).into_response();
        }
    }

    let is_admin = if !admin_password.is_empty() {
        let valid_passwords: Vec<&str> = admin_password.split(',').map(|s| s.trim()).collect();
        if valid_passwords.iter().any(|p| *p == auth_header || *p == cookie_value) {
            true
        } else if oauth_enabled && !cookie_value.is_empty() && cookie_value.contains(':') {
            // OAuth cookie format: "email:provider"
            let parts: Vec<&str> = cookie_value.splitn(2, ':').collect();
            parts.len() == 2 && !parts[0].is_empty()
        } else {
            false
        }
    } else if oauth_enabled && !cookie_value.is_empty() && cookie_value.contains(':') {
        // No password set, but OAuth is enabled
        let parts: Vec<&str> = cookie_value.splitn(2, ':').collect();
        parts.len() == 2 && !parts[0].is_empty()
    } else {
        false
    };

    if !is_admin {
        return (StatusCode::FORBIDDEN, axum::Json(serde_json::json!({"error": "Admin access required"}))).into_response();
    }

    let mut request = request;
    request.extensions_mut().insert(is_admin);

    next.run(request).await
}
