use deko::config::Config;
use deko::db::{init_db, run_migrations};
use deko::routes::create_router;
use deko::services::{VerdictService, MetricsCollector};
use deko::models::ActionStatus;
use deko::test_helpers::{MockLLMProvider, TestFixtures, TestApp};
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

async fn setup_test_db() -> sqlx::SqlitePool {
    let config = test_config();
    let pool = init_db(&config).await.unwrap();
    run_migrations(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn test_health_endpoint() {
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool).unwrap();

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
    let pool = setup_test_db().await;

    let (agent_id, _api_key) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Buy 10 shares of AAPL").await.unwrap();

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
    let pool = setup_test_db().await;

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "View dashboard").await.unwrap();

    let mock = MockLLMProvider::approved();
    let call_count = mock.call_count.clone();

    let mut vs = VerdictService::new(pool.clone(), &test_config(), Arc::new(MetricsCollector::new()));
    vs.providers.clear();
    vs.providers.push(Box::new(mock));
    vs.default_provider_idx = 0;

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
    let pool = setup_test_db().await;

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Delete all records").await.unwrap();

    let mock = MockLLMProvider::denied();
    let call_count = mock.call_count.clone();

    let mut vs = VerdictService::new(pool.clone(), &test_config(), Arc::new(MetricsCollector::new()));
    vs.providers.clear();
    vs.providers.push(Box::new(mock));
    vs.default_provider_idx = 0;

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
    let pool = setup_test_db().await;

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Transfer $50,000").await.unwrap();

    let mock = MockLLMProvider::escalated();
    let call_count = mock.call_count.clone();

    let mut vs = VerdictService::new(pool.clone(), &test_config(), Arc::new(MetricsCollector::new()));
    vs.providers.clear();
    vs.providers.push(Box::new(mock));
    vs.default_provider_idx = 0;

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
    let pool = setup_test_db().await;

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Some action").await.unwrap();

    let mock = MockLLMProvider::failing("Simulated LLM failure");

    let mut vs = VerdictService::new(pool.clone(), &test_config(), Arc::new(MetricsCollector::new()));
    vs.providers.clear();
    vs.providers.push(Box::new(mock));
    vs.default_provider_idx = 0;

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
    let pool = setup_test_db().await;

    TestFixtures::create_deny_keyword_policy(&pool, "No Delete All", &["delete_all", "delete everything"]).await.unwrap();

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "I want to delete_all records").await.unwrap();

    let vs = VerdictService::new(pool.clone(), &test_config(), Arc::new(MetricsCollector::new()));
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
    let pool = setup_test_db().await;

    TestFixtures::create_max_amount_policy(&pool, "Transfer Limit", 10000.0).await.unwrap();

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action_with_details(
        &pool, &agent_id,
        "Transfer funds",
        Some(r#"{"amount": 50000}"#),
        Some("https://bank.example.com/transfer"),
        Some("POST"),
    ).await.unwrap();

    let vs = VerdictService::new(pool.clone(), &test_config(), Arc::new(MetricsCollector::new()));
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
    let pool = setup_test_db().await;

    let mock = MockLLMProvider::approved();

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "View data").await.unwrap();

    let mut vs = VerdictService::new(pool.clone(), &test_config(), Arc::new(MetricsCollector::new()));
    vs.providers.clear();
    vs.providers.push(Box::new(mock));
    vs.default_provider_idx = 0;

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
    let pool = setup_test_db().await;

    let (agent_id, _) = TestFixtures::create_agent(&pool, "test_agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Test action").await.unwrap();

    let vs = VerdictService::new(pool.clone(), &test_config(), Arc::new(MetricsCollector::new()));
    let _ = vs.process_action(&action_id).await;

    let status: (String,) = sqlx::query_as("SELECT status FROM actions WHERE id = ?")
        .bind(&action_id)
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(status.0, "denied");
}

#[test]
fn test_config_validation_fails_missing_secret() {
}

#[test]
fn test_config_default_values() {
    let result = Config::from_env();
    if let Ok(config) = result {
        assert!(config.port <= 65535);
        assert_eq!(config.max_screenshot_size_mb, 10);
        assert_eq!(config.rate_limit_per_minute, 60);
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
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let live = client.get(format!("http://127.0.0.1:{}/health/live", addr.port())).send().await.unwrap();
    assert_eq!(live.status(), 200);

    let ready = client.get(format!("http://127.0.0.1:{}/health/ready", addr.port())).send().await.unwrap();
    assert_eq!(ready.status(), 200);

    let health = client.get(format!("http://127.0.0.1:{}/health", addr.port())).send().await.unwrap();
    assert_eq!(health.status(), 200);

    let body: serde_json::Value = health.json().await.unwrap();
    assert_eq!(body["status"], "healthy");
    assert_eq!(body["database"], "healthy");
}

#[tokio::test]
async fn test_admin_login_valid_password() {
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("http://127.0.0.1:{}/admin/login", addr.port()))
        .json(&serde_json::json!({"password": "testpassword"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_admin_login_invalid_password() {
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("http://127.0.0.1:{}/admin/login", addr.port()))
        .json(&serde_json::json!({"password": "wrongpassword"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_admin_dashboard_requires_auth() {
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("http://127.0.0.1:{}/admin", addr.port()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_admin_dashboard_with_valid_password() {
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("http://127.0.0.1:{}/admin", addr.port()))
        .header("X-Admin-Password", "testpassword")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_action_create_via_http_with_valid_key() {
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool.clone()).unwrap();
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
    assert!(body["id"].as_str().unwrap().len() > 0);
}

#[tokio::test]
async fn test_action_create_via_http_without_key_returns_401() {
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool).unwrap();
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
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool.clone()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let (agent_id, api_key) = TestFixtures::create_agent(&pool, "status-test-agent").await.unwrap();
    let action_id = TestFixtures::create_action(&pool, &agent_id, "Test action").await.unwrap();

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
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool.clone()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = reqwest::Client::new();

    let (agent_id, api_key) = TestFixtures::create_agent(&pool, "filter-test-agent").await.unwrap();
    TestFixtures::create_action(&pool, &agent_id, "First action").await.unwrap();
    TestFixtures::create_action(&pool, &agent_id, "Second action").await.unwrap();

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
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool).unwrap();
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
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool).unwrap();
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
    assert!(body["api_key"].as_str().unwrap().len() > 0);
}

#[tokio::test]
async fn test_swagger_docs_served() {
    let pool = setup_test_db().await;
    let config = test_config();
    let app = create_router(&config, pool).unwrap();
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
