use async_trait::async_trait;
use tracing::{error, info};

use crate::services::llm::{LLMProviderTrait, VerdictResult, parse_verdict_json, system_prompt};
use crate::config::LLMProvider;
use crate::error::{AppError, Result};

#[derive(Clone)]
pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

#[derive(serde::Serialize)]
struct OllamaRequest {
    model: String,
    system: String,
    prompt: String,
    stream: bool,
    options: OllamaOptions,
}

#[derive(serde::Serialize)]
struct OllamaOptions {
    temperature: f64,
    num_predict: usize,
}

#[derive(serde::Deserialize)]
struct OllamaResponse {
    response: String,
}

impl OllamaProvider {
    pub fn new(base_url: String, model: String, timeout_secs: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, base_url, model }
    }

    fn build_prompt(&self, intent: &str, payload: Option<&str>, screenshot_base64: Option<&str>, policy_context: &str) -> String {
        if screenshot_base64.is_some() {
            format!(
                "Action Intent: {}\n\nPayload: {}\n\nScreenshot: [image data omitted - Ollama does not support vision in this mode]\n\nPolicy Context: {}\n\nAnalyze this action and determine if it should be approved, denied, or escalated.",
                intent,
                payload.unwrap_or("(no payload)"),
                policy_context
            )
        } else {
            format!(
                "Action Intent: {}\n\nPayload: {}\n\nPolicy Context: {}\n\nAnalyze this action and determine if it should be approved, denied, or escalated.",
                intent,
                payload.unwrap_or("(no payload)"),
                policy_context
            )
        }
    }

    fn endpoint(&self) -> String {
        format!("{}/api/generate", self.base_url.trim_end_matches('/'))
    }
}

#[async_trait]
impl LLMProviderTrait for OllamaProvider {
    fn name(&self) -> LLMProvider {
        LLMProvider::Ollama
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
        let prompt = self.build_prompt(intent, payload, screenshot_base64, policy_context);

        let request = OllamaRequest {
            model: self.model.clone(),
            system: system_prompt().to_string(),
            prompt,
            stream: false,
            options: OllamaOptions {
                temperature: 0.1,
                num_predict: 1024,
            },
        };

        info!(model = %self.model, base_url = %self.base_url, "Calling Ollama API");

        let response = self
            .client
            .post(self.endpoint())
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                error!("Ollama API request failed: {}", e);
                AppError::OpenAI(format!("Ollama request failed: {}", e))
            })?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            error!("Ollama API error {}: {}", status, body);
            return Err(AppError::OpenAI(format!("Ollama API error {}: {}", status, body)));
        }

        let parsed: OllamaResponse = serde_json::from_str(&body).map_err(|e| {
            error!("Failed to parse Ollama response: {}", e);
            AppError::OpenAI(format!("Failed to parse Ollama response: {}", e))
        })?;

        parse_verdict_json(&parsed.response, LLMProvider::Ollama, self.model.clone())
    }
}
