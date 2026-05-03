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

        let mut last_err = None;
        for attempt in 0..=2 {
            if attempt > 0 {
                let delay = std::time::Duration::from_millis(1000 * 2u64.pow(attempt as u32));
                warn!("Webhook retry attempt {}/2 for action {}, waiting {:?}...", attempt, action_id, delay);
                tokio::time::sleep(delay).await;
            }

            match self.client.post(url).json(&payload).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        info!("Webhook delivered for action {} on attempt {}", action_id, attempt + 1);
                        return Ok(());
                    } else {
                        warn!("Webhook failed for action {}: status {} (attempt {})", action_id, response.status(), attempt + 1);
                        last_err = Some(anyhow::anyhow!("Webhook returned {}", response.status()));
                    }
                }
                Err(e) => {
                    warn!("Webhook request failed for action {}: {} (attempt {})", action_id, e, attempt + 1);
                    last_err = Some(anyhow::anyhow!("Webhook request failed: {}", e));
                }
            }
        }

        warn!("Webhook exhausted all retries for action {}", action_id);
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Webhook failed after retries")))
    }
}
