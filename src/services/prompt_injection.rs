use crate::models::RiskLevel;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct InjectionResult {
    pub detected: bool,
    pub patterns: Vec<MatchedPattern>,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Serialize)]
pub struct MatchedPattern {
    pub name: &'static str,
    pub severity: InjectionSeverity,
    pub match_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum InjectionSeverity {
    Low,
    Medium,
    High,
    Critical,
}

static INJECTION_PATTERNS: &[(&str, &str, InjectionSeverity)] = &[
    ("instruction_override", r"(?i)ignore\s+(all\s+)?(previous|above|prior)\s+(instructions|directives|commands|rules|prompts)", InjectionSeverity::Critical),
    ("instruction_forget", r"(?i)forget\s+(all\s+)?(previous|above|prior)\s+(instructions|directives|commands|rules|prompts)", InjectionSeverity::Critical),
    ("instruction_disregard", r"(?i)disregard\s+(all\s+)?(previous|above|prior)\s+(instructions|directives|commands|rules|prompts)", InjectionSeverity::Critical),
    ("role_switch", r"(?i)you\s+are\s+now\s+", InjectionSeverity::Critical),
    ("role_pretend", r"(?i)pretend\s+(you\s+are|to\s+be)\s+", InjectionSeverity::Critical),
    ("system_prompt_leak", r"(?i)(reveal|show|print|output|display|dump)\s+(your\s+)?(system|instructions|prompt|rules|directives)", InjectionSeverity::Critical),
    ("repeat_after", r"(?i)(repeat|say|write)\s+(after\s+me|the\s+words|exactly\s+this)", InjectionSeverity::High),
    ("ignore_above", r"(?i)ignore\s+the\s+above", InjectionSeverity::High),
    ("delimiter_confusion", r"(?i)(<\|im_start\|>|<\|im_end\|>|<\|sys\|>|<\|user\|>|<\|assistant\|>)", InjectionSeverity::Critical),
    ("tool_injection", r"(?i)use\s+(tool|function)\s+\w+\s+(to|with|on)", InjectionSeverity::High),
    ("data_exfiltration", r"(?i)(exfiltrate|exfiltrat|leak|steal)\s+(data|info|information|records|customers?|users?)", InjectionSeverity::Critical),
    ("mass_export", r"(?i)(export|download|copy|transfer|send)\s+(all|every|entire|mass)", InjectionSeverity::High),
    ("instruction_redirect", r"(?i)(now|instead),?\s+(say|respond|output|answer|write)", InjectionSeverity::High),
    ("output_control", r"(?i)output\s+\d+\s*(tokens?|words?|chars?)", InjectionSeverity::Medium),
    ("hidden_text", r"(?i)(<div[^>]*style=[^>]*display:\s*none|<span[^>]*style=[^>]*display:\s*none|<!--.*-->)", InjectionSeverity::Critical),
    ("base64_injection", r"(?i)[A-Za-z0-9+/]{40,}={0,2}", InjectionSeverity::Medium),
];

pub struct PromptInjectionDetector;

impl PromptInjectionDetector {
    pub fn analyze(intent: &str, payload: Option<&str>) -> InjectionResult {
        let full_text = format!("{} {}", intent, payload.unwrap_or(""));

        let mut matched = Vec::new();
        let mut max_severity = InjectionSeverity::Low;

        for (name, pattern, severity) in INJECTION_PATTERNS {
            if let Ok(re) = regex::Regex::new(pattern) {
                for cap in re.find_iter(&full_text) {
                    matched.push(MatchedPattern {
                        name,
                        severity: severity.clone(),
                        match_text: cap.as_str().to_string(),
                    });
                    max_severity = max_severity.max(severity.clone());
                }
            }
        }

        let risk_level = match max_severity {
            InjectionSeverity::Critical => RiskLevel::Critical,
            InjectionSeverity::High => RiskLevel::High,
            InjectionSeverity::Medium => RiskLevel::Medium,
            InjectionSeverity::Low => RiskLevel::Low,
        };

        InjectionResult {
            detected: !matched.is_empty(),
            patterns: matched,
            risk_level,
        }
    }

    pub fn injection_context(result: &InjectionResult) -> Option<String> {
        if !result.detected {
            return None;
        }
        let patterns: Vec<String> = result.patterns.iter()
            .map(|p| format!("{} ({}: {:?})", p.name, p.match_text, p.severity))
            .collect();
        Some(format!(
            "PROMPT INJECTION DETECTED — patterns matched: {} | risk: {:?}",
            patterns.join(", "),
            result.risk_level
        ))
    }
}
