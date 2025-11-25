use crate::config::TrueLayerConfig;
use crate::error::{AppError, Result};
use oauth2::{
    AuthUrl, AuthorizationCode, Client, ClientId, ClientSecret, CsrfToken, EndpointNotSet,
    EndpointSet, PkceCodeChallenge, RedirectUrl, RefreshToken, Scope, StandardRevocableToken,
    TokenResponse, TokenUrl,
    basic::{
        BasicClient, BasicErrorResponse, BasicRevocationErrorResponse,
        BasicTokenIntrospectionResponse, BasicTokenResponse,
    },
};
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use tiny_http::{Response, Server};
use tracing::{debug, info, instrument, warn};
use url::Url;

const TRUELAYER_SCOPES: &[&str] = &["cards", "transactions", "offline_access"];
const CALLBACK_PORT: u16 = 3000;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(super) struct TrueLayerTokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Expiry time as seconds since Unix epoch
    pub expires_at: i64,
}

impl TrueLayerTokens {
    /// Check if the access token is expired or about to expire (within 5 minutes)
    pub fn is_expired(&self) -> bool {
        let now = chrono::Utc::now().timestamp();
        // Add 5 minute buffer to refresh before actual expiry
        self.expires_at < (now + 300)
    }
}

// Type alias for the client when Auth and Token URLs are set
type ConfiguredClient = Client<
    BasicErrorResponse,
    BasicTokenResponse,
    BasicTokenIntrospectionResponse,
    StandardRevocableToken,
    BasicRevocationErrorResponse,
    EndpointSet,    // HasAuthUrl
    EndpointNotSet, // HasDeviceAuthUrl
    EndpointNotSet, // HasIntrospectionUrl
    EndpointNotSet, // HasRevocationUrl
    EndpointSet,    // HasTokenUrl
>;

pub(super) struct TrueLayerAuth {
    client: ConfiguredClient,
    http_client: reqwest::Client, // Add reqwest client
    providers: String,
}

impl TrueLayerAuth {
    pub(super) fn new(config: &TrueLayerConfig) -> Result<Self> {
        let client_id = ClientId::new(config.client_id.clone());
        let client_secret = ClientSecret::new(config.client_secret.clone());

        let base_auth_url = config.auth_url();
        let auth_url = AuthUrl::new(base_auth_url.to_string())
            .map_err(|e| AppError::Auth(format!("Invalid auth URL: {}", e)))?;
        let token_url = TokenUrl::new(format!("{}/connect/token", base_auth_url))
            .map_err(|e| AppError::Auth(format!("Invalid token URL: {}", e)))?;

        let redirect_url = format!("http://localhost:{}/callback", CALLBACK_PORT);
        let client = BasicClient::new(client_id)
            .set_client_secret(client_secret)
            .set_auth_uri(auth_url)
            .set_token_uri(token_url)
            .set_redirect_uri(
                RedirectUrl::new(redirect_url)
                    .map_err(|e| AppError::Auth(format!("Invalid redirect URL: {}", e)))?,
            );

        let http_client = reqwest::ClientBuilder::new()
            .redirect(Policy::none())
            .build()
            .map_err(|e| AppError::Auth(format!("Failed to build reqwest client: {}", e)))?;

        Ok(Self {
            client,
            http_client,
            providers: config.providers(),
        })
    }

    pub(super) fn http_client(&self) -> reqwest::Client {
        self.http_client.clone()
    }

    async fn authenticate(&self) -> Result<TrueLayerTokens> {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let scopes = TRUELAYER_SCOPES
            .iter()
            .map(|s| Scope::new(s.to_string()))
            .collect::<Vec<Scope>>();
        let auth_request = self
            .client
            .authorize_url(CsrfToken::new_random)
            .add_scopes(scopes)
            .set_pkce_challenge(pkce_challenge)
            .add_extra_param("providers", &self.providers);

        // Start a local server to receive the callback
        let bind_addr = format!("127.0.0.1:{}", CALLBACK_PORT);
        let server = Server::http(&bind_addr)
            .map_err(|e| AppError::Auth(format!("Failed to bind to {}: {}", bind_addr, e)))?;

        let (auth_url, csrf_token) = auth_request.url();
        println!("Open this URL in your browser:\n{}", auth_url);
        println!();
        println!("Waiting for authorization...");

        let request = server
            .recv()
            .map_err(|e| AppError::Auth(format!("Failed to receive request: {}", e)))?;

        let callback_url = format!("http://localhost:{}{}", CALLBACK_PORT, request.url());
        let url = Url::parse(&callback_url)
            .map_err(|e| AppError::Auth(format!("Failed to parse callback URL: {}", e)))?;

        let code_pair = url
            .query_pairs()
            .find(|(key, _)| key == "code")
            .ok_or_else(|| AppError::Auth("No code in callback".to_string()))?;

        let code = AuthorizationCode::new(code_pair.1.into_owned());

        let state_pair = url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .ok_or_else(|| AppError::Auth("No state in callback".to_string()))?;

        if state_pair.1.as_ref() != csrf_token.secret() {
            return Err(AppError::Auth("CSRF token mismatch".to_string()));
        }

        // Send success response
        let response =
            Response::from_string("Authentication successful! You can close this window.");
        request
            .respond(response)
            .map_err(|e| AppError::Auth(format!("Failed to send response: {}", e)))?;

        // Exchange the code for an access token
        let token_result = self
            .client
            .exchange_code(code)
            .set_pkce_verifier(pkce_verifier)
            .request_async(&self.http_client)
            .await
            .map_err(|e| AppError::Auth(format!("Failed to exchange code: {:?}", e)))?;

        Self::parse_and_save_tokens(token_result, None)
    }

    async fn refresh_access_token(&self, refresh_token: &str) -> Result<TrueLayerTokens> {
        let token_result = self
            .client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
            .request_async(&self.http_client)
            .await
            .map_err(|e| AppError::Auth(format!("Failed to refresh token: {:?}", e)))?;

        Self::parse_and_save_tokens(token_result, Some(refresh_token))
    }

    /// Parse token response, save to disk, and return TrueLayerTokens
    ///
    /// If `fallback_refresh_token` is provided, it will be used if the token response
    /// doesn't include a refresh token (common in refresh flows).
    fn parse_and_save_tokens(
        token_result: BasicTokenResponse,
        fallback_refresh_token: Option<&str>,
    ) -> Result<TrueLayerTokens> {
        let access_token = token_result.access_token().secret().clone();

        let refresh_token = match token_result.refresh_token() {
            Some(token) => token.secret().clone(),
            None => match fallback_refresh_token {
                Some(fallback) => fallback.to_string(),
                None => return Err(AppError::Auth("No refresh token received".to_string())),
            },
        };

        // Calculate expiry time
        let expires_in = token_result
            .expires_in()
            .map(|d| d.as_secs() as i64)
            .unwrap_or(3600); // Default to 1 hour if not provided
        let expires_at = chrono::Utc::now().timestamp() + expires_in;

        let tokens = TrueLayerTokens {
            access_token,
            refresh_token,
            expires_at,
        };

        // Save tokens to disk
        Self::save_tokens(&tokens)?;

        Ok(tokens)
    }

    fn token_cache_path() -> Result<PathBuf> {
        crate::config::Config::cache_file("truelayer_tokens.json")
    }

    fn load_tokens() -> Result<Option<TrueLayerTokens>> {
        let token_path = Self::token_cache_path()?;

        if !token_path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&token_path)
            .map_err(|e| AppError::Auth(format!("Failed to read tokens file: {}", e)))?;

        let tokens: TrueLayerTokens = serde_json::from_str(&contents)
            .map_err(|e| AppError::Auth(format!("Failed to parse tokens: {}", e)))?;

        Ok(Some(tokens))
    }

    fn save_tokens(tokens: &TrueLayerTokens) -> Result<()> {
        let token_path = Self::token_cache_path()?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = token_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AppError::Auth(format!("Failed to create token cache directory: {}", e))
            })?;
        }

        let contents = serde_json::to_string_pretty(tokens)
            .map_err(|e| AppError::Auth(format!("Failed to serialize tokens: {}", e)))?;

        // Create file with read-only permissions from the start to avoid race condition
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&token_path)
            .map_err(|e| AppError::Auth(format!("Failed to create tokens file: {}", e)))?;

        file.write_all(contents.as_bytes())
            .map_err(|e| AppError::Auth(format!("Failed to write tokens file: {}", e)))?;

        Ok(())
    }

    /// Get valid TrueLayer tokens, refreshing or re-authenticating as needed
    pub(super) async fn get_valid_tokens(&self) -> Result<TrueLayerTokens> {
        let Some(tokens) = Self::load_tokens()? else {
            debug!("No cached tokens found, authenticating with TrueLayer...");
            return self.authenticate().await;
        };

        if !tokens.is_expired() {
            debug!("Using cached TrueLayer tokens");
            return Ok(tokens);
        }

        debug!("Access token expired, refreshing...");

        match self.refresh_access_token(&tokens.refresh_token).await {
            Ok(refreshed_tokens) => {
                debug!("Token refresh successful");
                Ok(refreshed_tokens)
            }
            Err(e) => {
                debug!("Token refresh failed ({}), re-authenticating...", e);
                self.authenticate().await
            }
        }
    }
}

/// Clear cached TrueLayer tokens by deleting the token cache file
#[instrument(name = "Clearing auth tokens for TrueLayer", skip_all)]
pub fn clear_tokens() -> Result<()> {
    let token_path = TrueLayerAuth::token_cache_path()?;

    if !token_path.exists() {
        debug!("No TrueLayer tokens to clear");
        return Ok(());
    }

    fs::remove_file(&token_path)
        .map_err(|e| AppError::Auth(format!("Failed to delete tokens file: {}", e)))?;
    info!("Cleared TrueLayer cached tokens");

    Ok(())
}
