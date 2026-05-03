use async_trait::async_trait;
use tracing::{error, info};

use crate::services::llm::{LLMProviderTrait, VerdictResult, parse_verdict_json, system_prompt};
use crate::config::LLMProvider;
use crate::error::{AppError, Result};

#[derive(Clone)]
pub struct GeminiProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    timeout_secs: u64,
}

#[derive(serde::Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    generation_config: GenerationConfig,
}

#[derive(serde::Serialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum GeminiPart {
    Text { text: String },
    InlineData { inline_data: InlineData },
}

#[derive(serde::Serialize)]
struct InlineData {
    mime_type: String,
    data: String,
}

#[derive(serde::Serialize)]
struct GenerationConfig {
    temperature: f64,
    max_output_tokens: usize,
}

impl GeminiProvider {
    pub fn new(api_key: String, model: String, timeout_secs: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_key,
            model,
            timeout_secs,
        }
    }

    fn build_url(&self) -> String {
        format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        )
    }
}

#[async_trait]
impl LLMProviderTrait for GeminiProvider {
    fn name(&self) -> LLMProvider {
        LLMProvider::Gemini
    }

    fn model_name(&self) -> String {
        self.model.clone()
    }

    async fn analyze_action(
        &self,
        intent: &str,
        payload: Option<&str>,
        screenshot_base64: Option<&str>,
        policy_context: &str,
    ) -> Result<VerdictResult> {
        let mut user_text = format!("{}\n\n", system_prompt());
        user_text.push_str(&format!("Action Intent: {}\n", intent));
        if let Some(p) = payload {
            user_text.push_str(&format!("Payload: {}\n", p));
        }
        user_text.push_str(&format!("Active Policies: {}\n", policy_context));
        user_text.push_str("Respond in JSON format with keys: decision (approved/denied/escalate), reason (string), risk_level (low/medium/high/critical).");

        let mut parts = vec![GeminiPart::Text { text: user_text }];

        if let Some(img) = screenshot_base64 {
            parts.push(GeminiPart::InlineData {
                inline_data: InlineData {
                    mime_type: "image/png".to_string(),
                    data: img.to_string(),
                },
            });
        }

        let req_body = GeminiRequest {
            contents: vec![GeminiContent {
                role: "user".to_string(),
                parts,
            }],
            generation_config: GenerationConfig {
                temperature: 0.0,
                max_output_tokens: 512,
            },
        };

        info!("Gemini request: model={}, intent_len={}", self.model, intent.len());

        let mut last_err = None;
        for attempt in 0..=2 {
            if attempt > 0 {
                let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt as u32));
                info!("Gemini retry attempt {}/2, waiting {:?}...", attempt, delay);
                tokio::time::sleep(delay).await;
            }

            let response = self
                .client
                .post(self.build_url())
                .header("Content-Type", "application/json")
                .json(&req_body)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        error!("Gemini API error (attempt {}): {} - {}", attempt + 1, status, body);
                        last_err = Some(AppError::OpenAI(format!("Gemini returned {}: {}", status, body)));
                        continue;
                    }

                    let parsed: GeminiResponse = resp.json().await.map_err(|e| {
                        AppError::OpenAI(format!("Failed to parse Gemini response: {}", e))
                    })?;

                    let content = parsed
                        .candidates
                        .first()
                        .and_then(|c| c.content.parts.first())
                        .and_then(|p| p.text.clone())
                        .unwrap_or_default();

                    info!("Gemini response received on attempt {}", attempt + 1);
                    return parse_verdict_json(&content, LLMProvider::Gemini, self.model.clone());
                }
                Err(e) => {
                    error!("Gemini request failed (attempt {}): {}", attempt + 1, e);
                    last_err = Some(AppError::OpenAI(format!("Gemini request failed: {}", e)));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| AppError::OpenAI("Gemini exhausted all retries".into())))
    }
}

#[derive(serde::Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(serde::Deserialize)]
struct GeminiCandidate {
    content: GeminiResponseContent,
}

#[derive(serde::Deserialize)]
struct GeminiResponseContent {
    parts: Vec<GeminiResponsePart>,
}

#[derive(serde::Deserialize)]
struct GeminiResponsePart {
    text: Option<String>,
}
