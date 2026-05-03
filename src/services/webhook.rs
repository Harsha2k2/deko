use reqwest::Client;
use tracing::{info, warn};

use crate::services::VerdictResult;

#[derive(Clone)]
pub struct WebhookService {
    pub client: Client,
    pub webhook_url: Option<String>,
}

impl WebhookService {
    pub fn new(webhook_url: Option<String>) -> Self {
        Self {
            client: Client::new(),
            webhook_url,
        }
    }

    pub async fn send_verdict(&self, action_id: &str, verdict: &VerdictResult) -> anyhow::Result<()> {
        let Some(url) = &self.webhook_url else {
            return Ok(());
        };

        let payload = serde_json::json!({
            "event": "verdict",
            "action_id": action_id,
            "decision": verdict.decision,
            "reason": &verdict.reason,
            "risk_level": verdict.risk_level,
            "provider": verdict.provider,
            "model": verdict.model,
        });

        let response = self.client.post(url).json(&payload).send().await?;

        if response.status().is_success() {
            info!("Webhook delivered for action {}", action_id);
        } else {
            warn!("Webhook failed for action {}: status {}", action_id, response.status());
        }

        Ok(())
    }
}
