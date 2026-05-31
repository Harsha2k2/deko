use axum::extract::{Query, State};
use axum::http::header::{SET_COOKIE, HeaderValue};
use axum::response::{IntoResponse, Redirect};
use axum::Json;
use serde::Deserialize;
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{info, warn};

use crate::services::oauth::{OAuthConfig, OAuthProvider};

#[derive(Clone)]
pub struct OAuthState {
    pub provider: Option<OAuthProvider>,
    pub pending_states: Arc<RwLock<std::collections::HashMap<String, String>>>,
}

impl OAuthState {
    pub fn new(config: OAuthConfig) -> Self {
        let provider = OAuthProvider::new(config);
        Self {
            provider,
            pending_states: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
}

#[derive(Deserialize)]
pub struct CallbackParams {
    code: String,
    state: String,
}

pub async fn oauth_login(
    State(state): State<OAuthState>,
) -> impl IntoResponse {
    let provider = match &state.provider {
        Some(p) => p,
        None => {
            return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "OAuth not configured"
            }))).into_response()
        }
    };

    let (auth_url, csrf_token) = provider.auth_url();

    {
        let mut states = state.pending_states.write().unwrap();
        states.insert(csrf_token.secret().clone(), "admin".to_string());
    }

    Redirect::to(auth_url.as_str()).into_response()
}

pub async fn oauth_callback(
    State(state): State<OAuthState>,
    Query(params): Query<CallbackParams>,
) -> impl IntoResponse {
    let provider = match &state.provider {
        Some(p) => p,
        None => {
            return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "OAuth not configured"
            }))).into_response()
        }
    };

    // Verify state
    {
        let states = state.pending_states.read().unwrap();
        if !states.contains_key(&params.state) {
            warn!("Invalid OAuth state parameter");
            return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "Invalid state parameter"
            }))).into_response();
        }
    }

    // Exchange code for token
    let access_token = match provider.exchange_code(&params.code).await {
        Ok(token) => token,
        Err(e) => {
            warn!("OAuth token exchange failed: {}", e);
            return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "Token exchange failed"
            }))).into_response();
        }
    };

    // Get user info
    let user = match provider.get_user_info(&access_token).await {
        Ok(u) => u,
        Err(e) => {
            warn!("OAuth user info failed: {}", e);
            return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "Failed to get user info"
            }))).into_response();
        }
    };

    // Clean up state
    {
        let mut states = state.pending_states.write().unwrap();
        states.remove(&params.state);
    }

    info!("OAuth login successful: {} ({})", user.email, user.provider);

    // Set admin session cookie (same format as password-based login)
    let cookie_value = format!(
        "deko_admin={}:{}; Path=/; HttpOnly; SameSite=Strict; Max-Age=28800",
        user.email, user.provider
    );

    let mut response = Redirect::to("/admin").into_response();
    if let Ok(val) = HeaderValue::from_str(&cookie_value) {
        response.headers_mut().insert(SET_COOKIE, val);
    }

    response
}

pub async fn oauth_status(
    State(state): State<OAuthState>,
) -> Json<serde_json::Value> {
    let configured = state.provider.is_some();
    let provider_name = state.provider.as_ref().map(|p| p.config.provider.clone());

    Json(serde_json::json!({
        "enabled": configured,
        "provider": provider_name,
    }))
}
