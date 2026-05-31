use async_trait::async_trait;
use tracing::{error, info};

use crate::services::llm::{LLMProviderTrait, VerdictResult, parse_verdict_json, system_prompt};
use crate::config::LLMProvider;
use crate::error::{AppError, Result};

#[derive(Clone)]
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

#[derive(serde::Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: usize,
    system: String,
    messages: Vec<AnthropicMessage>,
}

#[derive(serde::Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContent>,
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum AnthropicContent {
    Text { text: String },
    Image {
        source: ImageSource,
        #[serde(rename = "type")]
        content_type: String,
    },
}

#[derive(serde::Serialize)]
struct ImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(serde::Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicResponseContent>,
}

#[derive(serde::Deserialize)]
struct AnthropicResponseContent {
    text: Option<String>,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String, timeout_secs: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, api_key, model }
    }

    fn build_request(&self, intent: &str, payload: Option<&str>, screenshot_base64: Option<&str>, policy_context: &str) -> AnthropicRequest {
        let user_message = if let Some(screenshot) = screenshot_base64 {
            let mut content = Vec::new();
            content.push(AnthropicContent::Text {
                text: format!(
                    "Action Intent: {}\n\nPayload: {}\n\nPolicy Context: {}\n\nAnalyze this action and determine if it should be approved, denied, or escalated.",
                    intent,
                    payload.unwrap_or("(no payload)"),
                    policy_context
                ),
            });
            content.push(AnthropicContent::Image {
                source: ImageSource {
                    source_type: "base64".to_string(),
                    media_type: "image/png".to_string(),
                    data: screenshot.to_string(),
                },
                content_type: "image".to_string(),
            });
            AnthropicMessage {
                role: "user".to_string(),
                content,
            }
        } else {
            AnthropicMessage {
                role: "user".to_string(),
                content: vec![AnthropicContent::Text {
                    text: format!(
                        "Action Intent: {}\n\nPayload: {}\n\nPolicy Context: {}\n\nAnalyze this action and determine if it should be approved, denied, or escalated.",
                        intent,
                        payload.unwrap_or("(no payload)"),
                        policy_context
                    ),
                }],
            }
        };

        AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 1024,
            system: system_prompt().to_string(),
            messages: vec![user_message],
        }
    }
}

#[async_trait]
impl LLMProviderTrait for AnthropicProvider {
    fn name(&self) -> LLMProvider {
        LLMProvider::Anthropic
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
        let request = self.build_request(intent, payload, screenshot_base64, policy_context);

        info!(model = %self.model, "Calling Anthropic Claude API");

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                error!("Anthropic API request failed: {}", e);
                AppError::OpenAI(format!("Anthropic request failed: {}", e))
            })?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            error!("Anthropic API error {}: {}", status, body);
            return Err(AppError::OpenAI(format!("Anthropic API error {}: {}", status, body)));
        }

        let parsed: AnthropicResponse = serde_json::from_str(&body).map_err(|e| {
            error!("Failed to parse Anthropic response: {}", e);
            AppError::OpenAI(format!("Failed to parse Anthropic response: {}", e))
        })?;

        let text = parsed.content
            .first()
            .and_then(|c| c.text.as_deref())
            .unwrap_or("");

        parse_verdict_json(text, LLMProvider::Anthropic, self.model.clone())
    }
}
