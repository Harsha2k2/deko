use std::net::SocketAddr;

use anyhow::{bail, Result};
use tracing::info;

/// Application configuration loaded from environment variables and per-environment profiles.
///
/// Supports SQLite (default) and PostgreSQL (`postgres` feature flag).
/// Database type is auto-detected from the `DEKO_DATABASE_URL` prefix
/// (`sqlite:` or `postgres://`).
///
/// # Environment Profiles
/// Config profiles automatically adjust security-sensitive defaults based on
/// [`Environment`]:
/// - `Dev`: generous rate limits, larger body sizes, permissive CORS
/// - `Staging`: balanced defaults suitable for pre-production
/// - `Prod`: strict rate limits, small body sizes, no wildcard CORS
///
/// Environment variables always override profile defaults, so you can set
/// `DEKO_ENV=prod` but still use `DEKO_RATE_LIMIT_PER_MINUTE=120` if needed.
#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub env: Environment,
    pub admin_password: String,
    pub database_url: String,
    pub database_read_url: Option<String>,
    pub default_provider: LLMProvider,
    #[allow(dead_code)]
    pub default_model: String,
    pub openai_api_key: Option<String>,
    pub openai_model: String,
    pub gemini_api_key: Option<String>,
    pub gemini_model: String,
    pub api_key_secret: String,
    pub allowed_origins: Vec<String>,
    pub rate_limit_per_minute: u64,
    #[allow(dead_code)]
    pub max_screenshot_size_mb: usize,
    pub max_request_body_kb: usize,
    pub openai_timeout_secs: u64,
    pub gemini_timeout_secs: u64,
    pub anthropic_api_key: Option<String>,
    pub anthropic_model: String,
    pub anthropic_timeout_secs: u64,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub ollama_timeout_secs: u64,
    pub azure_api_key: Option<String>,
    pub azure_endpoint: String,
    pub azure_deployment: String,
    pub azure_api_version: String,
    pub azure_timeout_secs: u64,
    pub bedrock_model_id: String,
    pub bedrock_region: String,
    pub custom_provider_url: Option<String>,
    pub custom_provider_model: String,
    pub custom_provider_timeout_secs: u64,
    pub processor_poll_interval_secs: u64,
    pub action_ttl_secs: u64,
    pub webhook_url: Option<String>,
    pub jwt_secret: String,
    pub jwt_expiry_secs: i64,
}

#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LLMProvider {
    OpenAI,
    Gemini,
    Anthropic,
    Ollama,
    Azure,
    Bedrock,
    Custom,
}

impl std::fmt::Display for LLMProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LLMProvider::OpenAI => write!(f, "openai"),
            LLMProvider::Gemini => write!(f, "gemini"),
            LLMProvider::Anthropic => write!(f, "anthropic"),
            LLMProvider::Ollama => write!(f, "ollama"),
            LLMProvider::Azure => write!(f, "azure"),
            LLMProvider::Bedrock => write!(f, "bedrock"),
            LLMProvider::Custom => write!(f, "custom"),
        }
    }
}

/// The environment Deko is running in.
///
/// Controls log level, log format (JSON in prod), default config values, and
/// validation strictness.
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
    pub anthropic_model: String,
    pub anthropic_timeout_secs: u64,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub ollama_timeout_secs: u64,
    pub azure_endpoint: String,
    pub azure_deployment: String,
    pub azure_api_version: String,
    pub azure_timeout_secs: u64,
    pub bedrock_model_id: String,
    pub bedrock_region: String,
    pub custom_provider_model: String,
    pub custom_provider_timeout_secs: u64,
    pub processor_poll_interval_secs: u64,
    pub action_ttl_secs: u64,
}

impl Environment {
    pub fn defaults(&self) -> EnvProfile {
        match self {
            Environment::Dev => EnvProfile {
                default_model: "gemini-2.0-flash".to_string(),
                openai_model: "gpt-4o".to_string(),
                gemini_model: "gemini-2.0-flash".to_string(),
                anthropic_model: "claude-sonnet-4-20250514".to_string(),
                ollama_base_url: "http://localhost:11434".to_string(),
                ollama_model: "llama3.2".to_string(),
                allowed_origins: vec!["*".to_string()],
                rate_limit_per_minute: 120,
                max_screenshot_size_mb: 10,
                max_request_body_kb: 1024,
                openai_timeout_secs: 60,
                gemini_timeout_secs: 60,
                anthropic_timeout_secs: 60,
                ollama_timeout_secs: 120,
                azure_endpoint: "https://your-resource.openai.azure.com".to_string(),
                azure_deployment: "gpt-4o".to_string(),
                azure_api_version: "2024-08-01-preview".to_string(),
                azure_timeout_secs: 60,
                bedrock_model_id: "anthropic.claude-sonnet-4-20250514".to_string(),
                bedrock_region: "us-east-1".to_string(),
                custom_provider_model: "custom-model".to_string(),
                custom_provider_timeout_secs: 60,
                processor_poll_interval_secs: 1,
                action_ttl_secs: 3600,
            },
            Environment::Staging => EnvProfile {
                default_model: "gemini-2.0-flash".to_string(),
                openai_model: "gpt-4o".to_string(),
                gemini_model: "gemini-2.0-flash".to_string(),
                anthropic_model: "claude-sonnet-4-20250514".to_string(),
                ollama_base_url: "http://localhost:11434".to_string(),
                ollama_model: "llama3.2".to_string(),
                allowed_origins: vec![
                    "http://localhost:8000".to_string(),
                    "http://localhost:3000".to_string(),
                ],
                rate_limit_per_minute: 60,
                max_screenshot_size_mb: 10,
                max_request_body_kb: 512,
                openai_timeout_secs: 30,
                gemini_timeout_secs: 30,
                anthropic_timeout_secs: 30,
                ollama_timeout_secs: 120,
                azure_endpoint: "https://your-resource.openai.azure.com".to_string(),
                azure_deployment: "gpt-4o".to_string(),
                azure_api_version: "2024-08-01-preview".to_string(),
                azure_timeout_secs: 30,
                bedrock_model_id: "anthropic.claude-sonnet-4-20250514".to_string(),
                bedrock_region: "us-east-1".to_string(),
                custom_provider_model: "custom-model".to_string(),
                custom_provider_timeout_secs: 30,
                processor_poll_interval_secs: 2,
                action_ttl_secs: 1800,
            },
            Environment::Prod => EnvProfile {
                default_model: "gemini-2.0-flash".to_string(),
                openai_model: "gpt-4o".to_string(),
                gemini_model: "gemini-2.0-flash".to_string(),
                anthropic_model: "claude-sonnet-4-20250514".to_string(),
                ollama_base_url: "http://localhost:11434".to_string(),
                ollama_model: "llama3.2".to_string(),
                allowed_origins: vec![],
                rate_limit_per_minute: 30,
                max_screenshot_size_mb: 5,
                max_request_body_kb: 256,
                openai_timeout_secs: 15,
                gemini_timeout_secs: 15,
                anthropic_timeout_secs: 15,
                ollama_timeout_secs: 120,
                azure_endpoint: "https://your-resource.openai.azure.com".to_string(),
                azure_deployment: "gpt-4o".to_string(),
                azure_api_version: "2024-08-01-preview".to_string(),
                azure_timeout_secs: 15,
                bedrock_model_id: "anthropic.claude-sonnet-4-20250514".to_string(),
                bedrock_region: "us-east-1".to_string(),
                custom_provider_model: "custom-model".to_string(),
                custom_provider_timeout_secs: 15,
                processor_poll_interval_secs: 2,
                action_ttl_secs: 1800,
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

        let database_read_url = std::env::var("DEKO_DATABASE_READ_URL").ok();

        let default_provider = match std::env::var("LLM_DEFAULT_PROVIDER").as_deref() {
            Ok("openai") => LLMProvider::OpenAI,
            Ok("gemini") => LLMProvider::Gemini,
            Ok("anthropic") => LLMProvider::Anthropic,
            Ok("ollama") => LLMProvider::Ollama,
            Ok("azure") => LLMProvider::Azure,
            Ok("bedrock") => LLMProvider::Bedrock,
            Ok("custom") => LLMProvider::Custom,
            _ => LLMProvider::Gemini,
        };

        let default_model = std::env::var("LLM_DEFAULT_MODEL")
            .unwrap_or(profile.default_model);

        let openai_api_key = std::env::var("OPENAI_API_KEY").ok();

        let openai_model = std::env::var("OPENAI_MODEL")
            .unwrap_or(profile.openai_model);

        let gemini_api_key = std::env::var("GEMINI_API_KEY").ok();

        let gemini_model = std::env::var("GEMINI_MODEL")
            .unwrap_or(profile.gemini_model);

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

        let anthropic_api_key = std::env::var("ANTHROPIC_API_KEY").ok();

        let anthropic_model = std::env::var("ANTHROPIC_MODEL")
            .unwrap_or(profile.anthropic_model);

        let anthropic_timeout_secs = std::env::var("ANTHROPIC_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(profile.anthropic_timeout_secs);

        let ollama_base_url = std::env::var("OLLAMA_BASE_URL")
            .unwrap_or(profile.ollama_base_url);

        let ollama_model = std::env::var("OLLAMA_MODEL")
            .unwrap_or(profile.ollama_model);

        let ollama_timeout_secs = std::env::var("OLLAMA_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(profile.ollama_timeout_secs);

        let azure_api_key = std::env::var("AZURE_API_KEY").ok();

        let azure_endpoint = std::env::var("AZURE_ENDPOINT")
            .unwrap_or(profile.azure_endpoint);

        let azure_deployment = std::env::var("AZURE_DEPLOYMENT")
            .unwrap_or(profile.azure_deployment);

        let azure_api_version = std::env::var("AZURE_API_VERSION")
            .unwrap_or(profile.azure_api_version);

        let azure_timeout_secs = std::env::var("AZURE_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(profile.azure_timeout_secs);

        let bedrock_model_id = std::env::var("BEDROCK_MODEL_ID")
            .unwrap_or(profile.bedrock_model_id);

        let bedrock_region = std::env::var("BEDROCK_REGION")
            .unwrap_or(profile.bedrock_region);

        let custom_provider_url = std::env::var("CUSTOM_PROVIDER_URL").ok();

        let custom_provider_model = std::env::var("CUSTOM_PROVIDER_MODEL")
            .unwrap_or(profile.custom_provider_model);

        let custom_provider_timeout_secs = std::env::var("CUSTOM_PROVIDER_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(profile.custom_provider_timeout_secs);

        let processor_poll_interval_secs = std::env::var("DEKO_PROCESSOR_POLL_INTERVAL_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(profile.processor_poll_interval_secs);

        let action_ttl_secs = std::env::var("DEKO_ACTION_TTL_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(profile.action_ttl_secs);

        let webhook_url = std::env::var("DEKO_WEBHOOK_URL").ok();

        let jwt_secret = std::env::var("DEKO_JWT_SECRET")
            .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());

        let jwt_expiry_secs = std::env::var("DEKO_JWT_EXPIRY_SECS")
            .ok()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(3600);

        if jwt_expiry_secs <= 0 {
            bail!("DEKO_JWT_EXPIRY_SECS must be a positive number");
        }

        let config = Config {
            port,
            env,
            admin_password,
            database_url,
            database_read_url,
            default_provider,
            default_model,
            openai_api_key,
            openai_model,
            gemini_api_key,
            gemini_model,
            anthropic_api_key,
            anthropic_model,
            anthropic_timeout_secs,
            ollama_base_url,
            ollama_model,
            ollama_timeout_secs,
            azure_api_key,
            azure_endpoint,
            azure_deployment,
            azure_api_version,
            azure_timeout_secs,
            bedrock_model_id,
            bedrock_region,
            custom_provider_url,
            custom_provider_model,
            custom_provider_timeout_secs,
            api_key_secret,
            allowed_origins,
            rate_limit_per_minute,
            max_screenshot_size_mb,
            max_request_body_kb,
            openai_timeout_secs,
            gemini_timeout_secs,
            processor_poll_interval_secs,
            action_ttl_secs,
            webhook_url,
            jwt_secret,
            jwt_expiry_secs,
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

        if self.admin_password.len() < 8 {
            bail!("DEKO_ADMIN_PASSWORD must be at least 8 characters");
        }

        if let Some(ref key) = self.openai_api_key {
            if key.len() < 5 {
                bail!("OPENAI_API_KEY appears to be too short (min 5 characters)");
            }
        }

        if let Some(ref key) = self.gemini_api_key {
            if key.len() < 5 {
                bail!("GEMINI_API_KEY appears to be too short (min 5 characters)");
            }
        }

        if self.env == Environment::Prod && self.allowed_origins.iter().any(|o| o == "*") {
            bail!("Wildcard CORS origin '*' is not allowed in production. Set DEKO_ALLOWED_ORIGINS to specific origins.");
        }

        if self.env == Environment::Prod && self.allowed_origins.is_empty() {
            bail!("DEKO_ALLOWED_ORIGINS must be explicitly set in production. Set at least one allowed origin.");
        }

        if self.database_url.starts_with("postgres://") || self.database_url.starts_with("postgresql://") {
            #[cfg(not(feature = "postgres"))]
            bail!("PostgreSQL database URL provided but the 'postgres' feature is not enabled. Build with --features postgres or use a sqlite:// URL.");
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
                if self.anthropic_api_key.is_none() || self.anthropic_api_key.as_ref().unwrap().is_empty() {
                    bail!("ANTHROPIC_API_KEY must be set when using Anthropic provider");
                }
            }
            LLMProvider::Ollama => {
                // Ollama is always available (local), no key validation needed
            }
            LLMProvider::Azure => {
                if self.azure_api_key.is_none() || self.azure_api_key.as_ref().unwrap().is_empty() {
                    bail!("AZURE_API_KEY must be set when using Azure OpenAI provider");
                }
            }
            LLMProvider::Bedrock => {
                // Bedrock uses AWS credentials (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY) from env
            }
            LLMProvider::Custom => {
                // Custom provider uses CUSTOM_PROVIDER_URL — no key validation needed
            }
        }

        Ok(())
    }

    pub fn addr(&self) -> SocketAddr {
        SocketAddr::from(([0, 0, 0, 0], self.port))
    }

    #[allow(dead_code)]
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
