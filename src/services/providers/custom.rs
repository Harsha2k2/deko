use async_trait::async_trait;
use tracing::{error, info};

use crate::services::llm::{LLMProviderTrait, VerdictResult, parse_verdict_json, system_prompt};
use crate::config::LLMProvider;
use crate::error::{AppError, Result};

#[derive(Clone)]
pub struct CustomProvider {
    client: reqwest::Client,
    url: String,
    model: String,
}

#[derive(serde::Serialize)]
struct CustomProviderRequest {
    model: String,
    intent: String,
    payload: Option<String>,
    screenshot_base64: Option<String>,
    policy_context: String,
    system_prompt: String,
}

impl CustomProvider {
    pub fn new(url: String, model: String, timeout_secs: u64) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, url, model }
    }
}

#[async_trait]
impl LLMProviderTrait for CustomProvider {
    fn name(&self) -> LLMProvider {
        LLMProvider::Custom
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
        let request = CustomProviderRequest {
            model: self.model.clone(),
            intent: intent.to_string(),
            payload: payload.map(|s| s.to_string()),
            screenshot_base64: screenshot_base64.map(|s| s.to_string()),
            policy_context: policy_context.to_string(),
            system_prompt: system_prompt().to_string(),
        };

        info!(url = %self.url, model = %self.model, "Calling custom LLM provider");

        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                error!("Custom provider request failed: {}", e);
                AppError::OpenAI(format!("Custom provider request failed: {}", e))
            })?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        if !status.is_success() {
            error!("Custom provider error {}: {}", status, body);
            return Err(AppError::OpenAI(format!("Custom provider error {}: {}", status, body)));
        }

        parse_verdict_json(&body, LLMProvider::Custom, self.model.clone())
    }
}
