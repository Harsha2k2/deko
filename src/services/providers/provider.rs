use std::sync::Arc;

use async_trait::async_trait;
use aws_config::BehaviorVersion;
use tracing::{error, info};

use crate::config::{Config, LLMProvider};
use crate::error::{AppError, Result};
use crate::services::llm::{parse_verdict_json, system_prompt, LLMProviderTrait, VerdictResult};

#[derive(Clone)]
pub struct UnifiedProvider {
    client: reqwest::Client,
    config: Arc<Config>,
    provider_type: LLMProvider,
    bedrock_client: std::sync::Arc<tokio::sync::OnceCell<aws_sdk_bedrockruntime::Client>>,
}

// ---- OpenAI-compatible request types ----
#[derive(serde::Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: usize,
    temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(serde::Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

#[derive(serde::Serialize)]
struct ChatMessage {
    role: String,
    content: Vec<ChatContent>,
}

#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ChatContent {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}

#[derive(serde::Serialize)]
struct ImageUrl {
    url: String,
}

#[derive(serde::Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(serde::Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(serde::Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

// ---- Gemini request types ----
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

// ---- Anthropic request types ----
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
    Text {
        text: String,
    },
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

// ---- Ollama request types ----
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

// ---- Bedrock request types ----
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
    Text {
        text: String,
        #[serde(rename = "type")]
        content_type: String,
    },
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

// ---- Custom provider request types ----
#[derive(serde::Serialize)]
struct CustomProviderRequest {
    model: String,
    intent: String,
    payload: Option<String>,
    screenshot_base64: Option<String>,
    policy_context: String,
    system_prompt: String,
}

impl UnifiedProvider {
    pub fn new(config: Arc<Config>, provider_type: LLMProvider) -> Self {
        let timeout_secs = match provider_type {
            LLMProvider::OpenAI => config.openai_timeout_secs,
            LLMProvider::Gemini => config.gemini_timeout_secs,
            LLMProvider::Anthropic => config.anthropic_timeout_secs,
            LLMProvider::Ollama => config.ollama_timeout_secs,
            LLMProvider::Azure => config.azure_timeout_secs,
            LLMProvider::Bedrock => 60,
            LLMProvider::Custom => config.custom_provider_timeout_secs,
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            config,
            provider_type,
            bedrock_client: std::sync::Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    fn model_for_provider(&self) -> String {
        match self.provider_type {
            LLMProvider::OpenAI => self.config.openai_model.clone(),
            LLMProvider::Gemini => self.config.gemini_model.clone(),
            LLMProvider::Anthropic => self.config.anthropic_model.clone(),
            LLMProvider::Ollama => self.config.ollama_model.clone(),
            LLMProvider::Azure => format!("azure/{}", self.config.azure_deployment),
            LLMProvider::Bedrock => self.config.bedrock_model_id.clone(),
            LLMProvider::Custom => self.config.custom_provider_model.clone(),
        }
    }

    fn user_prompt(&self, intent: &str, payload: Option<&str>, policy_context: &str) -> String {
        format!(
            "Action Intent: {}\n\nPayload: {}\n\nPolicy Context: {}\n\nAnalyze this action and determine if it should be approved, denied, or escalated.",
            intent,
            payload.unwrap_or("(no payload)"),
            policy_context
        )
    }

    async fn call_openai(
        &self,
        intent: &str,
        payload: Option<&str>,
        screenshot_base64: Option<&str>,
        policy_context: &str,
    ) -> Result<VerdictResult> {
        let api_key = self
            .config
            .openai_api_key
            .as_deref()
            .ok_or_else(|| AppError::OpenAI("OPENAI_API_KEY not configured".into()))?;

        let mut messages = vec![ChatMessage {
            role: "system".to_string(),
            content: vec![ChatContent::Text {
                text: system_prompt().to_string(),
            }],
        }];

        let mut user_text = format!("Action Intent: {}\n", intent);
        if let Some(p) = payload {
            user_text.push_str(&format!("Payload: {}\n", p));
        }
        user_text.push_str(&format!("Active Policies: {}\n", policy_context));
        user_text.push_str("Respond in JSON format with keys: decision (approved/denied/escalate), reason (string), risk_level (low/medium/high/critical).");

        let mut content_parts = vec![ChatContent::Text { text: user_text }];
        if let Some(img) = screenshot_base64 {
            content_parts.push(ChatContent::ImageUrl {
                image_url: ImageUrl {
                    url: format!("data:image/png;base64,{}", img),
                },
            });
        }

        messages.push(ChatMessage {
            role: "user".to_string(),
            content: content_parts,
        });

        let req_body = ChatRequest {
            model: self.model_for_provider(),
            messages,
            max_tokens: 512,
            temperature: 0.0,
            response_format: Some(ResponseFormat {
                format_type: "json_object".to_string(),
            }),
        };

        info!("OpenAI request: model={}", self.config.openai_model);
        let text = self
            .send_chat_request(
                "https://api.openai.com/v1/chat/completions",
                Some(("Authorization", format!("Bearer {}", api_key))),
                &req_body,
            )
            .await?;
        parse_verdict_json(&text, LLMProvider::OpenAI, self.model_for_provider())
    }

    async fn call_gemini(
        &self,
        intent: &str,
        payload: Option<&str>,
        screenshot_base64: Option<&str>,
        policy_context: &str,
    ) -> Result<VerdictResult> {
        let api_key = self
            .config
            .gemini_api_key
            .as_deref()
            .ok_or_else(|| AppError::OpenAI("GEMINI_API_KEY not configured".into()))?;

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

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.config.gemini_model, api_key
        );

        info!("Gemini request: model={}", self.config.gemini_model);

        let text = self
            .send_retry(&url, None, &req_body, |resp: GeminiResponse| {
                resp.candidates
                    .first()
                    .and_then(|c| c.content.parts.first())
                    .and_then(|p| p.text.clone())
                    .ok_or_else(|| AppError::OpenAI("Gemini returned empty response".into()))
            })
            .await?;

        parse_verdict_json(&text, LLMProvider::Gemini, self.model_for_provider())
    }

    async fn call_anthropic(
        &self,
        intent: &str,
        payload: Option<&str>,
        screenshot_base64: Option<&str>,
        policy_context: &str,
    ) -> Result<VerdictResult> {
        let api_key = self
            .config
            .anthropic_api_key
            .as_deref()
            .ok_or_else(|| AppError::OpenAI("ANTHROPIC_API_KEY not configured".into()))?;

        let user_message = if let Some(screenshot) = screenshot_base64 {
            AnthropicMessage {
                role: "user".to_string(),
                content: vec![
                    AnthropicContent::Text {
                        text: self.user_prompt(intent, payload, policy_context),
                    },
                    AnthropicContent::Image {
                        source: ImageSource {
                            source_type: "base64".to_string(),
                            media_type: "image/png".to_string(),
                            data: screenshot.to_string(),
                        },
                        content_type: "image".to_string(),
                    },
                ],
            }
        } else {
            AnthropicMessage {
                role: "user".to_string(),
                content: vec![AnthropicContent::Text {
                    text: self.user_prompt(intent, payload, policy_context),
                }],
            }
        };

        let req_body = AnthropicRequest {
            model: self.config.anthropic_model.clone(),
            max_tokens: 1024,
            system: system_prompt().to_string(),
            messages: vec![user_message],
        };

        info!("Anthropic request: model={}", self.config.anthropic_model);

        let text = self.send_anthropic(api_key, &req_body).await?;
        parse_verdict_json(&text, LLMProvider::Anthropic, self.model_for_provider())
    }

    async fn call_ollama(
        &self,
        intent: &str,
        payload: Option<&str>,
        _screenshot_base64: Option<&str>,
        policy_context: &str,
    ) -> Result<VerdictResult> {
        let prompt = self.user_prompt(intent, payload, policy_context);

        let req_body = OllamaRequest {
            model: self.config.ollama_model.clone(),
            system: system_prompt().to_string(),
            prompt,
            stream: false,
            options: OllamaOptions {
                temperature: 0.1,
                num_predict: 1024,
            },
        };

        let url = format!("{}/api/generate", self.config.ollama_base_url.trim_end_matches('/'));
        info!("Ollama request: model={}", self.config.ollama_model);

        let text = self
            .send_raw(&url, None, &req_body, |resp: OllamaResponse| resp.response)
            .await?;
        parse_verdict_json(&text, LLMProvider::Ollama, self.model_for_provider())
    }

    async fn call_azure(
        &self,
        intent: &str,
        payload: Option<&str>,
        screenshot_base64: Option<&str>,
        policy_context: &str,
    ) -> Result<VerdictResult> {
        let api_key = self
            .config
            .azure_api_key
            .as_deref()
            .ok_or_else(|| AppError::OpenAI("AZURE_API_KEY not configured".into()))?;

        let system_msg = ChatMessage {
            role: "system".to_string(),
            content: vec![ChatContent::Text {
                text: system_prompt().to_string(),
            }],
        };

        let user_msg = if let Some(screenshot) = screenshot_base64 {
            ChatMessage {
                role: "user".to_string(),
                content: vec![
                    ChatContent::Text {
                        text: self.user_prompt(intent, payload, policy_context),
                    },
                    ChatContent::ImageUrl {
                        image_url: ImageUrl {
                            url: format!("data:image/png;base64,{}", screenshot),
                        },
                    },
                ],
            }
        } else {
            ChatMessage {
                role: "user".to_string(),
                content: vec![ChatContent::Text {
                    text: self.user_prompt(intent, payload, policy_context),
                }],
            }
        };

        // Azure uses ChatRequest but without response_format and with different auth
        let req_body = ChatRequest {
            model: self.config.azure_deployment.clone(),
            messages: vec![system_msg, user_msg],
            max_tokens: 1024,
            temperature: 0.1,
            response_format: None,
        };

        let url = format!(
            "{}openai/deployments/{}/chat/completions?api-version={}",
            self.config.azure_endpoint.trim_end_matches('/'),
            self.config.azure_deployment,
            self.config.azure_api_version,
        );

        info!("Azure request: deployment={}", self.config.azure_deployment);

        let text = self
            .send_raw(&url, Some(("api-key", api_key)), &req_body, |resp: ChatResponse| {
                resp.choices
                    .first()
                    .and_then(|c| c.message.content.clone())
                    .unwrap_or_default()
            })
            .await?;

        parse_verdict_json(&text, LLMProvider::Azure, self.model_for_provider())
    }

    async fn call_bedrock(
        &self,
        intent: &str,
        payload: Option<&str>,
        screenshot_base64: Option<&str>,
        policy_context: &str,
    ) -> Result<VerdictResult> {
        let user_msg = if let Some(screenshot) = screenshot_base64 {
            BedrockMessage {
                role: "user".to_string(),
                content: vec![
                    BedrockContent::Text {
                        text: self.user_prompt(intent, payload, policy_context),
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
                    text: self.user_prompt(intent, payload, policy_context),
                    content_type: "text".to_string(),
                }],
            }
        };

        let req_body = BedrockClaudeRequest {
            version: "bedrock-2023-05-31".to_string(),
            max_tokens: 1024,
            system: system_prompt().to_string(),
            messages: vec![user_msg],
        };

        info!("Bedrock request: model={}", self.config.bedrock_model_id);

        let body = serde_json::to_vec(&req_body)
            .map_err(|e| AppError::OpenAI(format!("Bedrock serialization error: {}", e)))?;

        let client = self
            .bedrock_client
            .get_or_try_init(|| async {
                let config = aws_config::defaults(BehaviorVersion::latest())
                    .region(aws_types::region::Region::new(self.config.bedrock_region.clone()))
                    .load()
                    .await;
                let bedrock_config = aws_sdk_bedrockruntime::config::Builder::from(&config).build();
                Ok::<aws_sdk_bedrockruntime::Client, AppError>(aws_sdk_bedrockruntime::Client::from_conf(
                    bedrock_config,
                ))
            })
            .await?;

        let response = client
            .invoke_model()
            .model_id(&self.config.bedrock_model_id)
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

        let text = parsed.content.first().and_then(|c| c.text.as_deref()).unwrap_or("");

        parse_verdict_json(text, LLMProvider::Bedrock, self.model_for_provider())
    }

    async fn call_custom(
        &self,
        intent: &str,
        payload: Option<&str>,
        screenshot_base64: Option<&str>,
        policy_context: &str,
    ) -> Result<VerdictResult> {
        let url = self
            .config
            .custom_provider_url
            .as_deref()
            .ok_or_else(|| AppError::OpenAI("CUSTOM_PROVIDER_URL not configured".into()))?;

        let req_body = CustomProviderRequest {
            model: self.config.custom_provider_model.clone(),
            intent: intent.to_string(),
            payload: payload.map(|s| s.to_string()),
            screenshot_base64: screenshot_base64.map(|s| s.to_string()),
            policy_context: policy_context.to_string(),
            system_prompt: system_prompt().to_string(),
        };

        info!("Custom provider request: url={}", url);

        let body = serde_json::to_value(&req_body)
            .map_err(|e| AppError::OpenAI(format!("Custom provider serialization error: {}", e)))?;

        let text = self.send_raw_custom(url, body).await?;
        parse_verdict_json(&text, LLMProvider::Custom, self.model_for_provider())
    }

    // ---- HTTP helpers ----

    async fn send_chat_request(&self, url: &str, auth: Option<(&str, String)>, body: &ChatRequest) -> Result<String> {
        let mut req = self.client.post(url).header("Content-Type", "application/json");
        if let Some((key, val)) = auth {
            req = req.header(key, val);
        }
        req = req.json(body);

        let response = req
            .send()
            .await
            .map_err(|e| AppError::OpenAI(format!("Request failed: {}", e)))?;
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::OpenAI(format!("API error {}: {}", status, body_text)));
        }
        let parsed: ChatResponse = serde_json::from_str(&body_text)
            .map_err(|e| AppError::OpenAI(format!("Failed to parse response: {}", e)))?;
        Ok(parsed
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default())
    }

    async fn send_retry<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        auth: Option<(&str, &str)>,
        body: &impl serde::Serialize,
        extract: fn(T) -> Result<String>,
    ) -> Result<String> {
        let mut last_err = None;
        for attempt in 0..=2 {
            if attempt > 0 {
                let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt as u32));
                tokio::time::sleep(delay).await;
            }
            let mut req = self.client.post(url).header("Content-Type", "application/json");
            if let Some((key, val)) = auth {
                req = req.header(key, val);
            }
            req = req.json(body);

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if !status.is_success() {
                        let body_text = resp.text().await.unwrap_or_default();
                        error!("API error (attempt {}): {} - {}", attempt + 1, status, body_text);
                        last_err = Some(AppError::OpenAI(format!("API returned {}: {}", status, body_text)));
                        continue;
                    }
                    let parsed: T = resp
                        .json()
                        .await
                        .map_err(|e| AppError::OpenAI(format!("Failed to parse response: {}", e)))?;
                    return extract(parsed);
                }
                Err(e) => {
                    error!("Request failed (attempt {}): {}", attempt + 1, e);
                    last_err = Some(AppError::OpenAI(format!("Request failed: {}", e)));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| AppError::OpenAI("Exhausted all retries".into())))
    }

    async fn send_raw<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        auth: Option<(&str, &str)>,
        body: &impl serde::Serialize,
        extract: fn(T) -> String,
    ) -> Result<String> {
        let mut req = self.client.post(url).header("Content-Type", "application/json");
        if let Some((key, val)) = auth {
            req = req.header(key, val);
        }
        req = req.json(body);

        let response = req
            .send()
            .await
            .map_err(|e| AppError::OpenAI(format!("Request failed: {}", e)))?;
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::OpenAI(format!("API error {}: {}", status, body_text)));
        }
        let parsed: T = serde_json::from_str(&body_text)
            .map_err(|e| AppError::OpenAI(format!("Failed to parse response: {}", e)))?;
        Ok(extract(parsed))
    }

    async fn send_anthropic(&self, api_key: &str, body: &AnthropicRequest) -> Result<String> {
        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| AppError::OpenAI(format!("Anthropic request failed: {}", e)))?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::OpenAI(format!(
                "Anthropic API error {}: {}",
                status, body_text
            )));
        }
        let parsed: AnthropicResponse = serde_json::from_str(&body_text)
            .map_err(|e| AppError::OpenAI(format!("Failed to parse Anthropic response: {}", e)))?;
        Ok(parsed
            .content
            .first()
            .and_then(|c| c.text.as_deref())
            .unwrap_or("")
            .to_string())
    }

    async fn send_raw_custom(&self, url: &str, body: serde_json::Value) -> Result<String> {
        let response = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::OpenAI(format!("Custom provider request failed: {}", e)))?;

        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::OpenAI(format!(
                "Custom provider error {}: {}",
                status, body_text
            )));
        }
        Ok(body_text)
    }
}

#[async_trait]
impl LLMProviderTrait for UnifiedProvider {
    fn name(&self) -> LLMProvider {
        self.provider_type
    }

    fn model_name(&self) -> String {
        self.model_for_provider()
    }

    async fn analyze_action(
        &self,
        intent: &str,
        payload: Option<&str>,
        screenshot_base64: Option<&str>,
        policy_context: &str,
    ) -> Result<VerdictResult> {
        match self.provider_type {
            LLMProvider::OpenAI => {
                self.call_openai(intent, payload, screenshot_base64, policy_context)
                    .await
            }
            LLMProvider::Gemini => {
                self.call_gemini(intent, payload, screenshot_base64, policy_context)
                    .await
            }
            LLMProvider::Anthropic => {
                self.call_anthropic(intent, payload, screenshot_base64, policy_context)
                    .await
            }
            LLMProvider::Ollama => {
                self.call_ollama(intent, payload, screenshot_base64, policy_context)
                    .await
            }
            LLMProvider::Azure => {
                self.call_azure(intent, payload, screenshot_base64, policy_context)
                    .await
            }
            LLMProvider::Bedrock => {
                self.call_bedrock(intent, payload, screenshot_base64, policy_context)
                    .await
            }
            LLMProvider::Custom => {
                self.call_custom(intent, payload, screenshot_base64, policy_context)
                    .await
            }
        }
    }
}
