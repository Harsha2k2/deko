//! verdict webhook delivery.
//!
//! signatures are hmac-sha256 over `timestamp.payload` with the configured
//! secret, sent as `X-Deko-Signature: t=<ts>,v1=<hex>`. receivers verify by
//! recomputing over the raw body and rejecting timestamps older than ~5 min,
//! which blocks replay. this mirrors the github/stripe webhook scheme so
//! standard verification snippets apply.

use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::Sha256;
use tracing::{info, warn};

use crate::services::llm::VerdictResult;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct WebhookService {
    pub client: Client,
    pub webhook_url: Option<String>,
    pub webhook_secret: Option<String>,
}

impl WebhookService {
    pub fn new(webhook_url: Option<String>) -> Self {
        let webhook_secret = std::env::var("DEKO_WEBHOOK_SECRET").ok();
        Self {
            client: Client::new(),
            webhook_url,
            webhook_secret,
        }
    }

    /// computes the signed header value for a payload at the given timestamp.
    pub fn sign(&self, payload_bytes: &[u8], timestamp: i64) -> Option<String> {
        let secret = self.webhook_secret.as_ref()?;
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(payload_bytes);
        Some(format!(
            "t={},v1={}",
            timestamp,
            hex::encode(mac.finalize().into_bytes())
        ))
    }

    /// verifies a signature header against a payload (receiver-side logic,
    /// exposed for testing the scheme itself).
    pub fn verify(&self, payload_bytes: &[u8], timestamp: i64, header: &str) -> bool {
        let Some(expected) = self.sign(payload_bytes, timestamp) else {
            return false;
        };
        // constant-time compare via hmac equality on recomputed bytes
        let provided = header.trim();
        if provided.len() != expected.len() {
            return false;
        }
        provided
            .as_bytes()
            .iter()
            .zip(expected.as_bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }

    pub async fn send_verdict(
        &self,
        action_id: &str,
        verdict: &VerdictResult,
        agent_webhook_url: Option<&str>,
    ) -> anyhow::Result<()> {
        let url = agent_webhook_url.or(self.webhook_url.as_deref());
        let Some(url) = url else {
            return Ok(());
        };

        // agents can register webhook urls; they are untrusted input like any
        // other outbound target
        if let Err(e) = crate::services::egress::ValidatedUrl::parse(url) {
            warn!("Webhook url for action {} rejected by egress guard: {}", action_id, e);
            return Err(anyhow::anyhow!("webhook url rejected: {}", e));
        }

        let payload = serde_json::json!({
            "event": "verdict",
            "action_id": action_id,
            "decision": verdict.decision,
            "reason": &verdict.reason,
            "risk_level": verdict.risk_level,
            "provider": verdict.provider,
            "model": verdict.model,
        });

        let payload_bytes = serde_json::to_vec(&payload)?;
        let timestamp = chrono::Utc::now().timestamp();
        let signature = self.sign(&payload_bytes, timestamp);

        let mut request = self
            .client
            .post(url)
            .body(payload_bytes.clone())
            .header("Content-Type", "application/json");
        if let Some(sig) = signature {
            request = request
                .header("X-Deko-Signature", sig)
                .header("X-Deko-Timestamp", timestamp.to_string());
        }

        let mut last_err = None;
        for attempt in 0..=2 {
            if attempt > 0 {
                let delay = std::time::Duration::from_millis(1000 * 2u64.pow(attempt as u32));
                warn!(
                    "Webhook retry attempt {}/2 for action {}, waiting {:?}...",
                    attempt, action_id, delay
                );
                tokio::time::sleep(delay).await;
            }

            match request
                .try_clone()
                .unwrap_or_else(|| self.client.post(url).json(&payload))
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        info!("Webhook delivered for action {} on attempt {}", action_id, attempt + 1);
                        return Ok(());
                    } else {
                        warn!(
                            "Webhook failed for action {}: status {} (attempt {})",
                            action_id,
                            response.status(),
                            attempt + 1
                        );
                        last_err = Some(anyhow::anyhow!("Webhook returned {}", response.status()));
                    }
                }
                Err(e) => {
                    warn!(
                        "Webhook request failed for action {}: {} (attempt {})",
                        action_id,
                        e,
                        attempt + 1
                    );
                    last_err = Some(anyhow::anyhow!("Webhook request failed: {}", e));
                }
            }
        }

        warn!("Webhook exhausted all retries for action {}", action_id);
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Webhook failed after retries")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service_with_secret(secret: &str) -> WebhookService {
        WebhookService {
            client: Client::new(),
            webhook_url: None,
            webhook_secret: Some(secret.to_string()),
        }
    }

    #[test]
    fn signature_is_deterministic_and_timestamp_bound() {
        let svc = service_with_secret("whsec_test");
        let body = br#"{"decision":"approved"}"#;

        let s1 = svc.sign(body, 1_000_000).unwrap();
        let s2 = svc.sign(body, 1_000_000).unwrap();
        assert_eq!(s1, s2);

        let s3 = svc.sign(body, 1_000_001).unwrap();
        assert_ne!(s1, s3, "different timestamps must produce different sigs");

        assert!(s1.starts_with("t=1000000,v1="));
    }

    #[test]
    fn verify_accepts_valid_and_rejects_tampered() {
        let svc = service_with_secret("whsec_test");
        let body = br#"{"decision":"denied"}"#;
        let ts = 42i64;
        let sig = svc.sign(body, ts).unwrap();

        assert!(svc.verify(body, ts, &sig));

        let tampered = br#"{"decision":"approved"}"#;
        assert!(!svc.verify(tampered, ts, &sig), "payload swap must fail");
        assert!(!svc.verify(body, ts + 1, &sig), "replay with new ts must fail");

        let wrong_secret = service_with_secret("other");
        assert!(!wrong_secret.verify(body, ts, &sig), "secret swap must fail");
    }

    #[test]
    fn sign_without_secret_is_none() {
        let svc = WebhookService {
            client: Client::new(),
            webhook_url: None,
            webhook_secret: None,
        };
        assert!(svc.sign(b"x", 1).is_none());
        assert!(!svc.verify(b"x", 1, "t=1,v1=00"));
    }
}
