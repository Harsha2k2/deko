//! unified policy rule evaluator.
//!
//! one implementation serves every path that evaluates rules: live decisions,
//! dry-run mode, the /admin/policies/test endpoint, and simulation. before
//! this module existed those paths drifted apart, so a policy could behave
//! differently in testing than in production.
//!
//! guarantees:
//! - deterministic: rules are evaluated sorted by their optional `priority`
//!   field (stable for ties), then in declared order
//! - fail-closed: an unrecognized rule type denies the action instead of
//!   passing silently
//! - pure: no database, no clock-dependent behavior except `time_window`,
//!   which is documented as time-based by design

use crate::models::RiskLevel;
use chrono::{Datelike, Timelike};

/// everything a rule may inspect about the action under evaluation.
#[derive(Debug, Clone)]
pub struct RuleInput<'a> {
    pub intent: &'a str,
    pub payload: Option<&'a str>,
    pub target_url: Option<&'a str>,
    pub target_method: Option<&'a str>,
    pub metadata: Option<&'a str>,
}

impl<'a> RuleInput<'a> {
    pub fn from_action(action: &'a crate::models::Action) -> Self {
        Self {
            intent: &action.intent,
            payload: action.payload.as_deref(),
            target_url: action.target_url.as_deref(),
            target_method: action.target_method.as_deref(),
            metadata: action.metadata.as_deref(),
        }
    }
}

/// what a matched rule decided.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleOutcome {
    pub immediate_deny: bool,
    pub message: String,
    pub risk_level: RiskLevel,
}

/// rule types whose checks need database context; handled by the async
/// caller, never flagged as unknown here.
pub const ASYNC_RULE_TYPES: &[&str] = &[
    "rate_limit",
    "concurrency_limit",
    "budget_limit",
    "histogram_trend",
    "trend_anomaly",
];

fn outcome(deny: bool, level: RiskLevel, message: String) -> Option<RuleOutcome> {
    Some(RuleOutcome {
        immediate_deny: deny,
        message,
        risk_level: level,
    })
}

/// stable-sorts rules by their optional numeric `priority` field.
/// lower numbers evaluate first; missing priority sorts as 0.
pub fn sort_rules(rules: &mut [serde_json::Value]) {
    rules.sort_by_key(|r| r.get("priority").and_then(|v| v.as_i64()).unwrap_or(0));
}

/// evaluates a single rule. returns None when the rule does not match.
///
/// composite `and`/`or` rules recurse into their sub-rules.
pub fn evaluate_rule(rule: &serde_json::Value, input: &RuleInput) -> Option<RuleOutcome> {
    let rule_type = rule.get("type")?.as_str()?;

    match rule_type {
        "and" | "or" => evaluate_composite(rule, input, rule_type),
        _ => evaluate_simple(rule, input, rule_type),
    }
}

fn merge_outcomes(results: Vec<RuleOutcome>) -> Option<RuleOutcome> {
    if results.is_empty() {
        return None;
    }
    let risk_level = results
        .iter()
        .map(|r| r.risk_level.clone())
        .max_by_key(|lvl| match lvl {
            RiskLevel::Critical => 3,
            RiskLevel::High => 2,
            RiskLevel::Medium => 1,
            RiskLevel::Low => 0,
        })
        .unwrap_or(RiskLevel::Low);
    Some(RuleOutcome {
        immediate_deny: results.iter().any(|r| r.immediate_deny),
        message: results
            .iter()
            .map(|r| r.message.as_str())
            .collect::<Vec<_>>()
            .join("; "),
        risk_level,
    })
}

fn evaluate_composite(rule: &serde_json::Value, input: &RuleInput, operator: &str) -> Option<RuleOutcome> {
    let sub_rules = rule.get("rules")?.as_array()?;
    let is_and = operator == "and";

    let mut collected: Vec<RuleOutcome> = Vec::new();
    for sub in sub_rules {
        if let Some(result) = evaluate_rule(sub, input) {
            if !is_and {
                return Some(result); // short-circuit on first or-match
            }
            collected.push(result);
        } else if is_and {
            return None; // and requires every sub-rule to match
        }
    }

    if is_and {
        merge_outcomes(collected)
    } else {
        None
    }
}

fn evaluate_simple(rule: &serde_json::Value, input: &RuleInput, rule_type: &str) -> Option<RuleOutcome> {
    match rule_type {
        "deny_keyword" => {
            let keywords = rule.get("keywords")?.as_array()?;
            let intent_lower = input.intent.to_lowercase();
            for kw in keywords {
                if let Some(kw_str) = kw.as_str() {
                    if intent_lower.contains(&kw_str.to_lowercase()) {
                        return outcome(true, RiskLevel::Critical, format!("Denied keyword match: {}", kw_str));
                    }
                }
            }
            None
        }
        "require_approval" => {
            let action_types = rule.get("action_types")?.as_array()?;
            if let Some(method) = input.target_method {
                for at in action_types {
                    if let Some(at_str) = at.as_str() {
                        if method.to_uppercase() == at_str.to_uppercase() {
                            return outcome(
                                false,
                                RiskLevel::High,
                                format!("Requires human approval for {} actions", at_str),
                            );
                        }
                    }
                }
            }
            None
        }
        "max_amount" => {
            let max = rule.get("max")?.as_f64()?;
            let payload_str = input.payload?;
            let payload_json: serde_json::Value = serde_json::from_str(payload_str).ok()?;
            let amount = payload_json.get("amount").and_then(|v| v.as_f64())?;
            if amount > max {
                return outcome(
                    true,
                    RiskLevel::High,
                    format!("Amount {} exceeds maximum {}", amount, max),
                );
            }
            None
        }
        "regex_deny" => {
            let patterns = rule.get("patterns")?.as_array()?;
            let full_text = format!("{} {}", input.intent, input.payload.unwrap_or(""));
            for pat in patterns {
                if let Some(pat_str) = pat.as_str() {
                    if let Ok(re) = regex::Regex::new(pat_str) {
                        if re.is_match(&full_text) {
                            return outcome(true, RiskLevel::Critical, format!("Regex pattern matched: {}", pat_str));
                        }
                    }
                }
            }
            None
        }
        "risk_flag" => {
            let keywords = rule.get("keywords")?.as_array()?;
            let intent_lower = input.intent.to_lowercase();
            for kw in keywords {
                if let Some(kw_str) = kw.as_str() {
                    if intent_lower.contains(&kw_str.to_lowercase()) {
                        return outcome(false, RiskLevel::Medium, format!("Risk flag: {}", kw_str));
                    }
                }
            }
            None
        }
        "url_allowlist" => {
            // only meaningful when a target url exists; without one there is
            // nothing to allowlist against
            let allowed = rule.get("patterns")?.as_array()?;
            let url = input.target_url?;
            let is_allowed = allowed.iter().any(|p| p.as_str().is_some_and(|pat| url.contains(pat)));
            if !is_allowed {
                return outcome(true, RiskLevel::High, format!("URL not in allowlist: {}", url));
            }
            None
        }
        "url_blocklist" => {
            let blocked = rule.get("patterns")?.as_array()?;
            let url = input.target_url?;
            for pat in blocked {
                if let Some(pat_str) = pat.as_str() {
                    if url.contains(pat_str) {
                        return outcome(true, RiskLevel::Critical, format!("URL matches blocklist: {}", pat_str));
                    }
                }
            }
            None
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
                return outcome(
                    true,
                    RiskLevel::Medium,
                    format!(
                        "Action outside allowed time window (UTC {}-{}, allowed days: {:?})",
                        start, end, allowed_days
                    ),
                );
            }
            None
        }
        "ip_allowlist" => {
            let meta = input.metadata?;
            let parsed: serde_json::Value = serde_json::from_str(meta).ok()?;
            let ip = parsed.get("source_ip").and_then(|v| v.as_str())?;
            let allowed = rule.get("patterns")?.as_array()?;
            let is_allowed = allowed.iter().any(|p| p.as_str().is_some_and(|pat| ip.contains(pat)));
            if !is_allowed {
                return outcome(true, RiskLevel::High, format!("Source IP {} not in allowlist", ip));
            }
            None
        }
        "geofence" => {
            let meta = input.metadata?;
            let parsed: serde_json::Value = serde_json::from_str(meta).ok()?;
            let country = parsed.get("country").and_then(|v| v.as_str())?;
            let blocked = rule.get("blocked_countries")?.as_array()?;
            if blocked
                .iter()
                .any(|b| b.as_str().map_or(false, |b| b.eq_ignore_ascii_case(country)))
            {
                return outcome(
                    true,
                    RiskLevel::High,
                    format!("Country {} is blocked by geofence policy", country),
                );
            }
            None
        }
        other if ASYNC_RULE_TYPES.contains(&other) => {
            // evaluated asynchronously by the caller with database context
            None
        }
        other => {
            // fail-closed: an unrecognized rule type must never pass silently.
            // a typo like "deny_keywords" should stop traffic, not ignore it.
            outcome(
                true,
                RiskLevel::High,
                format!(
                    "Policy contains unknown rule type '{}' (fail-closed deny); \
                     review the policy definition",
                    other
                ),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input(intent: &str) -> RuleInput<'_> {
        RuleInput {
            intent,
            payload: None,
            target_url: None,
            target_method: Some("POST"),
            metadata: None,
        }
    }

    #[test]
    fn deny_keyword_matches_case_insensitive() {
        let rule = json!({"type": "deny_keyword", "keywords": ["DROP TABLE"]});
        assert!(evaluate_rule(&rule, &input("please DROP TABLE users")).is_some());
        assert!(evaluate_rule(&rule, &input("select 1")).is_none());
    }

    #[test]
    fn max_amount_enforces_threshold() {
        let rule = json!({"type": "max_amount", "max": 100.0});
        let rich = RuleInput {
            intent: "pay",
            payload: Some(r#"{"amount": 500}"#),
            target_url: None,
            target_method: None,
            metadata: None,
        };
        let small = RuleInput {
            intent: "pay",
            payload: Some(r#"{"amount": 50}"#),
            target_url: None,
            target_method: None,
            metadata: None,
        };
        assert!(evaluate_rule(&rule, &rich).unwrap().immediate_deny);
        assert!(evaluate_rule(&rule, &small).is_none());
    }

    #[test]
    fn unknown_rule_type_fails_closed() {
        let rule = json!({"type": "totally_made_up"});
        let result = evaluate_rule(&rule, &input("anything"));
        assert!(result.is_some(), "unknown type must match");
        assert!(result.unwrap().immediate_deny, "unknown type must deny");
    }

    #[test]
    fn async_rule_types_are_not_flagged_unknown() {
        let rule = json!({"type": "rate_limit", "max_count": 5});
        assert!(evaluate_rule(&rule, &input("x")).is_none());
    }

    #[test]
    fn sort_rules_orders_by_priority_stably() {
        let mut rules = vec![
            json!({"type": "a", "priority": 10}),
            json!({"type": "b"}),
            json!({"type": "c", "priority": 1}),
            json!({"type": "d", "priority": 1}),
        ];
        sort_rules(&mut rules);
        let types: Vec<&str> = rules.iter().map(|r| r["type"].as_str().unwrap()).collect();
        assert_eq!(
            types,
            vec!["b", "c", "d", "a"],
            "missing priority = 0, stable within tie"
        );
    }

    #[test]
    fn and_composite_requires_all_submatches() {
        let both = json!({"type": "and", "rules": [
            {"type": "deny_keyword", "keywords": ["delete"]},
            {"type": "risk_flag", "keywords": ["prod"]}
        ]});
        assert!(evaluate_rule(&both, &input("delete prod data")).is_some());
        assert!(evaluate_rule(&both, &input("delete staging data")).is_none());

        let either = json!({"type": "or", "rules": [
            {"type": "deny_keyword", "keywords": ["delete"]},
            {"type": "risk_flag", "keywords": ["prod"]}
        ]});
        assert!(evaluate_rule(&either, &input("touch staging data")).is_none());
        assert!(evaluate_rule(&either, &input("touch prod data")).is_some());
    }

    #[test]
    fn url_blocklist_and_allowlist_semantics() {
        let block = json!({"type": "url_blocklist", "patterns": ["evil.com"]});
        let hit = RuleInput {
            intent: "x",
            payload: None,
            target_url: Some("https://evil.com/x"),
            target_method: None,
            metadata: None,
        };
        let miss = RuleInput {
            intent: "x",
            payload: None,
            target_url: Some("https://ok.com/x"),
            target_method: None,
            metadata: None,
        };
        assert!(evaluate_rule(&block, &hit).unwrap().immediate_deny);
        assert!(evaluate_rule(&block, &miss).is_none());

        let allow = json!({"type": "url_allowlist", "patterns": ["ok.com"]});
        assert!(evaluate_rule(&allow, &miss).is_none());
        assert!(evaluate_rule(&allow, &hit).unwrap().immediate_deny);
    }
}
