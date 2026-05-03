use reqwest::Client;

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
}
