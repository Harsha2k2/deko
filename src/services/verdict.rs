use crate::db::DbPool;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use tracing::{info, warn};

use crate::config::{Config, LLMProvider};
use crate::error::{AppError, Result};
use crate::models::{ActionStatus, Policy};
use crate::services::llm::{LLMProviderTrait, ProviderMetrics, VerdictResult};
use crate::services::metrics::MetricsCollector;
use crate::services::prompt_injection::{InjectionSeverity, PromptInjectionDetector};
use crate::services::providers::UnifiedProvider;
use crate::services::webhook::WebhookService;
use crate::services::ws_broadcaster::WsBroadcaster;

pub struct VerdictService {
    pub pool: DbPool,
    /// providers in fallback order; primary first, first success wins
    pub providers: Vec<Box<dyn LLMProviderTrait>>,
    pub provider_metrics: Mutex<ProviderMetrics>,
    pub webhook: WebhookService,
    pub metrics: Arc<MetricsCollector>,
    pub ws_broadcaster: Arc<WsBroadcaster>,
}

/// which providers are actually usable given the configured credentials.
fn provider_is_configured(config: &Config, p: &LLMProvider) -> bool {
    match p {
        LLMProvider::OpenAI => config.openai_api_key.is_some(),
        LLMProvider::Gemini => config.gemini_api_key.is_some(),
        LLMProvider::Anthropic => config.anthropic_api_key.is_some(),
        LLMProvider::Ollama => !config.ollama_base_url.is_empty(),
        LLMProvider::Azure => config.azure_api_key.is_some() && !config.azure_endpoint.is_empty(),
        LLMProvider::Bedrock => {
            // a default region string is not evidence of credentials;
            // requiring explicit aws keys avoids surprise fallback attempts
            std::env::var("AWS_ACCESS_KEY_ID").is_ok() && std::env::var("AWS_SECRET_ACCESS_KEY").is_ok()
        }
        LLMProvider::Custom => config.custom_provider_url.is_some(),
    }
}

/// builds the fallback chain: default provider first, then every other
/// configured provider in stable order. the README promises automatic
/// failover; this is where that promise becomes true.
fn build_provider_chain(config: &Config) -> Vec<Box<dyn LLMProviderTrait>> {
    let candidates = [
        LLMProvider::OpenAI,
        LLMProvider::Gemini,
        LLMProvider::Anthropic,
        LLMProvider::Ollama,
        LLMProvider::Azure,
        LLMProvider::Bedrock,
        LLMProvider::Custom,
    ];

    let mut order: Vec<LLMProvider> = vec![config.default_provider];
    for p in candidates {
        if !order.contains(&p) && provider_is_configured(config, &p) {
            order.push(p);
        }
    }

    let shared = Arc::new(config.clone());
    order
        .into_iter()
        .map(|p| Box::new(UnifiedProvider::new(Arc::clone(&shared), p)) as Box<dyn LLMProviderTrait>)
        .collect()
}

impl VerdictService {
    pub fn new(
        pool: DbPool,
        config: &Config,
        metrics: Arc<MetricsCollector>,
        ws_broadcaster: Arc<WsBroadcaster>,
    ) -> Self {
        let webhook = WebhookService::new(config.webhook_url.clone());
        let providers = build_provider_chain(config);

        Self {
            pool,
            providers,
            provider_metrics: Mutex::new(ProviderMetrics::new()),
            webhook,
            metrics,
            ws_broadcaster,
        }
    }

    /// builds a verdict service with explicit providers (tests).
    #[allow(dead_code)]
    pub fn with_providers(
        pool: DbPool,
        config: &Config,
        metrics: Arc<MetricsCollector>,
        ws_broadcaster: Arc<WsBroadcaster>,
        providers: Vec<Box<dyn LLMProviderTrait>>,
    ) -> Self {
        let webhook = WebhookService::new(config.webhook_url.clone());
        Self {
            pool,
            providers,
            provider_metrics: Mutex::new(ProviderMetrics::new()),
            webhook,
            metrics,
            ws_broadcaster,
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
            info!(
                "Action {} already processed (status: {:?}), skipping",
                action_id, action.status
            );
            return Ok(());
        }

        sqlx::query("UPDATE actions SET status = ? WHERE id = ?")
            .bind(ActionStatus::Processing)
            .bind(action_id)
            .execute(&self.pool)
            .await
            .map_err(AppError::Database)?;

        self.audit(
            action_id,
            "processing_started",
            &serde_json::json!({
                "intent": action.intent,
            }),
        )
        .await?;

        let policy_result = self.evaluate_policies(&action).await?;

        if policy_result.immediate_deny {
            let reason = policy_result
                .reason
                .clone()
                .unwrap_or_else(|| "Blocked by policy rule".to_string());
            self.save_verdict(
                action_id,
                &agent_id,
                VerdictResult {
                    decision: crate::models::VerdictDecision::Denied,
                    reason: reason.clone(),
                    risk_level: policy_result.risk_level.unwrap_or(crate::models::RiskLevel::High),
                    raw_response: String::new(),
                    provider: LLMProvider::Gemini,
                    model: "policy_engine".to_string(),
                    confidence: 1.0,
                    reasoning_chain: Some(
                        ["Policy evaluation completed".to_string(),
                            format!(
                                "Matched policy: {}",
                                policy_result.matched_policy_id.as_deref().unwrap_or("unknown")
                            ),
                            format!("Violation: {}", reason)]
                        .join(" → "),
                    ),
                },
                policy_result.matched_policy_id,
            )
            .await?;
            return Ok(());
        }

        let injection_result = PromptInjectionDetector::analyze(&action.intent, action.payload.as_deref());

        if injection_result.detected {
            self.audit(
                action_id,
                "prompt_injection_detected",
                &serde_json::json!({
                    "patterns": injection_result.patterns,
                    "risk_level": injection_result.risk_level,
                }),
            )
            .await?;

            let has_critical = injection_result
                .patterns
                .iter()
                .any(|p| matches!(p.severity, InjectionSeverity::Critical));

            if has_critical {
                let patterns: Vec<String> = injection_result
                    .patterns
                    .iter()
                    .map(|p| format!("{}: {}", p.name, p.match_text))
                    .collect();
                self.save_verdict(
                    action_id,
                    &agent_id,
                    VerdictResult {
                        decision: crate::models::VerdictDecision::Denied,
                        reason: format!("Prompt injection detected: {}", patterns.join("; ")),
                        risk_level: crate::models::RiskLevel::Critical,
                        raw_response: String::new(),
                        provider: LLMProvider::Gemini,
                        model: "prompt_injection_detector".to_string(),
                        confidence: 1.0,
                        reasoning_chain: Some(
                            ["Prompt injection scan initiated".to_string(),
                                format!("Detected {} suspicious pattern(s)", injection_result.patterns.len()),
                                format!("Critical pattern(s) found: {}", patterns.join(", ")),
                                "Immediate deny triggered".to_string()]
                            .join(" → "),
                        ),
                    },
                    None,
                )
                .await?;
                return Ok(());
            }
        }

        let llm_context = match PromptInjectionDetector::injection_context(&injection_result) {
            Some(ctx) => format!("{}\n{}", policy_result.context, ctx),
            None => policy_result.context,
        };

        let verdict_result = self.try_llm_analysis(&action, &llm_context).await;

        self.save_verdict(action_id, &agent_id, verdict_result, policy_result.matched_policy_id)
            .await?;

        Ok(())
    }

    /// Spawn a background task that periodically pings the primary provider.
    /// Marks the service healthy again if it responds.
    pub fn start_health_checks(self: &Arc<Self>, interval_secs: u64) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
            interval.tick().await;
            loop {
                interval.tick().await;
                let is_healthy = svc.provider_metrics.lock().unwrap().is_healthy();
                if !is_healthy {
                    let Some(primary) = svc.providers.first() else {
                        continue;
                    };
                    match primary.health_check().await {
                        Ok(_) => {
                            info!("Provider {} is healthy again", primary.model_name());
                            svc.provider_metrics.lock().unwrap().set_healthy(true);
                        }
                        Err(e) => {
                            warn!("Provider {} health check failed: {}", primary.model_name(), e);
                        }
                    }
                }
            }
        });
    }

    async fn try_llm_analysis(&self, action: &crate::models::Action, policy_context: &str) -> VerdictResult {
        let start = Instant::now();
        self.metrics.inc_llm_call();

        crate::services::audit::record(
            &self.pool,
            Some(&action.id),
            "llm_call_started",
            &serde_json::json!({
                "provider": self.providers.first().map(|p| p.name().to_string()),
                "model": self.providers.first().map(|p| p.model_name()),
                "chain_length": self.providers.len(),
            }),
        )
        .await
        .ok();

        let mut last_error: Option<String> = None;

        for (idx, provider) in self.providers.iter().enumerate() {
            let result = provider
                .analyze_action(
                    &action.intent,
                    action.payload.as_deref(),
                    action.screenshot_base64.as_deref(),
                    policy_context,
                )
                .await;

            match result {
                Ok(mut verdict) => {
                    if idx > 0 {
                        warn!(
                            "Provider {} succeeded on fallback attempt {}/{}",
                            provider.model_name(),
                            idx + 1,
                            self.providers.len()
                        );
                    }
                    let elapsed = start.elapsed().as_millis() as u64;
                    self.metrics.record_llm_latency(elapsed);
                    let tokens_used = estimate_token_count(&action.intent, action.payload.as_deref(), &verdict.reason);
                    self.provider_metrics
                        .lock()
                        .unwrap()
                        .record_request(elapsed as f64, tokens_used);
                    verdict.confidence = verdict.confidence.clamp(0.0, 1.0);
                    return verdict;
                }
                Err(e) => {
                    warn!(
                        "Provider {} failed: {} ({}/{} in chain)",
                        provider.model_name(),
                        e,
                        idx + 1,
                        self.providers.len()
                    );
                    last_error = Some(e.to_string());

                    // audit each failed hop so failover is visible in history
                    crate::services::audit::record(
                        &self.pool,
                        Some(&action.id),
                        "llm_provider_failed",
                        &serde_json::json!({
                            "provider": provider.name().to_string(),
                            "attempt": idx + 1,
                            "error": e.to_string(),
                            "will_retry_with": self.providers.get(idx + 1).map(|p| p.name().to_string()),
                        }),
                    )
                    .await
                    .ok();
                }
            }
        }

        // every provider failed: fail closed
        self.metrics.inc_llm_error();
        self.provider_metrics.lock().unwrap().set_healthy(false);

        VerdictResult {
            decision: crate::models::VerdictDecision::Denied,
            reason: format!(
                "LLM analysis unavailable after trying {} provider(s): {}",
                self.providers.len(),
                last_error.unwrap_or_else(|| "unknown error".to_string())
            ),
            risk_level: crate::models::RiskLevel::High,
            raw_response: String::new(),
            provider: LLMProvider::Custom, // no provider answered; marker value
            model: "none".to_string(),
            confidence: 0.0,
            reasoning_chain: Some("all providers exhausted; fail-closed deny".to_string()),
        }
    }

    pub async fn evaluate_policies(&self, action: &crate::models::Action) -> Result<PolicyEvaluation> {
        // deterministic total order: oldest policy first, id as tiebreaker
        let policies: Vec<Policy> = sqlx::query_as(
            "SELECT id, name, description, rules, active, created_at, updated_at FROM policies \
             WHERE active = 1 \
             AND (activate_at IS NULL OR activate_at <= CURRENT_TIMESTAMP) \
             AND (deactivate_at IS NULL OR deactivate_at > CURRENT_TIMESTAMP) \
             ORDER BY created_at ASC, id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        let is_dry_run = std::env::var("DEKO_POLICY_DRY_RUN").is_ok();

        let mut context_parts = Vec::new();
        let mut had_match = false;

        for policy in &policies {
            let mut rules_value: serde_json::Value = policy.rules.clone();

            let rules: Vec<serde_json::Value> = match rules_value.as_array_mut() {
                Some(arr) => {
                    crate::services::policy_engine::sort_rules(arr);
                    arr.clone()
                }
                None => continue,
            };

            for rule in &rules {
                if let Some(result) = crate::services::policy_engine::evaluate_rule(
                    rule,
                    &crate::services::policy_engine::RuleInput::from_action(action),
                ) {
                    had_match = true;
                    context_parts.push(format!("{}: {}", policy.name, result.message));

                    // Record hit statistic
                    crate::services::audit::record(
                        &self.pool,
                        Some(&action.id),
                        "policy_matched",
                        &serde_json::json!({
                            "policy_id": policy.id,
                            "policy_name": policy.name,
                            "rule_type": rule.get("type"),
                            "message": result.message,
                            "dry_run": is_dry_run,
                        }),
                    )
                    .await
                    .ok();

                    if result.immediate_deny && !is_dry_run {
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

        if had_match && is_dry_run {
            info!(
                "[DRY RUN] Policy would have blocked action {}: {}",
                action.id,
                context_parts.join("; ")
            );
        }

        // Check rate limit policies asynchronously
        for policy in &policies {
            let rules: serde_json::Value = policy.rules.clone();
            if let Some(arr) = rules.as_array() {
                for rule in arr {
                    if rule.get("type").and_then(|t| t.as_str()) == Some("rate_limit") {
                        let max_count = rule.get("max_count").and_then(|v| v.as_i64()).unwrap_or(10);
                        let window_secs = rule.get("window_secs").and_then(|v| v.as_i64()).unwrap_or(60);
                        let cutoff = (chrono::Utc::now() - chrono::Duration::seconds(window_secs)).to_rfc3339();
                        if let Ok((count,)) = sqlx::query_as::<_, (i64,)>(
                            "SELECT COUNT(*) FROM actions WHERE agent_id = ? AND created_at > ? AND status != 'pending'"
                        )
                            .bind(&action.agent_id)
                            .bind(&cutoff)
                            .fetch_one(&self.pool)
                            .await
                        {
                            if count >= max_count && !is_dry_run {
                                return Ok(PolicyEvaluation {
                                    immediate_deny: true,
                                    reason: Some(format!("Rate limit: {} actions in {}s (max {})", count, window_secs, max_count)),
                                    risk_level: Some(crate::models::RiskLevel::Medium),
                                    matched_policy_id: Some(policy.id.clone()),
                                    context: context_parts.join("; "),
                                });
                            }
                        }
                    }
                    if rule.get("type").and_then(|t| t.as_str()) == Some("concurrency_limit") {
                        let max_simultaneous = rule.get("max_simultaneous").and_then(|v| v.as_i64()).unwrap_or(1);
                        if let Ok((count,)) = sqlx::query_as::<_, (i64,)>(
                            "SELECT COUNT(*) FROM actions WHERE agent_id = ? AND status = 'processing'",
                        )
                        .bind(&action.agent_id)
                        .fetch_one(&self.pool)
                        .await
                        {
                            if count >= max_simultaneous && !is_dry_run {
                                return Ok(PolicyEvaluation {
                                    immediate_deny: true,
                                    reason: Some(format!(
                                        "Concurrency limit: {} simultaneous actions (max {})",
                                        count, max_simultaneous
                                    )),
                                    risk_level: Some(crate::models::RiskLevel::Medium),
                                    matched_policy_id: Some(policy.id.clone()),
                                    context: context_parts.join("; "),
                                });
                            }
                        }
                    }
                    if rule.get("type").and_then(|t| t.as_str()) == Some("histogram_trend") {
                        let field = rule.get("field").and_then(|v| v.as_str()).unwrap_or("amount");
                        let stddev_threshold = rule.get("stddev_threshold").and_then(|v| v.as_f64()).unwrap_or(2.0);
                        if let Some(payload_str) = &action.payload {
                            if let Ok(payload_json) = serde_json::from_str::<serde_json::Value>(payload_str) {
                                let current_val = payload_json.get(field).and_then(|v| v.as_f64());
                                if let Some(val) = current_val {
                                    // json path is bound as a parameter, never interpolated
                                    let json_path = format!("$.{}", field);
                                    let query = "SELECT AVG(amount), COUNT(amount), SUM(amount*amount) FROM (SELECT CAST(JSON_EXTRACT(payload, ?) AS REAL) AS amount FROM actions WHERE agent_id = ? AND status != 'pending' AND payload IS NOT NULL) WHERE amount IS NOT NULL";
                                    if let Ok((Some(avg), count, Some(sum_sq))) =
                                        sqlx::query_as::<_, (Option<f64>, i64, Option<f64>)>(query)
                                            .bind(&json_path)
                                            .bind(&action.agent_id)
                                            .fetch_one(&self.pool)
                                            .await
                                    {
                                        let variance = (sum_sq / count as f64) - (avg * avg);
                                        let stddev = variance.sqrt();
                                        let deviation = (val - avg).abs() / stddev;
                                        if count > 5 && stddev > 0.0 && deviation > stddev_threshold && !is_dry_run {
                                            return Ok(PolicyEvaluation {
                                                immediate_deny: true,
                                                reason: Some(format!(
                                                    "Histogram anomaly: {} = {:.2} deviates {:.1}σ from mean {:.2} (threshold: {:.0}σ)",
                                                    field, val, deviation, avg, stddev_threshold
                                                )),
                                                risk_level: Some(crate::models::RiskLevel::High),
                                                matched_policy_id: Some(policy.id.clone()),
                                                context: context_parts.join("; "),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if rule.get("type").and_then(|t| t.as_str()) == Some("budget_limit") {
                        let max_budget = rule.get("max_budget").and_then(|v| v.as_f64()).unwrap_or(10000.0);
                        if let Ok((total,)) = sqlx::query_as::<_, (f64,)>(
                            "SELECT COALESCE(SUM(amount), 0) FROM (SELECT CAST(JSON_EXTRACT(payload, '$.amount') AS REAL) AS amount FROM actions WHERE agent_id = ? AND status != 'denied')",
                        )
                        .bind(&action.agent_id)
                        .fetch_one(&self.pool)
                        .await {
                            if total >= max_budget && !is_dry_run {
                                return Ok(PolicyEvaluation {
                                    immediate_deny: true,
                                    reason: Some(format!("Budget limit: ${:.2} total (max ${:.2})", total, max_budget)),
                                    risk_level: Some(crate::models::RiskLevel::High),
                                    matched_policy_id: Some(policy.id.clone()),
                                    context: context_parts.join("; "),
                                });
                            }
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
            "INSERT INTO verdicts (id, action_id, decision, reason, risk_level, policy_matched, llm_raw_response, reasoning_chain) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&verdict_id)
        .bind(action_id)
        .bind(&verdict.decision)
        .bind(&verdict.reason)
        .bind(&verdict.risk_level)
        .bind(&policy_matched)
        .bind(&verdict.raw_response)
        .bind(&verdict.reasoning_chain)
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
                "reasoning_chain": verdict.reasoning_chain,
            }),
        )
        .await?;

        tx.commit().await.map_err(AppError::Database)?;

        if matches!(
            verdict.decision,
            crate::models::VerdictDecision::Denied | crate::models::VerdictDecision::Escalate
        ) {
            let agent_webhook: Option<(Option<String>,)> =
                sqlx::query_as("SELECT webhook_url FROM agents WHERE id = ?")
                    .bind(agent_id)
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(AppError::Database)?;

            let webhook_url = agent_webhook.and_then(|r| r.0);
            if let Err(e) = self
                .webhook
                .send_verdict(action_id, &verdict, webhook_url.as_deref())
                .await
            {
                warn!("Failed to send webhook for action {}: {}", action_id, e);
            }
        }

        let ws_msg = serde_json::json!({
            "type": "verdict",
            "action_id": action_id,
            "agent_id": agent_id,
            "decision": verdict.decision,
            "reason": verdict.reason,
            "risk_level": verdict.risk_level,
            "provider": verdict.provider,
            "model": verdict.model,
            "confidence": verdict.confidence,
            "policy_matched": policy_matched,
            "reasoning_chain": verdict.reasoning_chain,
        });
        self.ws_broadcaster.send(ws_msg.to_string());

        info!(
            "Action {} verdict: {:?} (risk: {:?}, provider: {}) - {}",
            action_id, verdict.decision, verdict.risk_level, verdict.provider, verdict.reason
        );

        Ok(())
    }

    async fn audit(&self, action_id: &str, event_type: &str, details: &serde_json::Value) -> Result<()> {
        crate::services::audit::record(&self.pool, Some(action_id), event_type, details).await
    }

    async fn audit_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        action_id: &str,
        event_type: &str,
        details: &serde_json::Value,
    ) -> Result<()> {
        crate::services::audit::insert_chained_tx(tx, Some(action_id), event_type, details).await
    }
}

/// Roughly estimate token count from text lengths (~4 chars per token).
fn estimate_token_count(intent: &str, payload: Option<&str>, response: &str) -> u64 {
    let total_chars = intent.len() + payload.unwrap_or("").len() + response.len();
    (total_chars / 4).max(1) as u64
}

pub struct PolicyEvaluation {
    immediate_deny: bool,
    reason: Option<String>,
    risk_level: Option<crate::models::RiskLevel>,
    matched_policy_id: Option<String>,
    context: String,
}

