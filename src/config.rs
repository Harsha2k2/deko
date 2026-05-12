use std::net::SocketAddr;

use anyhow::{bail, Result};
use tracing::info;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub env: Environment,
    pub admin_password: String,
    pub database_url: String,
    pub default_provider: LLMProvider,
    pub default_model: String,
    pub openai_api_key: Option<String>,
    pub openai_model: String,
    pub gemini_api_key: Option<String>,
    pub gemini_model: String,
    pub api_key_secret: String,
    pub allowed_origins: Vec<String>,
    pub rate_limit_per_minute: u64,
    pub max_screenshot_size_mb: usize,
    pub max_request_body_kb: usize,
    pub openai_timeout_secs: u64,
    pub gemini_timeout_secs: u64,
    pub processor_poll_interval_secs: u64,
    pub webhook_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LLMProvider {
    OpenAI,
    Gemini,
    Anthropic,
}

impl std::fmt::Display for LLMProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LLMProvider::OpenAI => write!(f, "openai"),
            LLMProvider::Gemini => write!(f, "gemini"),
            LLMProvider::Anthropic => write!(f, "anthropic"),
        }
    }
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

pub struct EnvProfile {
    pub default_model: String,
    pub openai_model: String,
    pub gemini_model: String,
    pub allowed_origins: Vec<String>,
    pub rate_limit_per_minute: u64,
    pub max_screenshot_size_mb: usize,
    pub max_request_body_kb: usize,
    pub openai_timeout_secs: u64,
    pub gemini_timeout_secs: u64,
    pub processor_poll_interval_secs: u64,
}

impl Environment {
    pub fn defaults(&self) -> EnvProfile {
        match self {
            Environment::Dev => EnvProfile {
                default_model: "gemini-2.0-flash".to_string(),
                openai_model: "gpt-4o".to_string(),
                gemini_model: "gemini-2.0-flash".to_string(),
                allowed_origins: vec!["*".to_string()],
                rate_limit_per_minute: 120,
                max_screenshot_size_mb: 10,
                max_request_body_kb: 1024,
                openai_timeout_secs: 60,
                gemini_timeout_secs: 60,
                processor_poll_interval_secs: 1,
            },
            Environment::Staging => EnvProfile {
                default_model: "gemini-2.0-flash".to_string(),
                openai_model: "gpt-4o".to_string(),
                gemini_model: "gemini-2.0-flash".to_string(),
                allowed_origins: vec![
                    "http://localhost:8000".to_string(),
                    "http://localhost:3000".to_string(),
                ],
                rate_limit_per_minute: 60,
                max_screenshot_size_mb: 10,
                max_request_body_kb: 512,
                openai_timeout_secs: 30,
                gemini_timeout_secs: 30,
                processor_poll_interval_secs: 2,
            },
            Environment::Prod => EnvProfile {
                default_model: "gemini-2.0-flash".to_string(),
                openai_model: "gpt-4o".to_string(),
                gemini_model: "gemini-2.0-flash".to_string(),
                allowed_origins: vec![],
                rate_limit_per_minute: 30,
                max_screenshot_size_mb: 5,
                max_request_body_kb: 256,
                openai_timeout_secs: 15,
                gemini_timeout_secs: 15,
                processor_poll_interval_secs: 2,
            },
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

        let profile = env.defaults();

        let admin_password = std::env::var("DEKO_ADMIN_PASSWORD")
            .map_err(|_| anyhow::anyhow!("DEKO_ADMIN_PASSWORD is required"))?;

        let database_url = std::env::var("DEKO_DATABASE_URL")
            .map_err(|_| anyhow::anyhow!("DEKO_DATABASE_URL is required"))?;

        let default_provider = match std::env::var("LLM_DEFAULT_PROVIDER").as_deref() {
            Ok("openai") => LLMProvider::OpenAI,
            Ok("gemini") => LLMProvider::Gemini,
            Ok("anthropic") => LLMProvider::Anthropic,
            _ => LLMProvider::Gemini,
        };

        let default_model = std::env::var("LLM_DEFAULT_MODEL")
            .unwrap_or_else(|_| profile.default_model);

        let openai_api_key = std::env::var("OPENAI_API_KEY").ok();

        let openai_model = std::env::var("OPENAI_MODEL")
            .ok()
            .unwrap_or_else(|| profile.openai_model);

        let gemini_api_key = std::env::var("GEMINI_API_KEY").ok();

        let gemini_model = std::env::var("GEMINI_MODEL")
            .ok()
            .unwrap_or_else(|| profile.gemini_model);

        let api_key_secret = std::env::var("DEKO_API_KEY_SECRET")
            .map_err(|_| anyhow::anyhow!("DEKO_API_KEY_SECRET is required"))?;

        let allowed_origins = std::env::var("DEKO_ALLOWED_ORIGINS")
            .ok()
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(|| profile.allowed_origins);

        let rate_limit_per_minute = std::env::var("DEKO_RATE_LIMIT_PER_MINUTE")
            .ok()
            .and_then(|r| r.parse::<u64>().ok())
            .unwrap_or(profile.rate_limit_per_minute);

        let max_screenshot_size_mb = std::env::var("DEKO_MAX_SCREENSHOT_SIZE_MB")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(profile.max_screenshot_size_mb);

        let max_request_body_kb = std::env::var("DEKO_MAX_REQUEST_BODY_KB")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(profile.max_request_body_kb);

        let openai_timeout_secs = std::env::var("OPENAI_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(profile.openai_timeout_secs);

        let gemini_timeout_secs = std::env::var("GEMINI_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(profile.gemini_timeout_secs);

        let processor_poll_interval_secs = std::env::var("DEKO_PROCESSOR_POLL_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(profile.processor_poll_interval_secs);

        let webhook_url = std::env::var("DEKO_WEBHOOK_URL").ok();

        let config = Config {
            port,
            env,
            admin_password,
            database_url,
            default_provider,
            default_model,
            openai_api_key,
            openai_model,
            gemini_api_key,
            gemini_model,
            api_key_secret,
            allowed_origins,
            rate_limit_per_minute,
            max_screenshot_size_mb,
            max_request_body_kb,
            openai_timeout_secs,
            gemini_timeout_secs,
            processor_poll_interval_secs,
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

        if self.env == Environment::Prod && self.allowed_origins.iter().any(|o| o == "*") {
            bail!("Wildcard CORS origin '*' is not allowed in production. Set DEKO_ALLOWED_ORIGINS to specific origins.");
        }

        if self.env == Environment::Prod && self.allowed_origins.is_empty() {
            bail!("DEKO_ALLOWED_ORIGINS must be explicitly set in production. Set at least one allowed origin.");
        }

        match self.default_provider {
            LLMProvider::OpenAI => {
                if self.openai_api_key.is_none() || !self.openai_api_key.as_ref().unwrap().starts_with("sk-") {
                    bail!("OPENAI_API_KEY must be set and start with 'sk-' when using OpenAI provider");
                }
            }
            LLMProvider::Gemini => {
                if self.gemini_api_key.is_none() || self.gemini_api_key.as_ref().unwrap().is_empty() {
                    bail!("GEMINI_API_KEY must be set when using Gemini provider");
                }
            }
            LLMProvider::Anthropic => {
                bail!("Anthropic provider not yet implemented");
            }
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
