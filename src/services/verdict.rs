use std::sync::Arc;
use crate::db::DbPool;
use std::time::Instant;
use chrono::{Datelike, Timelike};
use tracing::{info, warn};

use crate::config::{Config, LLMProvider};
use crate::error::{AppError, Result};
use crate::models::{ActionStatus, Policy};
use crate::services::llm::LLMProviderTrait;
use crate::services::llm::VerdictResult;
use crate::services::providers::{GeminiProvider, OpenAIProvider};
use crate::services::webhook::WebhookService;
use crate::services::metrics::MetricsCollector;

pub struct VerdictService {
    pub pool: DbPool,
    pub providers: Vec<Box<dyn LLMProviderTrait>>,
    pub default_provider_idx: usize,
    pub webhook: WebhookService,
    pub metrics: Arc<MetricsCollector>,
}

impl VerdictService {
    pub fn new(pool: DbPool, config: &Config, metrics: Arc<MetricsCollector>) -> Self {
        let mut providers: Vec<Box<dyn LLMProviderTrait>> = Vec::new();
        let mut default_idx = 0;

        if config.gemini_api_key.is_some() {
            providers.push(Box::new(GeminiProvider::new(
                config.gemini_api_key.clone().unwrap(),
                config.gemini_model.clone(),
                config.gemini_timeout_secs,
            )));
            if config.default_provider == LLMProvider::Gemini {
                default_idx = providers.len() - 1;
            }
        }

        if config.openai_api_key.is_some() {
            providers.push(Box::new(OpenAIProvider::new(
                config.openai_api_key.clone().unwrap(),
                config.openai_model.clone(),
                config.openai_timeout_secs,
            )));
            if config.default_provider == LLMProvider::OpenAI {
                default_idx = providers.len() - 1;
            }
        }

        if providers.is_empty() {
            panic!("At least one LLM provider must be configured (GEMINI_API_KEY or OPENAI_API_KEY)");
        }

        Self {
            pool: pool.clone(),
            providers,
            default_provider_idx: default_idx,
            webhook: WebhookService::new(config.webhook_url.clone()),
            metrics,
        }
    }

    pub async fn process_action(&self, action_id: &str) -> Result<()> {
        let action = sqlx::query_as::<_, crate::models::Action>(
            "SELECT id, agent_id, intent, payload, screenshot_base64, metadata, status, target_url, target_method, created_at, updated_at, idempotency_key, priority, execute_at FROM actions WHERE id = ?",
        )
        .bind(action_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Action not found".into()))?;

        let agent_id = action.agent_id.clone();

        if action.status != ActionStatus::Pending {
            info!("Action {} already processed (status: {:?}), skipping", action_id, action.status);
            return Ok(());
        }

        sqlx::query("UPDATE actions SET status = ? WHERE id = ?")
            .bind(ActionStatus::Processing)
            .bind(action_id)
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;

        self.audit(action_id, "processing_started", &serde_json::json!({
            "intent": action.intent,
        }))
        .await?;

        let policy_result = self.evaluate_policies(&action).await?;

        if policy_result.immediate_deny {
            self.save_verdict(
                action_id,
                &agent_id,
                VerdictResult {
                    decision: crate::models::VerdictDecision::Denied,
                    reason: policy_result.reason.unwrap_or_else(|| "Blocked by policy rule".to_string()),
                    risk_level: policy_result.risk_level.unwrap_or(crate::models::RiskLevel::High),
                    raw_response: String::new(),
                    provider: LLMProvider::Gemini,
                    model: "policy_engine".to_string(),
                    confidence: 1.0,
                },
                policy_result.matched_policy_id,
            )
            .await?;
            return Ok(());
        }

        let verdict_result = self.try_llm_analysis(
            &action,
            &policy_result.context,
        ).await;

        self.save_verdict(
            action_id,
            &agent_id,
            verdict_result,
            policy_result.matched_policy_id,
        )
        .await?;

        Ok(())
    }

    pub async fn test_policies(&self, policies: &[Policy], action: &crate::models::Action) -> Option<PolicyEvaluation> {
        let is_dry_run = true;
        let mut context_parts = Vec::new();

        for policy in policies {
            let rules: serde_json::Value = policy.rules.clone();
            if let Some(arr) = rules.as_array() {
                for rule in arr {
                    if let Some(result) = self.evaluate_rule(rule, action) {
                        context_parts.push(format!("{}: {}", policy.name, result.message));
                        if result.immediate_deny {
                            return Some(PolicyEvaluation {
                                immediate_deny: true,
                                reason: Some(format!("Policy '{}' violated: {}", policy.name, result.message)),
                                risk_level: Some(result.risk_level),
                                matched_policy_id: Some(policy.id.clone()),
                                context: context_parts.join("; "),
                            });
                        }
                    }
                }
            }
        }

        if context_parts.is_empty() {
            None
        } else {
            Some(PolicyEvaluation {
                immediate_deny: false,
                reason: Some(context_parts.join("; ")),
                risk_level: None,
                matched_policy_id: None,
                context: context_parts.join("; "),
            })
        }
    }

    async fn save_verdict(
        &self,
        action_id: &str,
        agent_id: &str,
        verdict: VerdictResult,
        policy_matched: Option<String>,
    ) -> Result<()> {
        let verdict_id = uuid::Uuid::new_v4().to_string();

        let status = match verdict.decision {
            crate::models::VerdictDecision::Approved => {
                self.metrics.inc_action_approved();
                ActionStatus::Approved
            }
            crate::models::VerdictDecision::Denied => {
                self.metrics.inc_action_denied();
                ActionStatus::Denied
            }
            crate::models::VerdictDecision::Escalate => {
                self.metrics.inc_action_escalated();
                ActionStatus::Escalated
            }
        };
        self.metrics.inc_action();

        let mut tx = self.pool.begin().await.map_err(AppError::Database)?;

        sqlx::query(
            "INSERT INTO verdicts (id, action_id, decision, reason, risk_level, policy_matched, llm_raw_response) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&verdict_id)
        .bind(action_id)
        .bind(&verdict.decision)
        .bind(&verdict.reason)
        .bind(&verdict.risk_level)
        .bind(&policy_matched)
        .bind(&verdict.raw_response)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Database)?;

        sqlx::query("UPDATE actions SET status = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(&status)
            .bind(action_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::Database)?;

        self.audit_tx(
            &mut tx,
            action_id,
            "verdict_issued",
            &serde_json::json!({
                "verdict_id": verdict_id,
                "decision": verdict.decision,
                "reason": verdict.reason,
                "risk_level": verdict.risk_level,
                "provider": verdict.provider,
                "model": verdict.model,
                "confidence": verdict.confidence,
                "policy_matched": policy_matched,
            }),
        )
        .await?;

        tx.commit().await.map_err(AppError::Database)?;

        if matches!(verdict.decision, crate::models::VerdictDecision::Denied | crate::models::VerdictDecision::Escalate) {
            let agent_webhook: Option<(Option<String>,)> = sqlx::query_as(
                "SELECT webhook_url FROM agents WHERE id = ?"
            )
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::Database)?;

            let webhook_url = agent_webhook.and_then(|r| r.0);
            if let Err(e) = self.webhook.send_verdict(action_id, &verdict, webhook_url.as_deref()).await {
                warn!("Failed to send webhook for action {}: {}", action_id, e);
            }
        }

        info!(
            "Action {} verdict: {:?} (risk: {:?}, provider: {}) - {}",
            action_id, verdict.decision, verdict.risk_level, verdict.provider, verdict.reason
        );

        Ok(())
    }

    async fn audit(&self, action_id: &str, event_type: &str, details: &serde_json::Value) -> Result<()> {
        sqlx::query(
            "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(action_id)
        .bind(event_type)
        .bind(details)
        .execute(&self.pool)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    #[cfg(not(feature = "postgres"))]
    async fn audit_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        action_id: &str,
        event_type: &str,
        details: &serde_json::Value,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO audit_log (id, action_id, event_type, details) VALUES (?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(action_id)
        .bind(event_type)
        .bind(details)
        .execute(&mut **tx)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }

    #[cfg(feature = "postgres")]
    async fn audit_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        action_id: &str,
        event_type: &str,
        details: &serde_json::Value,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO audit_log (id, action_id, event_type, details) VALUES ($1, $2, $3, $4)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(action_id)
        .bind(event_type)
        .bind(details)
        .execute(&mut **tx)
        .await
        .map_err(AppError::Database)?;
        Ok(())
    }
}

pub struct PolicyEvaluation {
    immediate_deny: bool,
    reason: Option<String>,
    risk_level: Option<crate::models::RiskLevel>,
    matched_policy_id: Option<String>,
    context: String,
}

pub struct RuleResult {
    immediate_deny: bool,
    message: String,
    risk_level: crate::models::RiskLevel,
}
