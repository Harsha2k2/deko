use async_trait::async_trait;
use tracing::{error, info};

use crate::services::llm::{LLMProviderTrait, VerdictResult, parse_verdict_json, system_prompt};
use crate::config::LLMProvider;
use crate::error::{AppError, Result};

#[derive(Clone)]
pub struct OpenAIProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    timeout_secs: u64,
}

#[derive(serde::Serialize)]
struct VisionRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: usize,
    temperature: f64,
    response_format: ResponseFormat,
}

#[derive(serde::Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

#[derive(serde::Serialize)]
struct Message {
    role: String,
    content: Vec<ContentPart>,
}

#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

#[derive(serde::Serialize)]
struct ImageUrl {
    url: String,
}

#[derive(serde::Deserialize)]
struct VisionResponse {
    choices: Vec<Choice>,
}

#[derive(serde::Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(serde::Deserialize)]
struct ResponseMessage {
    content: String,
}

impl OpenAIProvider {
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
}

#[async_trait]
impl LLMProviderTrait for OpenAIProvider {
    fn name(&self) -> LLMProvider {
        LLMProvider::OpenAI
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
        let mut messages = vec![Message {
            role: "system".to_string(),
            content: vec![ContentPart::Text {
                text: system_prompt().to_string(),
            }],
        }];

        let mut user_text = format!("Action Intent: {}\n", intent);
        if let Some(p) = payload {
            user_text.push_str(&format!("Payload: {}\n", p));
        }
        user_text.push_str(&format!("Active Policies: {}\n", policy_context));
        user_text.push_str("Respond in JSON format with keys: decision (approved/denied/escalate), reason (string), risk_level (low/medium/high/critical).");

        let mut content_parts = vec![ContentPart::Text { text: user_text }];

        if let Some(img) = screenshot_base64 {
            content_parts.push(ContentPart::ImageUrl {
                image_url: ImageUrl {
                    url: format!("data:image/png;base64,{}", img),
                },
            });
        }

        messages.push(Message {
            role: "user".to_string(),
            content: content_parts,
        });

        let req_body = VisionRequest {
            model: self.model.clone(),
            messages,
            max_tokens: 512,
            temperature: 0.0,
            response_format: ResponseFormat {
                format_type: "json_object".to_string(),
            },
        };

        info!("OpenAI request: model={}, intent_len={}", self.model, intent.len());

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await;

        match response {
            Ok(resp) => {
                let status = resp.status();
                if !status.is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    error!("OpenAI API error: {} - {}", status, body);
                    return Err(AppError::OpenAI(format!("OpenAI returned {}: {}", status, body)));
                }

                let parsed: VisionResponse = resp.json().await.map_err(|e| {
                    AppError::OpenAI(format!("Failed to parse OpenAI response: {}", e))
                })?;

                let content = parsed
                    .choices
                    .first()
                    .map(|c| c.message.content.clone())
                    .unwrap_or_default();

                parse_verdict_json(&content, LLMProvider::OpenAI, self.model.clone())
            }
            Err(e) => Err(AppError::OpenAI(format!("OpenAI request failed: {}", e))),
        }
    }
}
