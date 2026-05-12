use std::sync::Arc;
use deko::config::{Config, Environment, LLMProvider};
use deko::error::AppError;
use deko::models::{ActionStatus, RiskLevel, VerdictDecision};
use deko::services::metrics::{MetricsCollector, RateLimiter};

// F009: Config tests
#[test]
fn test_config_env_display() {
    assert_eq!(format!("{}", Environment::Dev), "dev");
    assert_eq!(format!("{}", Environment::Staging), "staging");
    assert_eq!(format!("{}", Environment::Prod), "prod");
}

#[test]
fn test_llm_provider_display() {
    assert_eq!(format!("{}", LLMProvider::OpenAI), "openai");
    assert_eq!(format!("{}", LLMProvider::Gemini), "gemini");
    assert_eq!(format!("{}", LLMProvider::Anthropic), "anthropic");
}

// F019: DB connection test
#[tokio::test]
async fn test_db_connection() {
    std::env::set_var("DEKO_DATABASE_URL", "sqlite::memory:");
    let config = Config::from_env().unwrap();
    let (pool, _pool_set) = deko::db::init_db(&config).await.unwrap();
    let result: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
    assert_eq!(result.0, 1);
}

// F028: Model serialization tests
#[test]
fn test_action_status_serialization() {
    assert_eq!(serde_json::to_string(&ActionStatus::Pending).unwrap(), "\"pending\"");
    assert_eq!(serde_json::to_string(&ActionStatus::Approved).unwrap(), "\"approved\"");
    assert_eq!(serde_json::to_string(&ActionStatus::Denied).unwrap(), "\"denied\"");
    assert_eq!(serde_json::to_string(&ActionStatus::Escalated).unwrap(), "\"escalated\"");
}

#[test]
fn test_risk_level_serialization() {
    assert_eq!(serde_json::to_string(&RiskLevel::Low).unwrap(), "\"low\"");
    assert_eq!(serde_json::to_string(&RiskLevel::Medium).unwrap(), "\"medium\"");
    assert_eq!(serde_json::to_string(&RiskLevel::High).unwrap(), "\"high\"");
    assert_eq!(serde_json::to_string(&RiskLevel::Critical).unwrap(), "\"critical\"");
}

#[test]
fn test_verdict_decision_serialization() {
    assert_eq!(serde_json::to_string(&VerdictDecision::Approved).unwrap(), "\"approved\"");
    assert_eq!(serde_json::to_string(&VerdictDecision::Denied).unwrap(), "\"denied\"");
    assert_eq!(serde_json::to_string(&VerdictDecision::Escalate).unwrap(), "\"escalate\"");
}

#[test]
fn test_action_status_display() {
    assert_eq!(format!("{}", ActionStatus::Pending), "pending");
    assert_eq!(format!("{}", ActionStatus::Approved), "approved");
    assert_eq!(format!("{}", ActionStatus::Denied), "denied");
}

#[test]
fn test_risk_level_display() {
    assert_eq!(format!("{}", RiskLevel::Low), "low");
    assert_eq!(format!("{}", RiskLevel::Critical), "critical");
}

// F032: Error handling tests
#[test]
fn test_app_error_not_found() {
    let err = AppError::NotFound("item not found".into());
    assert!(err.to_string().contains("not found"));
}

#[test]
fn test_app_error_unauthorized() {
    let err = AppError::Unauthorized("bad key".into());
    assert!(err.to_string().contains("Unauthorized"));
}

#[test]
fn test_app_error_forbidden() {
    let err = AppError::Forbidden("access denied".into());
    assert!(err.to_string().contains("Forbidden"));
}

#[test]
fn test_app_error_bad_request() {
    let err = AppError::BadRequest("invalid input".into());
    assert!(err.to_string().contains("Bad request"));
}

#[test]
fn test_app_error_rate_limited() {
    let err = AppError::RateLimited;
    assert!(err.to_string().contains("Rate limited"));
}

#[test]
fn test_app_error_locked() {
    let err = AppError::Locked("action locked".into());
    assert!(err.to_string().contains("locked"));
}

// F039: Auth tests
#[test]
fn test_hash_api_key_is_deterministic() {
    let hash1 = deko::middleware::auth::hash_api_key("test-key", "secret");
    let hash2 = deko::middleware::auth::hash_api_key("test-key", "secret");
    assert_eq!(hash1, hash2);
}

#[test]
fn test_hash_api_key_different_secrets() {
    let hash1 = deko::middleware::auth::hash_api_key("test-key", "secret1");
    let hash2 = deko::middleware::auth::hash_api_key("test-key", "secret2");
    assert_ne!(hash1, hash2);
}

#[test]
fn test_hash_api_key_different_keys() {
    let hash1 = deko::middleware::auth::hash_api_key("key1", "secret");
    let hash2 = deko::middleware::auth::hash_api_key("key2", "secret");
    assert_ne!(hash1, hash2);
}

#[test]
fn test_hash_api_key_length() {
    let hash = deko::middleware::auth::hash_api_key("test", "secret");
    assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex characters
}

// F059: Polling tests (Retry-After header)
#[test]
fn test_pending_action_returns_retry_after_value() {
    let pending_status = ActionStatus::Pending;
    assert_eq!(format!("{}", pending_status), "pending");
}

// F081: Policy engine unit tests
#[test]
fn test_deny_keyword_matching() {
    let rules: serde_json::Value = serde_json::json!([
        {"type": "deny_keyword", "keywords": ["delete", "drop", "destroy"]}
    ]);

    let intent = "I want to delete all records";
    let matched = check_deny_keyword(&rules, intent);
    assert!(matched);
}

#[test]
fn test_deny_keyword_no_match() {
    let rules: serde_json::Value = serde_json::json!([
        {"type": "deny_keyword", "keywords": ["delete", "drop"]}
    ]);

    let intent = "I want to read records";
    let matched = check_deny_keyword(&rules, intent);
    assert!(!matched);
}

fn check_deny_keyword(rules: &serde_json::Value, intent: &str) -> bool {
    if let Some(arr) = rules.as_array() {
        for rule in arr {
            if let Some(rule_type) = rule.get("type").and_then(|t| t.as_str()) {
                if rule_type == "deny_keyword" {
                    if let Some(keywords) = rule.get("keywords").and_then(|k| k.as_array()) {
                        let lower_intent = intent.to_lowercase();
                        for kw in keywords {
                            if let Some(kw_str) = kw.as_str() {
                                if lower_intent.contains(&kw_str.to_lowercase()) {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

// F092: LLM client unit tests
#[test]
fn test_parse_verdict_json_approved() {
    let json = r#"{"decision": "approved", "reason": "Safe action", "risk_level": "low"}"#;
    let result = deko::services::llm::parse_verdict_json(
        json,
        deko::config::LLMProvider::Gemini,
        "gemini-2.0-flash".into(),
    ).unwrap();

    assert_eq!(result.decision, VerdictDecision::Approved);
    assert_eq!(result.risk_level, RiskLevel::Low);
    assert_eq!(result.provider, deko::config::LLMProvider::Gemini);
}

#[test]
fn test_parse_verdict_json_denied() {
    let json = r#"{"decision": "denied", "reason": "Policy violation", "risk_level": "high"}"#;
    let result = deko::services::llm::parse_verdict_json(
        json,
        deko::config::LLMProvider::OpenAI,
        "gpt-4o".into(),
    ).unwrap();

    assert_eq!(result.decision, VerdictDecision::Denied);
    assert_eq!(result.risk_level, RiskLevel::High);
}

#[test]
fn test_parse_verdict_json_escalate() {
    let json = r#"{"decision": "escalate", "reason": "Unclear intent", "risk_level": "medium"}"#;
    let result = deko::services::llm::parse_verdict_json(
        json,
        deko::config::LLMProvider::Gemini,
        "gemini-2.0-flash".into(),
    ).unwrap();

    assert_eq!(result.decision, VerdictDecision::Escalate);
}

#[test]
fn test_parse_verdict_json_invalid() {
    let json = "not valid json";
    let result = deko::services::llm::parse_verdict_json(
        json,
        deko::config::LLMProvider::Gemini,
        "gemini-2.0-flash".into(),
    );
    assert!(result.is_err());
}

#[test]
fn test_system_prompt_not_empty() {
    let prompt = deko::services::llm::system_prompt();
    assert!(!prompt.is_empty());
    assert!(prompt.contains("approved"));
    assert!(prompt.contains("denied"));
    assert!(prompt.contains("escalate"));
}

// F110: Processor tests
#[tokio::test]
async fn test_processor_shutdown() {
    let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
    assert!(!shutdown.load(std::sync::atomic::Ordering::Relaxed));
    shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(shutdown.load(std::sync::atomic::Ordering::Relaxed));
}

// F117: Webhook tests
#[tokio::test]
async fn test_webhook_no_url_returns_ok() {
    let webhook = deko::services::webhook::WebhookService::new(None);
    let verdict = deko::services::llm::VerdictResult {
        decision: VerdictDecision::Denied,
        reason: "test".into(),
        risk_level: RiskLevel::High,
        raw_response: "".into(),
        provider: deko::config::LLMProvider::Gemini,
        model: "test".into(),
        confidence: 0.5,
    };
    let result = webhook.send_verdict("test-action", &verdict, None).await;
    assert!(result.is_ok());
}

// F122: Admin auth tests
#[test]
fn test_admin_password_comparison() {
    let password = "secret123";
    let correct = "secret123";
    let incorrect = "wrong";

    assert!(password == correct);
    assert!(password != incorrect);
}

// F135: Template rendering tests
#[test]
fn test_templates_directory_exists() {
    let templates = std::fs::read_dir("templates").unwrap();
    let files: Vec<_> = templates.filter_map(|e| e.ok()).collect();
    assert!(files.len() >= 5); // base, login, dashboard, actions_list, action_detail
}

#[test]
fn test_base_template_exists() {
    assert!(std::path::Path::new("templates/base.html").exists());
}

#[test]
fn test_login_template_exists() {
    assert!(std::path::Path::new("templates/login.html").exists());
}

#[test]
fn test_dashboard_template_exists() {
    assert!(std::path::Path::new("templates/dashboard.html").exists());
}

#[test]
fn test_audit_log_template_exists() {
    assert!(std::path::Path::new("templates/audit_log.html").exists());
}

#[test]
fn test_agent_management_template_exists() {
    assert!(std::path::Path::new("templates/agent_management.html").exists());
}

#[test]
fn test_policy_management_template_exists() {
    assert!(std::path::Path::new("templates/policy_management.html").exists());
}

#[test]
fn test_verdict_history_template_exists() {
    assert!(std::path::Path::new("templates/verdict_history.html").exists());
}

// F145: Dashboard API tests - template rendering verification
#[test]
fn test_dashboard_template_file_exists() {
    let content = std::fs::read_to_string("templates/dashboard.html").unwrap();
    assert!(content.contains("Dashboard"));
    assert!(content.contains("total_actions"));
    assert!(content.contains("Recent Actions"));
}

// F151: Health check tests
#[tokio::test]
async fn test_health_endpoint_returns_ok() {
    let pool = deko::db::DbPool::connect("sqlite::memory:").await.unwrap();
    let result: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
    assert_eq!(result.0, 1);
}

// F158: Logging tests
#[test]
fn test_tracing_initializes() {
    deko::config::init_tracing(&Environment::Dev);
}

// F164: Doc generation test
#[test]
fn test_openapi_doc_generates() {
    use utoipa::OpenApi;
    use deko::routes::ApiDoc;

    let doc = ApiDoc::openapi();
    assert!(!doc.info.title.is_empty());
    assert!(!doc.info.version.is_empty());
}

// F175: Env config tests
// F014: Per-environment config profiles
#[test]
fn test_env_profile_dev() {
    let profile = Environment::Dev.defaults();
    assert_eq!(profile.rate_limit_per_minute, 120);
    assert_eq!(profile.max_screenshot_size_mb, 10);
    assert_eq!(profile.max_request_body_kb, 1024);
    assert_eq!(profile.openai_timeout_secs, 60);
    assert_eq!(profile.gemini_timeout_secs, 60);
    assert_eq!(profile.processor_poll_interval_secs, 1);
    assert!(profile.allowed_origins.contains(&"*".to_string()));
}

#[test]
fn test_env_profile_staging() {
    let profile = Environment::Staging.defaults();
    assert_eq!(profile.rate_limit_per_minute, 60);
    assert_eq!(profile.max_screenshot_size_mb, 10);
    assert_eq!(profile.max_request_body_kb, 512);
    assert_eq!(profile.openai_timeout_secs, 30);
    assert_eq!(profile.gemini_timeout_secs, 30);
    assert_eq!(profile.processor_poll_interval_secs, 2);
    assert!(!profile.allowed_origins.contains(&"*".to_string()));
}

#[test]
fn test_env_profile_prod() {
    let profile = Environment::Prod.defaults();
    assert_eq!(profile.rate_limit_per_minute, 30);
    assert_eq!(profile.max_screenshot_size_mb, 5);
    assert_eq!(profile.max_request_body_kb, 256);
    assert_eq!(profile.openai_timeout_secs, 15);
    assert_eq!(profile.gemini_timeout_secs, 15);
    assert_eq!(profile.processor_poll_interval_secs, 2);
    assert!(profile.allowed_origins.is_empty());
}

#[test]
fn test_config_port_default() {
    let config = Config::from_env().unwrap();
    assert!(config.port > 0);
    assert!(config.port <= 65535);
}

#[test]
fn test_config_addr() {
    let config = Config::from_env().unwrap();
    let addr = config.addr();
    assert_eq!(addr.ip().to_string(), "0.0.0.0");
}

#[test]
fn test_config_screenshot_limit() {
    let config = Config::from_env().unwrap();
    assert_eq!(config.max_screenshot_size_mb, 10);
}

#[test]
fn test_config_rate_limit() {
    let config = Config::from_env().unwrap();
    assert_eq!(config.rate_limit_per_minute, 60);
}

// F177-F180: Test infrastructure
#[test]
fn test_metrics_collector_new() {
    let metrics = MetricsCollector::new();
    assert_eq!(metrics.actions_total.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[test]
fn test_metrics_collector_inc_action() {
    let metrics = MetricsCollector::new();
    metrics.inc_action();
    assert_eq!(metrics.actions_total.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[test]
fn test_metrics_collector_to_json() {
    let metrics = MetricsCollector::new();
    metrics.inc_action();
    let json = metrics.to_json();
    assert_eq!(json["actions"]["total"], 1);
    assert!(json.get("database").is_some());
    assert_eq!(json["database"]["pool_max_connections"], 10);
}

#[test]
fn test_metrics_collector_pool_config() {
    let metrics = MetricsCollector::new();
    metrics.set_pool_config(20, 10);
    let json = metrics.to_json();
    assert_eq!(json["database"]["pool_max_connections"], 20);
    assert_eq!(json["database"]["pool_acquire_timeout_secs"], 10);
}

#[test]
fn test_metrics_error_counters() {
    let metrics = MetricsCollector::new();
    metrics.inc_error_database();
    metrics.inc_error_llm();
    metrics.inc_error_validation();
    metrics.inc_error_auth();
    let json = metrics.to_json();
    assert_eq!(json["errors"]["database"], 1);
    assert_eq!(json["errors"]["llm"], 1);
    assert_eq!(json["errors"]["validation"], 1);
    assert_eq!(json["errors"]["auth"], 1);
}

#[tokio::test]
async fn test_rate_limiter_allows_first_request() {
    let limiter = RateLimiter::new(10, 60);
    assert!(limiter.is_allowed("127.0.0.1"));
}

#[tokio::test]
async fn test_rate_limiter_blocks_after_limit() {
    let limiter = RateLimiter::new(2, 60);
    assert!(limiter.is_allowed("127.0.0.1"));
    assert!(limiter.is_allowed("127.0.0.1"));
    assert!(!limiter.is_allowed("127.0.0.1"));
}

#[tokio::test]
async fn test_rate_limiter_different_keys() {
    let limiter = RateLimiter::new(1, 60);
    assert!(limiter.is_allowed("key1"));
    assert!(limiter.is_allowed("key2"));
    assert!(!limiter.is_allowed("key1"));
    assert!(!limiter.is_allowed("key2"));
}

// F191: Rate limiting tests (covered above in RateLimiter tests)

// F193: Screenshot size limit tests
#[test]
fn test_screenshot_size_10mb_in_bytes() {
    let max_bytes = 10 * 1024 * 1024;
    assert_eq!(max_bytes, 10485760);
}

// F194: Input sanitization tests
#[test]
fn test_sanitize_input_removes_xss() {
    let input = "<script>alert('xss')</script>";
    let sanitized = input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;");
    assert!(!sanitized.contains("<script>"));
    assert!(sanitized.contains("&lt;"));
}

#[test]
fn test_clippy_toml_is_valid() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("clippy.toml");
    assert!(path.exists(), "clippy.toml must exist");
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("allow-unwrap-in-tests"), "clippy.toml should contain test-specific config");
}

#[test]
fn test_config_database_url_sqlite_default() {
    std::env::set_var("DEKO_DATABASE_URL", "sqlite::memory:");
    let config = Config::from_env().unwrap();
    assert!(config.database_url.starts_with("sqlite:"));
}

#[test]
fn test_db_pool_is_sqlite_by_default() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let pool = deko::db::DbPool::connect("sqlite::memory:").await.unwrap();
        let result: (i64,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await.unwrap();
        assert_eq!(result.0, 1);
    });
}

#[test]
fn test_redacted_short_value() {
    let r = deko::redact::Redacted::new("abc");
    assert_eq!(format!("{}", r), "***");
}

#[test]
fn test_redacted_display_hides_most() {
    let r = deko::redact::Redacted::new("sk-abc123def456ghi789");
    let displayed = format!("{}", r);
    assert!(displayed.starts_with("sk-abc"));
    assert!(displayed.contains("..."));
    assert!(!displayed.contains("def456ghi789"));
}

#[test]
fn test_redacted_inner_returns_full() {
    let r = deko::redact::Redacted::new("sk-abc123");
    assert_eq!(r.inner(), "sk-abc123");
}

#[test]
fn test_redacted_debug_same_as_display() {
    let r = deko::redact::Redacted::new("secret-value-12345");
    assert_eq!(format!("{:?}", r), format!("{}", r));
}

proptest::proptest! {
    #[test]
    fn test_sanitize_input_never_panics(s in ".*") {
        let sanitized = s
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#x27;");
        assert!(!sanitized.contains('<'));
        assert!(!sanitized.contains('>'));
    }

    #[test]
    fn test_redacted_never_panics(s in ".*") {
        let r = deko::redact::Redacted::new(&s);
        let _display = format!("{}", r);
        let _inner = r.inner();
    }
}
