use reqwest::Client;

use crate::config::Config;

pub struct OpenAIClient {
    pub client: Client,
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
}

impl OpenAIClient {
    pub fn new(config: &Config) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.openai_timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_key: config.openai_api_key.clone(),
            model: config.openai_model.clone(),
            timeout_secs: config.openai_timeout_secs,
        }
    }
}
