use async_trait::async_trait;
use std::sync::Mutex;
use tracing::{error, info};

use crate::services::llm::{LLMProviderTrait, VerdictResult, parse_verdict_json, system_prompt};
use crate::config::LLMProvider;
use crate::error::{AppError, Result};

#[derive(Clone)]
pub struct BedrockProvider {
    client: std::sync::Arc<Mutex<Option<aws_sdk_bedrockruntime::Client>>>,
    model_id: String,
    region: String,
}

#[derive(serde::Serialize)]
struct BedrockClaudeRequest {
    #[serde(rename = "anthropic_version")]
    version: String,
    max_tokens: usize,
    system: String,
    messages: Vec<BedrockMessage>,
}

#[derive(serde::Serialize)]
struct BedrockMessage {
    role: String,
    content: Vec<BedrockContent>,
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum BedrockContent {
    Text { text: String, #[serde(rename = "type")] content_type: String },
    Image {
        source: BedrockImageSource,
        #[serde(rename = "type")]
        content_type: String,
    },
}

#[derive(serde::Serialize)]
struct BedrockImageSource {
    #[serde(rename = "type")]
    source_type: String,
    media_type: String,
    data: String,
}

#[derive(serde::Deserialize)]
struct BedrockClaudeResponse {
    content: Vec<BedrockResponseContent>,
}

#[derive(serde::Deserialize)]
struct BedrockResponseContent {
    text: Option<String>,
}

impl BedrockProvider {
    pub fn new(model_id: String, region: String) -> Self {
        Self { client: std::sync::Arc::new(Mutex::new(None)), model_id, region }
    }

    fn client(&self) -> aws_sdk_bedrockruntime::Client {
        let mut guard = self.client.lock().unwrap();
        if let Some(ref client) = *guard {
            return client.clone();
        }

        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime for Bedrock");
        let client = rt.block_on(async {
            let config = aws_config::from_env()
                .region(aws_types::region::Region::new(self.region.clone()))
                .load()
                .await;
            let bedrock_config = aws_sdk_bedrockruntime::config::Builder::from(&config)
                .build();
            aws_sdk_bedrockruntime::Client::from_conf(bedrock_config)
        });

        *guard = Some(client.clone());
        client
    }

    fn build_request(&self, intent: &str, payload: Option<&str>, screenshot_base64: Option<&str>, policy_context: &str) -> BedrockClaudeRequest {
        let user_msg = if let Some(screenshot) = screenshot_base64 {
            BedrockMessage {
                role: "user".to_string(),
                content: vec![
                    BedrockContent::Text {
                        text: format!(
                            "Action Intent: {}\n\nPayload: {}\n\nPolicy Context: {}\n\nAnalyze this action and determine if it should be approved, denied, or escalated.",
                            intent,
                            payload.unwrap_or("(no payload)"),
                            policy_context
                        ),
                        content_type: "text".to_string(),
                    },
                    BedrockContent::Image {
                        source: BedrockImageSource {
                            source_type: "base64".to_string(),
                            media_type: "image/png".to_string(),
                            data: screenshot.to_string(),
                        },
                        content_type: "image".to_string(),
                    },
                ],
            }
        } else {
            BedrockMessage {
                role: "user".to_string(),
                content: vec![BedrockContent::Text {
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

        BedrockClaudeRequest {
            version: "bedrock-2023-05-31".to_string(),
            max_tokens: 1024,
            system: system_prompt().to_string(),
            messages: vec![user_msg],
        }
    }
}

#[async_trait]
impl LLMProviderTrait for BedrockProvider {
    fn name(&self) -> LLMProvider {
        LLMProvider::Bedrock
    }

    fn model_name(&self) -> String {
        self.model_id.clone()
    }

    async fn analyze_action(
        &self,
        intent: &str,
        payload: Option<&str>,
        screenshot_base64: Option<&str>,
        policy_context: &str,
    ) -> Result<VerdictResult> {
        let request = self.build_request(intent, payload, screenshot_base64, policy_context);

        info!(model = %self.model_id, "Calling AWS Bedrock API");

        let body = serde_json::to_vec(&request).map_err(|e| {
            error!("Failed to serialize Bedrock request: {}", e);
            AppError::OpenAI(format!("Bedrock serialization error: {}", e))
        })?;

        let response = self
            .client()
            .invoke_model()
            .model_id(&self.model_id)
            .content_type("application/json")
            .accept("application/json")
            .body(aws_sdk_bedrockruntime::primitives::Blob::new(body))
            .send()
            .await
            .map_err(|e| {
                error!("Bedrock API request failed: {}", e);
                AppError::OpenAI(format!("Bedrock request failed: {}", e))
            })?;

        let body_bytes = response.body.as_ref();
        let parsed: BedrockClaudeResponse = serde_json::from_slice(body_bytes).map_err(|e| {
            error!("Failed to parse Bedrock response: {}", e);
            AppError::OpenAI(format!("Failed to parse Bedrock response: {}", e))
        })?;

        let text = parsed.content
            .first()
            .and_then(|c| c.text.as_deref())
            .unwrap_or("");

        parse_verdict_json(text, LLMProvider::Bedrock, self.model_id.clone())
    }
}
