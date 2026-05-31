use async_trait::async_trait;
use tracing::{error, info};

use crate::services::llm::{LLMProviderTrait, VerdictResult, parse_verdict_json, system_prompt};
use crate::config::LLMProvider;
use crate::error::{AppError, Result};

#[derive(Clone)]
pub struct AzureOpenAIProvider {
    client: reqwest::Client,
    endpoint: String,
    deployment: String,
    api_key: String,
    api_version: String,
}

#[derive(serde::Serialize)]
struct AzureChatRequest {
    messages: Vec<AzureMessage>,
    max_tokens: usize,
    temperature: f64,
}

#[derive(serde::Serialize)]
struct AzureMessage {
    role: String,
    content: Vec<AzureContent>,
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum AzureContent {
    Text { text: String, #[serde(rename = "type")] content_type: String },
    Image {
        image_url: AzureImageUrl,
        #[serde(rename = "type")]
        content_type: String,
    },
}

#[derive(serde::Serialize)]
struct AzureImageUrl {
    url: String,
}

#[derive(serde::Deserialize)]
struct AzureChatResponse {
    choices: Vec<AzureChoice>,
}

#[derive(serde::Deserialize)]
struct AzureChoice {
    message: AzureResponseMessage,
}

#[derive(serde::Deserialize)]
struct AzureResponseMessage {
    content: Option<String>,
}

impl AzureOpenAIProvider {
    pub fn new(endpoint: String, deployment: String, api_key: String, api_version: String, timeout_secs: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, endpoint, deployment, api_key, api_version }
    }

    fn build_request(&self, intent: &str, payload: Option<&str>, screenshot_base64: Option<&str>, policy_context: &str) -> AzureChatRequest {
        let system_msg = AzureMessage {
            role: "system".to_string(),
            content: vec![AzureContent::Text {
                text: system_prompt().to_string(),
                content_type: "text".to_string(),
            }],
        };

        let user_msg = if let Some(screenshot) = screenshot_base64 {
            AzureMessage {
                role: "user".to_string(),
                content: vec![
                    AzureContent::Text {
                        text: format!(
                            "Action Intent: {}\n\nPayload: {}\n\nPolicy Context: {}\n\nAnalyze this action and determine if it should be approved, denied, or escalated.",
                            intent,
                            payload.unwrap_or("(no payload)"),
                            policy_context
                        ),
                        content_type: "text".to_string(),
                    },
                    AzureContent::Image {
                        image_url: AzureImageUrl {
                            url: format!("data:image/png;base64,{}", screenshot),
                        },
                        content_type: "image_url".to_string(),
                    },
                ],
            }
        } else {
            AzureMessage {
                role: "user".to_string(),
                content: vec![AzureContent::Text {
                    text: format!(
                        "Action Intent: {}\n\nPayload: {}\n\nPolicy Context: {}\n\nAnalyze this action and determine if it should be approved, denied, or escalated.",
                        intent,
                        payload.unwrap_or("(no payload)"),
                        policy_context
                    ),
                    content_type: "text".to_string(),
                }],
            }
        };

        AzureChatRequest {
            messages: vec![system_msg, user_msg],
            max_tokens: 1024,
            temperature: 0.1,
        }
    }

    fn endpoint_url(&self) -> String {
        format!(
            "{}openai/deployments/{}/chat/completions?api-version={}",
            self.endpoint.trim_end_matches('/').trim_end_matches('/'),
            self.deployment,
            self.api_version
        )
    }
}

#[async_trait]
impl LLMProviderTrait for AzureOpenAIProvider {
    fn name(&self) -> LLMProvider {
        LLMProvider::Azure
    }

    fn model_name(&self) -> String {
        format!("azure/{}", self.deployment)
    }

    async fn analyze_action(
        &self,
        intent: &str,
        payload: Option<&str>,
        screenshot_base64: Option<&str>,
        policy_context: &str,
    ) -> Result<VerdictResult> {
        let request = self.build_request(intent, payload, screenshot_base64, policy_context);

        info!(deployment = %self.deployment, "Calling Azure OpenAI API");

        let response = self
            .client
            .post(self.endpoint_url())
            .header("api-key", &self.api_key)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                error!("Azure OpenAI API request failed: {}", e);
                AppError::OpenAI(format!("Azure OpenAI request failed: {}", e))
            })?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            error!("Azure OpenAI API error {}: {}", status, body);
            return Err(AppError::OpenAI(format!("Azure OpenAI API error {}: {}", status, body)));
        }

        let parsed: AzureChatResponse = serde_json::from_str(&body).map_err(|e| {
            error!("Failed to parse Azure OpenAI response: {}", e);
            AppError::OpenAI(format!("Failed to parse Azure OpenAI response: {}", e))
        })?;

        let text = parsed.choices
            .first()
            .and_then(|c| c.message.content.as_deref())
            .unwrap_or("");

        parse_verdict_json(text, LLMProvider::Azure, self.model_name())
    }
}
