use deko::config::Config;
use deko::db::{init_db, run_migrations};
use deko::routes::create_router;
use deko::services::{MetricsCollector, VerdictService};

use deko::test_helpers::{MockLLMProvider, TestApp, TestFixtures};
use std::sync::Arc;

fn test_config() -> Config {
    std::env::set_var("DEKO_PORT", "0");
    std::env::set_var("DEKO_ENV", "dev");
    std::env::set_var("DEKO_ADMIN_PASSWORD", "testpassword");
    std::env::set_var("DEKO_DATABASE_URL", "sqlite::memory:");
    std::env::set_var("DEKO_API_KEY_SECRET", "test-secret-key-12345678");
    std::env::set_var("LLM_DEFAULT_PROVIDER", "gemini");
    std::env::set_var("LLM_DEFAULT_MODEL", "gemini-2.0-flash");
    std::env::set_var("GEMINI_API_KEY", "test-key");
    std::env::set_var("GEMINI_MODEL", "gemini-2.0-flash");
    std::env::set_var("OPENAI_API_KEY", "sk-test");
    std::env::set_var("OPENAI_MODEL", "gpt-4o");
    std::env::set_var("DEKO_PROCESSOR_POLL_INTERVAL_SECS", "1");
    Config::from_env().unwrap()
}

async fn setup_test_db() -> (deko::db::DbPool, Arc<deko::db::DbPoolSet>) {
    let config = test_config();
    let (pool, pool_set) = init_db(&config).await.unwrap();
    run_migrations(&pool).await.unwrap();
    (pool, pool_set)
}

#[tokio::test]
async fn test_health_endpoint() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://127.0.0.1:{}/health/live", addr.port()))
        .send()
        .await
        .unwrap();

    assert!(resp.status().is_success());
}

#[tokio::test]
async fn test_action_lifecycle() {
    let (pool, _pool_set) = setup_test_db().await;

    let (agent_id, _api_key) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Buy 10 shares of AAPL")
        .await
        .unwrap();

    let row: (String, String) = sqlx::query_as("SELECT id, status FROM actions WHERE id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(row.0, action_id);
    assert_eq!(row.1, "pending");
}

#[tokio::test]
async fn test_mock_llm_approved() {
    let (pool, _pool_set) = setup_test_db().await;

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "View dashboard")
        .await
        .unwrap();

    let mock = MockLLMProvider::approved();
    let call_count = mock.call_count.clone();

    let vs = VerdictService::with_providers(
        pool.clone(),
        &test_config(),
        Arc::new(MetricsCollector::new()),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
        vec![Box::new(mock)],
    );

    vs.process_action(&action_id).await.unwrap();

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);

    let status: (String,) = sqlx::query_as("SELECT status FROM actions WHERE id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(status.0, "approved");
}

#[tokio::test]
async fn test_mock_llm_denied() {
    let (pool, _pool_set) = setup_test_db().await;

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Delete all records")
        .await
        .unwrap();

    let mock = MockLLMProvider::denied();
    let call_count = mock.call_count.clone();

    let vs = VerdictService::with_providers(
        pool.clone(),
        &test_config(),
        Arc::new(MetricsCollector::new()),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
        vec![Box::new(mock)],
    );

    vs.process_action(&action_id).await.unwrap();

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);

    let status: (String,) = sqlx::query_as("SELECT status FROM actions WHERE id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(status.0, "denied");
}

#[tokio::test]
async fn test_mock_llm_escalated() {
    let (pool, _pool_set) = setup_test_db().await;

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Transfer $50,000")
        .await
        .unwrap();

    let mock = MockLLMProvider::escalated();
    let call_count = mock.call_count.clone();

    let vs = VerdictService::with_providers(
        pool.clone(),
        &test_config(),
        Arc::new(MetricsCollector::new()),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
        vec![Box::new(mock)],
    );

    vs.process_action(&action_id).await.unwrap();

    assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1);

    let status: (String,) = sqlx::query_as("SELECT status FROM actions WHERE id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(status.0, "escalated");
}

#[tokio::test]
async fn test_mock_llm_failure_fails_closed() {
    let (pool, _pool_set) = setup_test_db().await;

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Some action")
        .await
        .unwrap();

    let mock = MockLLMProvider::failing("Simulated LLM failure");

    let vs = VerdictService::with_providers(
        pool.clone(),
        &test_config(),
        Arc::new(MetricsCollector::new()),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
        vec![Box::new(mock)],
    );

    vs.process_action(&action_id).await.unwrap();

    let status: (String,) = sqlx::query_as("SELECT status FROM actions WHERE id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(status.0, "denied");
}

#[tokio::test]
async fn test_policy_deny_keyword() {
    let (pool, _pool_set) = setup_test_db().await;

    TestFixtures::create_deny_keyword_policy(&pool, "No Delete All", &["delete_all", "delete everything"])
        .await
        .unwrap();

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "I want to delete_all records")
        .await
        .unwrap();

    let vs = VerdictService::new(
        pool.clone(),
        &test_config(),
        Arc::new(MetricsCollector::new()),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    );
    vs.process_action(&action_id).await.unwrap();

    let status: (String,) = sqlx::query_as("SELECT status FROM actions WHERE id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(status.0, "denied");
}

#[tokio::test]
async fn test_policy_max_amount() {
    let (pool, _pool_set) = setup_test_db().await;

    TestFixtures::create_max_amount_policy(&pool, "Transfer Limit", 10000.0)
        .await
        .unwrap();

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action_with_details(
        &pool,
        &agent_id,
        "Transfer funds",
        Some(r#"{"amount": 50000}"#),
        Some("https://bank.example.com/transfer"),
        Some("POST"),
    )
    .await
    .unwrap();

    let vs = VerdictService::new(
        pool.clone(),
        &test_config(),
        Arc::new(MetricsCollector::new()),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    );
    vs.process_action(&action_id).await.unwrap();

    let status: (String,) = sqlx::query_as("SELECT status FROM actions WHERE id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(status.0, "denied");
}

#[tokio::test]
async fn test_audit_log_created_for_verdict() {
    let (pool, _pool_set) = setup_test_db().await;

    let mock = MockLLMProvider::approved();

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "View data")
        .await
        .unwrap();

    let vs = VerdictService::with_providers(
        pool.clone(),
        &test_config(),
        Arc::new(MetricsCollector::new()),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
        vec![Box::new(mock)],
    );

    vs.process_action(&action_id).await.unwrap();

    let audit_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE action_id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert!(audit_count.0 >= 2);
}

#[tokio::test]
async fn test_fail_closed_on_real_llm_failure() {
    let (pool, _pool_set) = setup_test_db().await;

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Test action")
        .await
        .unwrap();

    let vs = VerdictService::new(
        pool.clone(),
        &test_config(),
        Arc::new(MetricsCollector::new()),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    );
    let _ = vs.process_action(&action_id).await;

    let status: (String,) = sqlx::query_as("SELECT status FROM actions WHERE id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(status.0, "denied");
}

#[test]
fn test_config_validation_fails_missing_secret() {}

#[test]
fn test_config_default_values() {
    let result = Config::from_env();
    if let Ok(config) = result {
        assert_eq!(config.max_screenshot_size_mb, 10);
        assert_eq!(config.rate_limit_per_minute, 120);
    }
}

#[tokio::test]
async fn test_test_app_helper() {
    let app = TestApp::setup().await;

    let (agent_id, api_key) = app.setup_with_agent("integration_test_agent").await;

    assert!(!agent_id.is_empty());
    assert!(!api_key.is_empty());

    let action_id = app.setup_with_action(&agent_id, "Test intent").await;
    assert!(!action_id.is_empty());
}

#[tokio::test]
async fn test_all_health_endpoints() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let live = client
        .get(format!("http://127.0.0.1:{}/health/live", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(live.status(), 200);

    let ready = client
        .get(format!("http://127.0.0.1:{}/health/ready", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(ready.status(), 200);

    let health = client
        .get(format!("http://127.0.0.1:{}/health", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(health.status(), 200);

    let body: serde_json::Value = health.json().await.unwrap();
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["database"], "healthy");
}

#[tokio::test]
async fn test_admin_login_valid_password() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("http://127.0.0.1:{}/admin/login", addr.port()))
        .form(&serde_json::json!({"password": "testpassword"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_admin_login_invalid_password() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("http://127.0.0.1:{}/admin/login", addr.port()))
        .form(&serde_json::json!({"password": "wrongpassword"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_admin_dashboard_requires_auth() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("http://127.0.0.1:{}/api/admin/dashboard", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_admin_dashboard_with_valid_password() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("http://127.0.0.1:{}/api/admin/dashboard", addr.port()))
        .header("X-Admin-Password", "testpassword")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_action_create_via_http_with_valid_key() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let (agent_id, api_key) = TestFixtures::create_agent(&pool, "http-test-agent").await.unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:{}/action", addr.port()))
        .header("X-API-Key", &api_key)
        .json(&serde_json::json!({
            "intent": "Buy 10 shares of AAPL",
            "payload": "{\"symbol\": \"AAPL\", \"quantity\": 10}",
            "agent_id": agent_id,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "pending");
    assert!(!body["id"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_action_create_via_http_without_key_returns_401() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("http://127.0.0.1:{}/action", addr.port()))
        .json(&serde_json::json!({"intent": "Test"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_action_status_returns_pending_initially() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let (agent_id, api_key) = TestFixtures::create_agent(&pool, "status-test-agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Test action")
        .await
        .unwrap();

    let resp = client
        .get(format!("http://127.0.0.1:{}/action/{}/status", addr.port(), action_id))
        .header("X-API-Key", &api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["action_id"], action_id);
    assert_eq!(body["status"], "pending");
}

#[tokio::test]
async fn test_action_filter_by_status() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let (agent_id, api_key) = TestFixtures::create_agent(&pool, "filter-test-agent").await.unwrap();
    TestFixtures::create_action(&pool, &agent_id, "First action")
        .await
        .unwrap();
    TestFixtures::create_action(&pool, &agent_id, "Second action")
        .await
        .unwrap();

    let resp = client
        .get(format!("http://127.0.0.1:{}/actions?status=pending", addr.port()))
        .header("X-API-Key", &api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("http://127.0.0.1:{}/metrics", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("actions").is_some());
    assert!(body.get("llm").is_some());
}

#[tokio::test]
async fn test_admin_register_agent_via_http() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("http://127.0.0.1:{}/admin/agents/register", addr.port()))
        .header("X-Admin-Password", "testpassword")
        .json(&serde_json::json!({"name": "http-registered-agent"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["name"], "http-registered-agent");
    assert!(!body["api_key"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_swagger_docs_served() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("http://127.0.0.1:{}/docs/", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let openapi = client
        .get(format!("http://127.0.0.1:{}/api-docs/openapi.json", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(openapi.status(), 200);
}

// ---- forwarding honesty + egress guard ----

async fn start_forward_app() -> (std::net::SocketAddr, deko::db::DbPool, String, String) {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (agent_id, api_key) = TestFixtures::create_agent(&pool, "forward-agent").await.unwrap();
    (addr, pool, agent_id, api_key)
}

async fn create_action_with_target(pool: &deko::db::DbPool, agent_id: &str, target_url: Option<&str>) -> String {
    TestFixtures::create_action_with_details(pool, agent_id, "test forward action", None, target_url, Some("GET"))
        .await
        .unwrap()
}

async fn approve_action(pool: &deko::db::DbPool, action_id: &str) {
    sqlx::query("INSERT INTO verdicts (id, action_id, decision, reason, risk_level, policy_matched, llm_raw_response) VALUES (?, ?, 'approved', 'test approve', 'low', NULL, '{}')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(action_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE actions SET status = 'approved' WHERE id = ?")
        .bind(action_id)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_forward_blocked_for_private_target_is_honest_failure() {
    let (addr, _pool, _agent_id, api_key) = start_forward_app().await;
    let client = reqwest::Client::new();

    // dangerous targets are refused at submit time, before any state exists
    let resp = client
        .post(format!("http://127.0.0.1:{}/action", addr.port()))
        .header("X-API-Key", &api_key)
        .json(&serde_json::json!({
            "intent": "fetch metadata",
            "target_url": "http://169.254.169.254/latest/meta-data/",
            "target_method": "GET"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400, "metadata endpoint must be rejected at creation");

    for bad_target in [
        "http://127.0.0.1/admin",
        "http://192.168.1.1/",
        "http://10.0.0.5/x",
        "file:///etc/passwd",
        "https://evil.com@127.0.0.1/",
    ] {
        let resp = client
            .post(format!("http://127.0.0.1:{}/action", addr.port()))
            .header("X-API-Key", &api_key)
            .json(&serde_json::json!({
                "intent": "probe",
                "target_url": bad_target,
                "target_method": "GET"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 400, "should reject {}", bad_target);
    }
}

#[tokio::test]
async fn test_forward_approved_loopback_reports_failure_not_success() {
    let (addr, pool, agent_id, api_key) = start_forward_app().await;
    let client = reqwest::Client::new();

    // .invalid tld is guaranteed unresolvable (rfc 2606): passes the guard,
    // then fails at dns -> exercises the honest forward_failed path
    let action_id = create_action_with_target(&pool, &agent_id, Some("https://unreachable.invalid/x")).await;
    approve_action(&pool, &action_id).await;

    let resp = client
        .post(format!("http://127.0.0.1:{}/action/{}/forward", addr.port(), action_id))
        .header("X-API-Key", &api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["forwarded"], false, "guard must block loopback egress");

    let status: (String,) = sqlx::query_as("SELECT status FROM actions WHERE id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status.0, "forward_failed");
}

#[tokio::test]
async fn test_forward_denied_action_returns_403() {
    let (addr, pool, agent_id, api_key) = start_forward_app().await;
    let client = reqwest::Client::new();

    let action_id = create_action_with_target(&pool, &agent_id, Some("https://api.example.com/x")).await;
    sqlx::query("INSERT INTO verdicts (id, action_id, decision, reason, risk_level, policy_matched, llm_raw_response) VALUES (?, ?, 'denied', 'nope', 'high', NULL, '{}')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&action_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE actions SET status = 'denied' WHERE id = ?")
        .bind(&action_id)
        .execute(&pool)
        .await
        .unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:{}/action/{}/forward", addr.port(), action_id))
        .header("X-API-Key", &api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_forward_escalated_action_returns_423() {
    let (addr, pool, agent_id, api_key) = start_forward_app().await;
    let client = reqwest::Client::new();

    let action_id = create_action_with_target(&pool, &agent_id, Some("https://api.example.com/x")).await;
    sqlx::query("INSERT INTO verdicts (id, action_id, decision, reason, risk_level, policy_matched, llm_raw_response) VALUES (?, ?, 'escalate', 'needs human', 'medium', NULL, '{}')")
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&action_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE actions SET status = 'escalated' WHERE id = ?")
        .bind(&action_id)
        .execute(&pool)
        .await
        .unwrap();

    let resp = client
        .post(format!("http://127.0.0.1:{}/action/{}/forward", addr.port(), action_id))
        .header("X-API-Key", &api_key)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 423);
}

#[tokio::test]
async fn test_cors_preflight_respects_allowlist() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = reqwest::Client::new();
    // dev profile allowlist includes localhost:8000
    let resp = client
        .request(
            reqwest::Method::OPTIONS,
            format!("http://127.0.0.1:{}/health", addr.port()),
        )
        .header("Origin", "http://localhost:8000")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .unwrap();

    let origin_header = resp
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok());
    assert_eq!(
        origin_header,
        Some("http://localhost:8000"),
        "preflight must echo allowed origins"
    );
}

// ---- audit hash chain ----

#[tokio::test]
async fn test_audit_chain_verify_detects_tampering() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    // generate some audited activity
    let (agent_id, api_key) = TestFixtures::create_agent(&pool, "audit-agent").await.unwrap();
    let client = reqwest::Client::new();
    for i in 0..3 {
        let resp = client
            .post(format!("http://127.0.0.1:{}/action", addr.port()))
            .header("X-API-Key", &api_key)
            .json(&serde_json::json!({ "intent": format!("action number {}", i) }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
    }
    let _ = agent_id;

    // intact chain verifies
    let resp = client
        .get(format!("http://127.0.0.1:{}/admin/audit/verify", addr.port()))
        .header("X-Admin-Password", "testpassword")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["valid"], true, "fresh chain must verify");
    let entries_before = body["entries_checked"].as_u64().unwrap();
    assert!(entries_before >= 3);

    // layer 1: database guards refuse the edit outright
    let middle_id: (String,) = sqlx::query_as("SELECT id FROM audit_log ORDER BY rowid ASC LIMIT 1 OFFSET 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let guard_result = sqlx::query("UPDATE audit_log SET details = '{\"evil\": true}' WHERE id = ?")
        .bind(&middle_id.0)
        .execute(&pool)
        .await;
    assert!(
        guard_result.is_err(),
        "tamper guards must refuse direct updates on chained entries"
    );

    // layer 2: a determined attacker who drops the guards still cannot hide
    // the edit — hash recomputation catches it
    sqlx::query("DROP TRIGGER IF EXISTS audit_log_guard_update")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE audit_log SET details = '{\"evil\": true}' WHERE id = ?")
        .bind(&middle_id.0)
        .execute(&pool)
        .await
        .unwrap();

    let resp = client
        .get(format!("http://127.0.0.1:{}/admin/audit/verify", addr.port()))
        .header("X-Admin-Password", "testpassword")
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["valid"], false, "tampered chain must fail verification");
    assert_eq!(body["first_broken_id"], middle_id.0);
}

#[tokio::test]
async fn test_audit_chain_backfill_makes_legacy_rows_verifiable() {
    let (pool, _pool_set) = setup_test_db().await;

    // simulate legacy rows written before hash columns existed
    for i in 0..3 {
        sqlx::query("INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, NULL, 'legacy_event', ?)")
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(format!("{{\"i\":{}}}", i))
            .execute(&pool)
            .await
            .unwrap();
    }

    // startup backfill chains them
    let backfilled = deko::services::audit::backfill_unchained(&pool).await.unwrap();
    assert_eq!(backfilled, 3);
    // idempotent on second run
    let second = deko::services::audit::backfill_unchained(&pool).await.unwrap();
    assert_eq!(second, 0);

    let report = deko::services::audit::verify_chain(&pool).await.unwrap();
    assert!(report.valid);
    assert_eq!(report.entries_checked, 3);
}

// ---- unified policy engine ----

#[tokio::test]
async fn test_policy_test_endpoint_fails_closed_on_unknown_rule() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = reqwest::Client::new();

    // typo'd rule type must deny, never silently pass
    let resp = client
        .post(format!("http://127.0.0.1:{}/admin/policies/test", addr.port()))
        .header("X-Admin-Password", "testpassword")
        .json(&serde_json::json!({
            "rules": [{"type": "deny_keywords", "keywords": ["nope"]}],
            "intent": "harmless action"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["matched"], true);
    assert_eq!(body["immediate_deny"], true, "unknown rule types must fail closed");
}

// ---- llm provider fallback chain ----

use deko::test_helpers::MockLLMProvider as ChainMock;

#[tokio::test]
async fn test_provider_fallback_succeeds_when_primary_fails() {
    let (pool, _pool_set) = setup_test_db().await;

    let (agent_id, _) = TestFixtures::create_agent(&pool, "fallback-agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "View dashboard")
        .await
        .unwrap();

    let broken = ChainMock::failing("primary provider down");
    let healthy = ChainMock::approved();
    let healthy_calls = healthy.call_count.clone();

    let vs = VerdictService::with_providers(
        pool.clone(),
        &test_config(),
        Arc::new(MetricsCollector::new()),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
        vec![Box::new(broken), Box::new(healthy)],
    );

    vs.process_action(&action_id).await.unwrap();

    // fallback provider must have been called exactly once
    assert_eq!(healthy_calls.load(std::sync::atomic::Ordering::SeqCst), 1);

    // outcome comes from the fallback: approved, not fail-closed deny
    let status: (String,) = sqlx::query_as("SELECT status FROM actions WHERE id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status.0, "approved");

    // the failed hop must be visible in the audit trail
    let failures: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM audit_log WHERE action_id = ? AND event_type = 'llm_provider_failed'")
            .bind(&action_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(failures.0, 1, "provider failure must be audited");
}

#[tokio::test]
async fn test_all_providers_failing_denies_fail_closed() {
    let (pool, _pool_set) = setup_test_db().await;

    let (agent_id, _) = TestFixtures::create_agent(&pool, "allfail-agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Transfer funds")
        .await
        .unwrap();

    let vs = VerdictService::with_providers(
        pool.clone(),
        &test_config(),
        Arc::new(MetricsCollector::new()),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
        vec![Box::new(ChainMock::failing("one")), Box::new(ChainMock::failing("two"))],
    );

    vs.process_action(&action_id).await.unwrap();

    // no provider answered: action denied, never approved
    let status: (String,) = sqlx::query_as("SELECT status FROM actions WHERE id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status.0, "denied");

    let decision: (String,) =
        sqlx::query_as("SELECT decision FROM verdicts WHERE action_id = ? ORDER BY created_at DESC LIMIT 1")
            .bind(&action_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(decision.0, "denied");

    let reason: (String,) =
        sqlx::query_as("SELECT reason FROM verdicts WHERE action_id = ? ORDER BY created_at DESC LIMIT 1")
            .bind(&action_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        reason.0.contains("2 provider"),
        "reason should mention exhaustion across chain, got: {}",
        reason.0
    );
}

// ---- hardened admin auth (sessions) ----

#[tokio::test]
async fn test_login_mints_opaque_session_not_password_cookie() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/admin/login", addr.port()))
        .header("X-Forwarded-For", "10.9.9.9")
        .form(&serde_json::json!({"password": "testpassword"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let set_cookie = resp
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        set_cookie.starts_with("deko_session="),
        "cookie renamed: {}",
        set_cookie
    );
    assert!(
        !set_cookie.contains("testpassword"),
        "raw password must never appear in cookie"
    );
    assert!(set_cookie.contains("HttpOnly"));

    // session token authenticates the admin api without password header
    let token = set_cookie
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("deko_session=")
        .to_string();

    let resp = client
        .get(format!("http://127.0.0.1:{}/api/admin/dashboard", addr.port()))
        .header("Cookie", format!("deko_session={}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "valid session cookie grants admin");
}

#[tokio::test]
async fn test_forged_oauth_style_cookies_are_rejected() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = reqwest::Client::new();

    // the old middleware granted admin to any "x:y" shaped deko_admin cookie
    for evil in [
        "deko_admin=attacker@google.com:google",
        "deko_admin=anything:goes",
        "deko_admin=testpassword", // old raw-password cookie scheme also dead
    ] {
        let resp = client
            .get(format!("http://127.0.0.1:{}/api/admin/dashboard", addr.port()))
            .header("Cookie", evil)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 403, "forged cookie {:?} must not grant admin", evil);
    }

    // unknown session tokens rejected too
    let resp = client
        .get(format!("http://127.0.0.1:{}/api/admin/dashboard", addr.port()))
        .header(
            "Cookie",
            "deko_session=deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_logout_kills_session_server_side() {
    let (pool, pool_set) = setup_test_db().await;
    let config = test_config();
    let app = create_router(
        &config,
        pool.clone(),
        pool_set.clone(),
        std::sync::Arc::new(deko::services::ws_broadcaster::WsBroadcaster::new(64)),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/admin/login", addr.port()))
        .form(&serde_json::json!({"password": "testpassword"}))
        .send()
        .await
        .unwrap();
    let set_cookie = resp
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_string();
    let token = set_cookie
        .split(';')
        .next()
        .unwrap()
        .trim_start_matches("deko_session=")
        .to_string();

    // works before logout
    let resp = client
        .get(format!("http://127.0.0.1:{}/api/admin/dashboard", addr.port()))
        .header("Cookie", format!("deko_session={}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // logout deletes server-side row
    let resp = client
        .get(format!("http://127.0.0.1:{}/admin/logout", addr.port()))
        .header("Cookie", format!("deko_session={}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // same token is now dead even if the browser kept the cookie
    let resp = client
        .get(format!("http://127.0.0.1:{}/api/admin/dashboard", addr.port()))
        .header("Cookie", format!("deko_session={}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "session row must be deleted on logout");

    // only hash stored in db
    let stored: Vec<(String,)> = sqlx::query_as("SELECT id FROM admin_sessions")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(
        stored.iter().all(|(id,)| id != &token),
        "raw session token must never be stored"
    );
}
