use crate::db::DbPool;
use chrono::{Datelike, Timelike};
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
    pub provider: Box<dyn LLMProviderTrait>,
    pub provider_metrics: Mutex<ProviderMetrics>,
    pub webhook: WebhookService,
    pub metrics: Arc<MetricsCollector>,
    pub ws_broadcaster: Arc<WsBroadcaster>,
}

impl VerdictService {
    pub fn new(
        pool: DbPool,
        config: &Config,
        metrics: Arc<MetricsCollector>,
        ws_broadcaster: Arc<WsBroadcaster>,
    ) -> Self {
        let webhook = WebhookService::new(config.webhook_url.clone());
        let config = Arc::new(config.clone());
        let provider = Box::new(UnifiedProvider::new(Arc::clone(&config), config.default_provider));

        Self {
            pool: pool.clone(),
            provider,
            provider_metrics: Mutex::new(ProviderMetrics::new()),
            webhook,
            metrics,
            ws_broadcaster,
        }
    }

    /// builds a verdict service with an explicit provider (used by tests and
    /// future fallback chains to control which provider handles analysis).
    #[allow(dead_code)]
    pub fn with_provider(
        pool: DbPool,
        config: &Config,
        metrics: Arc<MetricsCollector>,
        ws_broadcaster: Arc<WsBroadcaster>,
        provider: Box<dyn LLMProviderTrait>,
    ) -> Self {
        let webhook = WebhookService::new(config.webhook_url.clone());
        Self {
            pool,
            provider,
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
                        vec![
                            "Policy evaluation completed".to_string(),
                            format!(
                                "Matched policy: {}",
                                policy_result.matched_policy_id.as_deref().unwrap_or("unknown")
                            ),
                            format!("Violation: {}", reason),
                        ]
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
                            vec![
                                "Prompt injection scan initiated".to_string(),
                                format!("Detected {} suspicious pattern(s)", injection_result.patterns.len()),
                                format!("Critical pattern(s) found: {}", patterns.join(", ")),
                                "Immediate deny triggered".to_string(),
                            ]
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

    /// Spawn a background task that periodically pings the provider.
    /// Marks it healthy again if it responds.
    pub fn start_health_checks(self: &Arc<Self>, interval_secs: u64) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
            interval.tick().await;
            loop {
                interval.tick().await;
                let is_healthy = svc.provider_metrics.lock().unwrap().is_healthy();
                if !is_healthy {
                    match svc.provider.health_check().await {
                        Ok(_) => {
                            info!("Provider {} is healthy again", svc.provider.model_name());
                            svc.provider_metrics.lock().unwrap().set_healthy(true);
                        }
                        Err(e) => {
                            warn!("Provider {} health check failed: {}", svc.provider.model_name(), e);
                        }
                    }
                }
            }
        });
    }

    async fn try_llm_analysis(&self, action: &crate::models::Action, policy_context: &str) -> VerdictResult {
        let start = Instant::now();
        self.metrics.inc_llm_call();

        let provider_name = self.provider.name();
        let model_name = self.provider.model_name();

        crate::services::audit::record(
            &self.pool,
            Some(&action.id),
            "llm_call_started",
            &serde_json::json!({
                "provider": provider_name,
                "model": model_name,
            }),
        )
        .await
        .ok();

        let result = self
            .provider
            .analyze_action(
                &action.intent,
                action.payload.as_deref(),
                action.screenshot_base64.as_deref(),
                policy_context,
            )
            .await;

        let elapsed = start.elapsed().as_millis() as u64;
        self.metrics.record_llm_latency(elapsed);

        match result {
            Ok(mut verdict) => {
                let tokens_used = estimate_token_count(&action.intent, action.payload.as_deref(), &verdict.reason);
                self.provider_metrics
                    .lock()
                    .unwrap()
                    .record_request(elapsed as f64, tokens_used);
                verdict.confidence = verdict.confidence.max(0.0).min(1.0);
                verdict
            }
            Err(e) => {
                warn!("Provider {} failed: {}", self.provider.model_name(), e);
                self.metrics.inc_llm_error();
                self.provider_metrics.lock().unwrap().set_healthy(false);

                VerdictResult {
                    decision: crate::models::VerdictDecision::Denied,
                    reason: format!("LLM provider failed: {}", e),
                    risk_level: crate::models::RiskLevel::High,
                    raw_response: String::new(),
                    provider: provider_name,
                    model: model_name,
                    confidence: 0.0,
                    reasoning_chain: Some("LLM provider failed; fail-closed deny".to_string()),
                }
            }
        }
    }

    pub async fn evaluate_policies(&self, action: &crate::models::Action) -> Result<PolicyEvaluation> {
        let policies: Vec<Policy> = sqlx::query_as(
            "SELECT id, name, description, rules, active, created_at, updated_at FROM policies \
             WHERE active = 1 \
             AND (activate_at IS NULL OR activate_at <= CURRENT_TIMESTAMP) \
             AND (deactivate_at IS NULL OR deactivate_at > CURRENT_TIMESTAMP)",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::Database)?;

        let is_dry_run = std::env::var("DEKO_POLICY_DRY_RUN").is_ok();

        let mut context_parts = Vec::new();
        let mut had_match = false;

        for policy in &policies {
            let rules: serde_json::Value = policy.rules.clone();

            if let Some(arr) = rules.as_array() {
                for rule in arr {
                    if let Some(result) = self.evaluate_rule(rule, action) {
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
                                    if let Ok(row) = sqlx::query_as::<_, (Option<f64>, i64, Option<f64>)>(query)
                                        .bind(&json_path)
                                        .bind(&action.agent_id)
                                        .fetch_one(&self.pool)
                                        .await
                                    {
                                        if let (Some(avg), count, Some(sum_sq)) = row {
                                            if count > 5 {
                                                let variance = (sum_sq / count as f64) - (avg * avg);
                                                let stddev = variance.sqrt();
                                                if stddev > 0.0 {
                                                    let deviation = (val - avg).abs() / stddev;
                                                    if deviation > stddev_threshold && !is_dry_run {
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

    fn evaluate_rule(&self, rule: &serde_json::Value, action: &crate::models::Action) -> Option<RuleResult> {
        let rule_type = rule.get("type")?.as_str()?;

        let priority = rule.get("priority").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

        match rule_type {
            "and" | "or" => self.evaluate_composite_rule(rule, action, rule_type, priority),
            _ => self.evaluate_simple_rule(rule, action, rule_type, priority),
        }
    }

    fn evaluate_composite_rule(
        &self,
        rule: &serde_json::Value,
        action: &crate::models::Action,
        operator: &str,
        _priority: i32,
    ) -> Option<RuleResult> {
        let rules = rule.get("rules")?.as_array()?;
        let is_and = operator == "and";

        let mut results: Vec<RuleResult> = Vec::new();
        for sub_rule in rules {
            if let Some(result) = self.evaluate_rule(sub_rule, action) {
                if is_and {
                    results.push(result);
                } else {
                    return Some(result);
                }
            } else if is_and {
                return None;
            }
        }

        if is_and && !results.is_empty() {
            Some(RuleResult {
                immediate_deny: results.iter().any(|r| r.immediate_deny),
                message: results
                    .iter()
                    .map(|r| r.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
                risk_level: if results
                    .iter()
                    .any(|r| r.risk_level == crate::models::RiskLevel::Critical)
                {
                    crate::models::RiskLevel::Critical
                } else if results.iter().any(|r| r.risk_level == crate::models::RiskLevel::High) {
                    crate::models::RiskLevel::High
                } else if results.iter().any(|r| r.risk_level == crate::models::RiskLevel::Medium) {
                    crate::models::RiskLevel::Medium
                } else {
                    crate::models::RiskLevel::Low
                },
            })
        } else {
            None
        }
    }

    fn evaluate_simple_rule(
        &self,
        rule: &serde_json::Value,
        action: &crate::models::Action,
        rule_type: &str,
        _priority: i32,
    ) -> Option<RuleResult> {
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
            "url_allowlist" => {
                let allowed = rule.get("patterns")?.as_array()?;
                if let Some(url) = &action.target_url {
                    let is_allowed = allowed.iter().any(|p| p.as_str().is_some_and(|pat| url.contains(pat)));
                    if !is_allowed {
                        return Some(RuleResult {
                            immediate_deny: true,
                            message: format!("URL not in allowlist: {}", url),
                            risk_level: crate::models::RiskLevel::High,
                        });
                    }
                }
            }
            "url_blocklist" => {
                let blocked = rule.get("patterns")?.as_array()?;
                if let Some(url) = &action.target_url {
                    for pat in blocked {
                        if let Some(pat_str) = pat.as_str() {
                            if url.contains(pat_str) {
                                return Some(RuleResult {
                                    immediate_deny: true,
                                    message: format!("URL matches blocklist: {}", pat_str),
                                    risk_level: crate::models::RiskLevel::Critical,
                                });
                            }
                        }
                    }
                }
            }
            "time_window" => {
                let now = chrono::Utc::now();
                let start = rule.get("start_hour_utc").and_then(|v| v.as_i64()).unwrap_or(0);
                let end = rule.get("end_hour_utc").and_then(|v| v.as_i64()).unwrap_or(24);
                let hour = now.hour() as i64;
                let allowed_days = rule.get("days").and_then(|v| v.as_array()).map(|days| {
                    days.iter()
                        .filter_map(|d| d.as_i64().map(|d| d as u32))
                        .collect::<Vec<_>>()
                });
                let day_ok = match allowed_days {
                    Some(ref days) => days.contains(&now.weekday().num_days_from_monday()),
                    None => true,
                };
                if !day_ok || hour < start || hour >= end {
                    return Some(RuleResult {
                        immediate_deny: true,
                        message: format!(
                            "Action outside allowed time window (UTC {}-{}, allowed days: {:?})",
                            start, end, allowed_days
                        ),
                        risk_level: crate::models::RiskLevel::Medium,
                    });
                }
            }
            "ip_allowlist" => {
                if let Some(meta) = &action.metadata {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(meta) {
                        let source_ip = parsed.get("source_ip").and_then(|v| v.as_str());
                        let allowed = rule.get("patterns")?.as_array()?;
                        if let Some(ip) = source_ip {
                            let is_allowed = allowed.iter().any(|p| p.as_str().is_some_and(|pat| ip.contains(pat)));
                            if !is_allowed {
                                return Some(RuleResult {
                                    immediate_deny: true,
                                    message: format!("Source IP {} not in allowlist", ip),
                                    risk_level: crate::models::RiskLevel::High,
                                });
                            }
                        }
                    }
                }
            }
            "concurrency_limit" | "budget_limit" | "trend_anomaly" => {
                // Checked in evaluate_policies (async context needed)
            }
            "geofence" => {
                if let Some(meta) = &action.metadata {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(meta) {
                        let country = parsed.get("country").and_then(|v| v.as_str());
                        let blocked = rule.get("blocked_countries")?.as_array()?;
                        if let Some(c) = country {
                            if blocked
                                .iter()
                                .any(|b| b.as_str().map_or(false, |b| b.eq_ignore_ascii_case(c)))
                            {
                                return Some(RuleResult {
                                    immediate_deny: true,
                                    message: format!("Country {} is blocked by geofence policy", c),
                                    risk_level: crate::models::RiskLevel::High,
                                });
                            }
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

pub struct RuleResult {
    immediate_deny: bool,
    message: String,
    risk_level: crate::models::RiskLevel,
}
