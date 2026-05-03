use sqlx::SqlitePool;
use tracing::{info, warn};

use crate::config::{Config, LLMProvider};
use crate::error::{AppError, Result};
use crate::models::{ActionStatus, Policy};
use crate::services::llm::LLMProviderTrait;
use crate::services::llm::VerdictResult;
use crate::services::providers::{GeminiProvider, OpenAIProvider};
use crate::services::webhook::WebhookService;

pub struct VerdictService {
    pub pool: SqlitePool,
    pub providers: Vec<Box<dyn LLMProviderTrait>>,
    pub default_provider_idx: usize,
    pub webhook: WebhookService,
}

impl VerdictService {
    pub fn new(pool: SqlitePool, config: &Config) -> Self {
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
        }
    }

    pub async fn process_action(&self, action_id: &str) -> Result<()> {
        let action = sqlx::query_as::<_, crate::models::Action>(
            "SELECT id, agent_id, intent, payload, screenshot_base64, metadata, status, target_url, target_method, created_at, updated_at FROM actions WHERE id = ?",
        )
        .bind(action_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::Database)?
        .ok_or_else(|| AppError::NotFound("Action not found".into()))?;

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
                VerdictResult {
                    decision: crate::models::VerdictDecision::Denied,
                    reason: policy_result.reason.unwrap_or_else(|| "Blocked by policy rule".to_string()),
                    risk_level: policy_result.risk_level.unwrap_or(crate::models::RiskLevel::High),
                    raw_response: String::new(),
                    provider: LLMProvider::Gemini,
                    model: "policy_engine".to_string(),
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
            verdict_result,
            policy_result.matched_policy_id,
        )
        .await?;

        Ok(())
    }

    async fn try_llm_analysis(
        &self,
        action: &crate::models::Action,
        policy_context: &str,
    ) -> VerdictResult {
        let primary = &self.providers[self.default_provider_idx];

        match primary.analyze_action(
            &action.intent,
            action.payload.as_deref(),
            action.screenshot_base64.as_deref(),
            policy_context,
        ).await {
            Ok(result) => result,
            Err(e) => {
                warn!("Primary provider failed: {}", e);
                if self.providers.len() > 1 {
                    let fallback_idx = if self.default_provider_idx == 0 { 1 } else { 0 };
                    let fallback = &self.providers[fallback_idx];
                    match fallback.analyze_action(
                        &action.intent,
                        action.payload.as_deref(),
                        action.screenshot_base64.as_deref(),
                        policy_context,
                    ).await {
                        Ok(result) => result,
                        Err(e2) => {
                            warn!("Fallback provider also failed: {}", e2);
                            VerdictResult {
                                decision: crate::models::VerdictDecision::Denied,
                                reason: format!("All LLM providers failed. Primary: {}, Fallback: {}", e, e2),
                                risk_level: crate::models::RiskLevel::High,
                                raw_response: String::new(),
                                provider: fallback.name(),
                                model: fallback.model_name(),
                            }
                        }
                    }
                } else {
                    VerdictResult {
                        decision: crate::models::VerdictDecision::Denied,
                        reason: format!("LLM analysis failed: {}", e),
                        risk_level: crate::models::RiskLevel::High,
                        raw_response: String::new(),
                        provider: primary.name(),
                        model: primary.model_name(),
                    }
                }
            }
        }
    }

    async fn evaluate_policies(&self, action: &crate::models::Action) -> Result<PolicyEvaluation> {
        let policies: Vec<Policy> = sqlx::query_as(
            "SELECT id, name, description, rules, active, created_at, updated_at FROM policies WHERE active = 1",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        let mut context_parts = Vec::new();

        for policy in &policies {
            let rules: serde_json::Value = policy.rules.clone();

            if let Some(arr) = rules.as_array() {
                for rule in arr {
                    if let Some(result) = self.evaluate_rule(rule, action) {
                        context_parts.push(format!("{}: {}", policy.name, result.message));
                        if result.immediate_deny {
                            return Ok(PolicyEvaluation {
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

        Ok(PolicyEvaluation {
            immediate_deny: false,
            reason: None,
            risk_level: None,
            matched_policy_id: None,
            context: if context_parts.is_empty() {
                "No active policy rules matched".to_string()
            } else {
                context_parts.join("; ")
            },
        })
    }

    fn evaluate_rule(&self, rule: &serde_json::Value, action: &crate::models::Action) -> Option<RuleResult> {
        let rule_type = rule.get("type")?.as_str()?;

        match rule_type {
            "deny_keyword" => {
                let keywords = rule.get("keywords")?.as_array()?;
                let intent_lower = action.intent.to_lowercase();
                for kw in keywords {
                    if let Some(kw_str) = kw.as_str() {
                        if intent_lower.contains(&kw_str.to_lowercase()) {
                            return Some(RuleResult {
                                immediate_deny: true,
                                message: format!("Denied keyword match: {}", kw_str),
                                risk_level: crate::models::RiskLevel::Critical,
                            });
                        }
                    }
                }
            }
            "require_approval" => {
                let action_types = rule.get("action_types")?.as_array()?;
                if let Some(method) = &action.target_method {
                    for at in action_types {
                        if let Some(at_str) = at.as_str() {
                            if method.to_uppercase() == at_str.to_uppercase() {
                                return Some(RuleResult {
                                    immediate_deny: false,
                                    message: format!("Requires human approval for {} actions", at_str),
                                    risk_level: crate::models::RiskLevel::High,
                                });
                            }
                        }
                    }
                }
            }
            "max_amount" => {
                let max = rule.get("max")?.as_f64()?;
                if let Some(payload_str) = &action.payload {
                    if let Ok(payload_json) = serde_json::from_str::<serde_json::Value>(payload_str) {
                        if let Some(amount) = payload_json.get("amount").and_then(|v| v.as_f64()) {
                            if amount > max {
                                return Some(RuleResult {
                                    immediate_deny: true,
                                    message: format!("Amount {} exceeds maximum {}", amount, max),
                                    risk_level: crate::models::RiskLevel::High,
                                });
                            }
                        }
                    }
                }
            }
            "regex_deny" => {
                let patterns = rule.get("patterns")?.as_array()?;
                let full_text = format!("{} {}", action.intent, action.payload.as_deref().unwrap_or(""));
                for pat in patterns {
                    if let Some(pat_str) = pat.as_str() {
                        if let Ok(re) = regex::Regex::new(pat_str) {
                            if re.is_match(&full_text) {
                                return Some(RuleResult {
                                    immediate_deny: true,
                                    message: format!("Regex pattern matched: {}", pat_str),
                                    risk_level: crate::models::RiskLevel::Critical,
                                });
                            }
                        }
                    }
                }
            }
            "risk_flag" => {
                let keywords = rule.get("keywords")?.as_array()?;
                let intent_lower = action.intent.to_lowercase();
                for kw in keywords {
                    if let Some(kw_str) = kw.as_str() {
                        if intent_lower.contains(&kw_str.to_lowercase()) {
                            return Some(RuleResult {
                                immediate_deny: false,
                                message: format!("Risk flag: {}", kw_str),
                                risk_level: crate::models::RiskLevel::Medium,
                            });
                        }
                    }
                }
            }
            _ => {}
        }

        None
    }

    async fn save_verdict(
        &self,
        action_id: &str,
        verdict: VerdictResult,
        policy_matched: Option<String>,
    ) -> Result<()> {
        let verdict_id = uuid::Uuid::new_v4().to_string();

        let status = match verdict.decision {
            crate::models::VerdictDecision::Approved => ActionStatus::Approved,
            crate::models::VerdictDecision::Denied => ActionStatus::Denied,
            crate::models::VerdictDecision::Escalate => ActionStatus::Escalated,
        };

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

        sqlx::query("UPDATE actions SET status = ?, updated_at = datetime('now') WHERE id = ?")
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
                "policy_matched": policy_matched,
            }),
        )
        .await?;

        tx.commit().await.map_err(AppError::Database)?;

        if matches!(verdict.decision, crate::models::VerdictDecision::Denied | crate::models::VerdictDecision::Escalate) {
            if let Err(e) = self.webhook.send_verdict(action_id, &verdict).await {
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
}

struct PolicyEvaluation {
    immediate_deny: bool,
    reason: Option<String>,
    risk_level: Option<crate::models::RiskLevel>,
    matched_policy_id: Option<String>,
    context: String,
}

struct RuleResult {
    immediate_deny: bool,
    message: String,
    risk_level: crate::models::RiskLevel,
}
