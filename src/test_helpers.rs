use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::config::LLMProvider;
use crate::error::Result;
use crate::models::{RiskLevel, VerdictDecision};
use crate::services::llm::{LLMProviderTrait, VerdictResult};

pub struct MockLLMProvider {
    pub decision: VerdictDecision,
    pub reason: String,
    pub risk_level: RiskLevel,
    pub call_count: Arc<AtomicUsize>,
    pub should_fail: bool,
    pub fail_message: String,
}

impl MockLLMProvider {
    pub fn approved() -> Self {
        Self {
            decision: VerdictDecision::Approved,
            reason: "Action is safe and within policy".to_string(),
            risk_level: RiskLevel::Low,
            call_count: Arc::new(AtomicUsize::new(0)),
            should_fail: false,
            fail_message: String::new(),
        }
    }

    pub fn denied() -> Self {
        Self {
            decision: VerdictDecision::Denied,
            reason: "Action violates security policy".to_string(),
            risk_level: RiskLevel::High,
            call_count: Arc::new(AtomicUsize::new(0)),
            should_fail: false,
            fail_message: String::new(),
        }
    }

    pub fn escalated() -> Self {
        Self {
            decision: VerdictDecision::Escalate,
            reason: "Action requires human judgment".to_string(),
            risk_level: RiskLevel::Medium,
            call_count: Arc::new(AtomicUsize::new(0)),
            should_fail: false,
            fail_message: String::new(),
        }
    }

    pub fn failing(message: &str) -> Self {
        Self {
            decision: VerdictDecision::Denied,
            reason: String::new(),
            risk_level: RiskLevel::Low,
            call_count: Arc::new(AtomicUsize::new(0)),
            should_fail: true,
            fail_message: message.to_string(),
        }
    }

    pub fn call_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl LLMProviderTrait for MockLLMProvider {
    fn name(&self) -> LLMProvider {
        LLMProvider::Gemini
    }

    fn model_name(&self) -> String {
        "mock-model".to_string()
    }

    async fn analyze_action(
        &self,
        _intent: &str,
        _payload: Option<&str>,
        _screenshot_base64: Option<&str>,
        _policy_context: &str,
    ) -> Result<VerdictResult> {
        self.call_count.fetch_add(1, Ordering::SeqCst);

        if self.should_fail {
            return Err(crate::error::AppError::Gemini(self.fail_message.clone()));
        }

        Ok(VerdictResult {
            decision: self.decision.clone(),
            reason: self.reason.clone(),
            risk_level: self.risk_level.clone(),
            raw_response: format!(
                "{{\"decision\": \"{}\", \"reason\": \"{}\", \"risk_level\": \"{}\"}}",
                match &self.decision {
                    VerdictDecision::Approved => "approved",
                    VerdictDecision::Denied => "denied",
                    VerdictDecision::Escalate => "escalate",
                },
                self.reason,
                match &self.risk_level {
                    RiskLevel::Low => "low",
                    RiskLevel::Medium => "medium",
                    RiskLevel::High => "high",
                    RiskLevel::Critical => "critical",
                },
            ),
            provider: LLMProvider::Gemini,
            model: "mock-model".to_string(),
        })
    }
}

pub struct TestFixtures;

impl TestFixtures {
    pub async fn create_agent(pool: &sqlx::SqlitePool, name: &str) -> sqlx::Result<(String, String)> {
        let agent_id = uuid::Uuid::new_v4().to_string();
        let api_key = uuid::Uuid::new_v4().to_string();
        let api_key_hash = crate::middleware::auth::hash_api_key_with_secret(
            &api_key,
            &std::env::var("DEKO_API_KEY_SECRET").unwrap_or_else(|_| "test-secret-key-12345678".to_string()),
        );

        sqlx::query(
            "INSERT INTO agents (id, name, api_key_hash, active) VALUES (?, ?, ?, 1)",
        )
        .bind(&agent_id)
        .bind(name)
        .bind(&api_key_hash)
        .execute(pool)
        .await?;

        Ok((agent_id, api_key))
    }

    pub async fn create_action(
        pool: &sqlx::SqlitePool,
        agent_id: &str,
        intent: &str,
    ) -> sqlx::Result<String> {
        let action_id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO actions (id, agent_id, intent, payload, screenshot_base64, metadata, target_url, target_method, status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&action_id)
        .bind(agent_id)
        .bind(intent)
        .bind::<Option<String>>(None)
        .bind::<Option<String>>(None)
        .bind::<Option<String>>(None)
        .bind::<Option<String>>(None)
        .bind::<Option<String>>(None)
        .bind("pending")
        .execute(pool)
        .await?;

        Ok(action_id)
    }

    pub async fn create_action_with_details(
        pool: &sqlx::SqlitePool,
        agent_id: &str,
        intent: &str,
        payload: Option<&str>,
        target_url: Option<&str>,
        target_method: Option<&str>,
    ) -> sqlx::Result<String> {
        let action_id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO actions (id, agent_id, intent, payload, screenshot_base64, metadata, target_url, target_method, status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&action_id)
        .bind(agent_id)
        .bind(intent)
        .bind(payload)
        .bind::<Option<String>>(None)
        .bind::<Option<String>>(None)
        .bind(target_url)
        .bind(target_method)
        .bind("pending")
        .execute(pool)
        .await?;

        Ok(action_id)
    }

    pub async fn create_policy(
        pool: &sqlx::SqlitePool,
        name: &str,
        rules_json: &str,
    ) -> sqlx::Result<String> {
        let policy_id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO policies (id, name, description, rules, active) VALUES (?, ?, ?, ?, 1)",
        )
        .bind(&policy_id)
        .bind(name)
        .bind("")
        .bind(rules_json)
        .execute(pool)
        .await?;

        Ok(policy_id)
    }

    pub async fn create_deny_keyword_policy(
        pool: &sqlx::SqlitePool,
        name: &str,
        keywords: &[&str],
    ) -> sqlx::Result<String> {
        let rules = serde_json::json!([{
            "type": "deny_keyword",
            "keywords": keywords,
            "immediate_deny": true
        }]);

        Self::create_policy(pool, name, &rules.to_string()).await
    }

    pub async fn create_max_amount_policy(
        pool: &sqlx::SqlitePool,
        name: &str,
        max: f64,
    ) -> sqlx::Result<String> {
        let rules = serde_json::json!([{
            "type": "max_amount",
            "max": max,
            "immediate_deny": true
        }]);

        Self::create_policy(pool, name, &rules.to_string()).await
    }
}

pub struct TestApp {
    pub pool: sqlx::SqlitePool,
    pub metrics: Arc<crate::services::MetricsCollector>,
}

impl TestApp {
    pub async fn setup() -> Self {
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

        let config = crate::config::Config::from_env().unwrap();
        let pool = crate::db::init_db(&config).await.unwrap();
        crate::db::run_migrations(&pool).await.unwrap();

        Self {
            pool,
            metrics: Arc::new(crate::services::MetricsCollector::new()),
        }
    }

    pub async fn setup_with_agent(&self, name: &str) -> (String, String) {
        TestFixtures::create_agent(&self.pool, name).await.unwrap()
    }

    pub async fn setup_with_action(&self, agent_id: &str, intent: &str) -> String {
        TestFixtures::create_action(&self.pool, agent_id, intent).await.unwrap()
    }
}
