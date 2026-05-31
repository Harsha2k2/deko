use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, ClientId, ClientSecret, CsrfToken, RedirectUrl,
    Scope, TokenUrl, AuthorizationCode, TokenResponse,
};
use oauth2::reqwest::http_client;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    pub enabled: bool,
    pub provider: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_url: String,
    pub allowed_domains: Vec<String>,
    pub allowed_emails: Vec<String>,
}

impl OAuthConfig {
    pub fn from_env() -> Self {
        let enabled = std::env::var("DEKO_OAUTH_ENABLED")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);

        Self {
            enabled,
            provider: std::env::var("DEKO_OAUTH_PROVIDER").unwrap_or_default(),
            client_id: std::env::var("DEKO_OAUTH_CLIENT_ID").unwrap_or_default(),
            client_secret: std::env::var("DEKO_OAUTH_CLIENT_SECRET").unwrap_or_default(),
            redirect_url: std::env::var("DEKO_OAUTH_REDIRECT_URL")
                .unwrap_or_else(|_| "http://localhost:8000/admin/auth/callback".to_string()),
            allowed_domains: std::env::var("DEKO_OAUTH_ALLOWED_DOMAINS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            allowed_emails: std::env::var("DEKO_OAUTH_ALLOWED_EMAILS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleUser {
    pub sub: String,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub hd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubUser {
    pub id: i64,
    pub login: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OAuthProvider {
    pub client: BasicClient,
    pub config: OAuthConfig,
}

impl OAuthProvider {
    pub fn new(config: OAuthConfig) -> Option<Self> {
        if !config.enabled || config.client_id.is_empty() || config.client_secret.is_empty() {
            return None;
        }

        let (auth_url, token_url, _userinfo_url): (&str, &str, &str) = match config.provider.as_str() {
            "google" => (
                "https://accounts.google.com/o/oauth2/v2/auth",
                "https://oauth2.googleapis.com/token",
                "https://www.googleapis.com/oauth2/v3/userinfo",
            ),
            "github" => (
                "https://github.com/login/oauth/authorize",
                "https://github.com/login/oauth/access_token",
                "https://api.github.com/user",
            ),
            _ => return None,
        };

        let client = BasicClient::new(
            ClientId::new(config.client_id.clone()),
            Some(ClientSecret::new(config.client_secret.clone())),
            AuthUrl::new(auth_url.to_string()).ok()?,
            Some(TokenUrl::new(token_url.to_string()).ok()?),
        )
        .set_redirect_uri(RedirectUrl::new(config.redirect_url.clone()).ok()?);

        Some(Self { client, config })
    }

    pub fn auth_url(&self) -> (Url, CsrfToken) {
        let mut scopes = match self.config.provider.as_str() {
            "google" => vec![
                Scope::new("openid".to_string()),
                Scope::new("email".to_string()),
                Scope::new("profile".to_string()),
            ],
            "github" => vec![
                Scope::new("read:user".to_string()),
                Scope::new("user:email".to_string()),
            ],
            _ => vec![],
        };

        let (auth_url, csrf_token) = self
            .client
            .authorize_url(CsrfToken::new_random)
            .add_scopes(scopes.drain(..))
            .url();

        (auth_url, csrf_token)
    }

    pub async fn exchange_code(&self, code: &str) -> Result<String, String> {
        let token_result = self
            .client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .request(http_client)
            .map_err(|e| format!("Token exchange failed: {}", e))?;

        Ok(token_result.access_token().secret().clone())
    }

    pub async fn get_user_info(&self, access_token: &str) -> Result<OAuthUser, String> {
        match self.config.provider.as_str() {
            "google" => self.get_google_user(access_token).await,
            "github" => self.get_github_user(access_token).await,
            _ => Err("Unsupported provider".to_string()),
        }
    }

    async fn get_google_user(&self, token: &str) -> Result<OAuthUser, String> {
        let client = reqwest::Client::new();
        let resp = client
            .get("https://www.googleapis.com/oauth2/v3/userinfo")
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("Google userinfo failed: {}", e))?;

        let user: GoogleUser = resp
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))?;

        self.validate_email(&user.email, user.hd.as_deref())?;

        Ok(OAuthUser {
            id: user.sub,
            email: user.email,
            name: user.name.unwrap_or_default(),
            avatar: user.picture,
            provider: "google".to_string(),
        })
    }

    async fn get_github_user(&self, token: &str) -> Result<OAuthUser, String> {
        let client = reqwest::Client::new();
        let resp = client
            .get("https://api.github.com/user")
            .header("User-Agent", "Deko")
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("GitHub userinfo failed: {}", e))?;

        let user: GitHubUser = resp
            .json()
            .await
            .map_err(|e| format!("Parse failed: {}", e))?;

        let email = if let Some(ref e) = user.email {
            e.clone()
        } else {
            let emails_resp = client
                .get("https://api.github.com/user/emails")
                .header("User-Agent", "Deko")
                .bearer_auth(token)
                .send()
                .await
                .map_err(|e| format!("GitHub emails failed: {}", e))?;

            let emails: Vec<serde_json::Value> = emails_resp
                .json()
                .await
                .map_err(|e| format!("Parse emails failed: {}", e))?;

            emails
                .iter()
                .find(|e| e["primary"].as_bool().unwrap_or(false))
                .and_then(|e| e["email"].as_str().map(String::from))
                .unwrap_or_default()
        };

        self.validate_email(&email, None)?;

        Ok(OAuthUser {
            id: user.id.to_string(),
            email,
            name: user.name.unwrap_or(user.login),
            avatar: user.avatar_url,
            provider: "github".to_string(),
        })
    }

    fn validate_email(&self, email: &str, hd: Option<&str>) -> Result<(), String> {
        if !self.config.allowed_emails.is_empty() && !self.config.allowed_emails.contains(&email.to_string()) {
            return Err(format!("Email {} not in allowed list", email));
        }
        if !self.config.allowed_domains.is_empty() {
            let domain = hd.unwrap_or_else(|| email.split('@').nth(1).unwrap_or(""));
            if !self.config.allowed_domains.iter().any(|d| domain == d) {
                return Err(format!("Domain {} not in allowed list", domain));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthUser {
    pub id: String,
    pub email: String,
    pub name: String,
    pub avatar: Option<String>,
    pub provider: String,
}
