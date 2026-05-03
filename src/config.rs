use std::net::SocketAddr;

use anyhow::{bail, Result};
use tracing::info;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub env: Environment,
    pub admin_password: String,
    pub database_url: String,
    pub openai_api_key: String,
    pub openai_model: String,
    pub api_key_secret: String,
    pub allowed_origins: Vec<String>,
    pub rate_limit_per_minute: u64,
    pub max_screenshot_size_mb: usize,
    pub max_request_body_kb: usize,
    pub openai_timeout_secs: u64,
    pub webhook_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Environment {
    Dev,
    Staging,
    Prod,
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Environment::Dev => write!(f, "dev"),
            Environment::Staging => write!(f, "staging"),
            Environment::Prod => write!(f, "prod"),
        }
    }
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let port = std::env::var("DEKO_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8000);

        let env = match std::env::var("DEKO_ENV").as_deref() {
            Ok("prod") => Environment::Prod,
            Ok("staging") => Environment::Staging,
            _ => Environment::Dev,
        };

        let admin_password = std::env::var("DEKO_ADMIN_PASSWORD")
            .map_err(|_| anyhow::anyhow!("DEKO_ADMIN_PASSWORD is required"))?;

        let database_url = std::env::var("DEKO_DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DEKO_DATABASE_URL is required"))?;

        let openai_api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENAI_API_KEY is required"))?;

        let openai_model = std::env::var("OPENAI_MODEL")
            .ok()
            .unwrap_or_else(|| "gpt-4o".to_string());

        let api_key_secret = std::env::var("DEKO_API_KEY_SECRET")
            .map_err(|_| anyhow::anyhow!("DEKO_API_KEY_SECRET is required"))?;

        let allowed_origins = std::env::var("DEKO_ALLOWED_ORIGINS")
            .ok()
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(|| vec!["http://localhost:8000".to_string()]);

        let rate_limit_per_minute = std::env::var("DEKO_RATE_LIMIT_PER_MINUTE")
            .ok()
            .and_then(|r| r.parse::<u64>().ok())
            .unwrap_or(60);

        let max_screenshot_size_mb = std::env::var("DEKO_MAX_SCREENSHOT_SIZE_MB")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(10);

        let max_request_body_kb = std::env::var("DEKO_MAX_REQUEST_BODY_KB")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(512);

        let openai_timeout_secs = std::env::var("OPENAI_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30);

        let webhook_url = std::env::var("DEKO_WEBHOOK_URL").ok();

        let config = Config {
            port,
            env,
            admin_password,
            database_url,
            openai_api_key,
            openai_model,
            api_key_secret,
            allowed_origins,
            rate_limit_per_minute,
            max_screenshot_size_mb,
            max_request_body_kb,
            openai_timeout_secs,
            webhook_url,
        };

        config.validate()?;

        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        if self.admin_password == "changeme" && self.env == Environment::Prod {
            bail!("DEKO_ADMIN_PASSWORD must be changed in production");
        }

        if self.api_key_secret.len() < 16 {
            bail!("DEKO_API_KEY_SECRET must be at least 16 characters");
        }

        if !self.openai_api_key.starts_with("sk-") {
            bail!("OPENAI_API_KEY must start with 'sk-'");
        }

        Ok(())
    }

    pub fn addr(&self) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], self.port))
    }

    pub fn is_prod(&self) -> bool {
        self.env == Environment::Prod
    }
}

pub fn init_tracing(env: &Environment) {
    let log_level = match env {
        Environment::Prod => "info",
        Environment::Staging => "debug",
        Environment::Dev => "debug",
    };

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("{log_level},tower_http={log_level}").into());

    if env == &Environment::Prod {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_target(false)
            .init();
    }

    info!("Tracing initialized for environment: {}", env);
}
