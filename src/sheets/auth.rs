use crate::config::{Config, GoogleConfig};
use crate::error::{AppError, Result};
use crate::sheets::client::AUTH_SCOPE;
use hyper_util::client::legacy::connect::HttpConnector;
use std::fs;
use std::path::PathBuf;
use tracing::debug;
use tracing::instrument;
use yup_oauth2::{
    ApplicationSecret, InstalledFlowAuthenticator, InstalledFlowReturnMethod,
    authenticator::Authenticator, hyper_rustls::HttpsConnector,
};

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_CERT_URL: &str = "https://www.googleapis.com/oauth2/v1/certs";
pub(crate) const GOOGLE_REDIRECT_URI: &str = "urn:ietf:wg:oauth:2.0:oob";

type AuthType = Authenticator<HttpsConnector<HttpConnector>>;

/// Create and verify authenticator by fetching a token
pub(super) async fn create_and_verify_authenticator(config: &GoogleConfig) -> Result<AuthType> {
    let auth = from_installed_flow(config.client_id.clone(), config.client_secret.clone()).await?;

    // Trigger authentication by requesting a token
    let _token = auth
        .token(&[AUTH_SCOPE])
        .await
        .map_err(|e| AppError::Auth(format!("Failed to get token: {}", e)))?;

    Ok(auth)
}

async fn from_installed_flow(client_id: String, client_secret: String) -> Result<AuthType> {
    // Build the OAuth application secret from config values
    let secret = ApplicationSecret {
        client_id,
        client_secret,
        auth_uri: GOOGLE_AUTH_URL.to_string(),
        token_uri: GOOGLE_TOKEN_URL.to_string(),
        auth_provider_x509_cert_url: Some(GOOGLE_CERT_URL.to_string()),
        redirect_uris: vec![GOOGLE_REDIRECT_URI.to_string()],
        project_id: None,
        client_email: None,
        client_x509_cert_url: None,
    };

    // Determine token cache path (stored in same directory as config)
    let token_cache_path = token_cache_path()?;

    // Create parent directory if it doesn't exist
    if let Some(parent) = token_cache_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::Auth(format!("Failed to create token cache directory: {}", e))
        })?;
    }

    // Build the authenticator with installed flow (interactive mode)
    // User will copy/paste the authorization code from the browser
    let auth = InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::Interactive)
        .persist_tokens_to_disk(token_cache_path)
        .build()
        .await
        .map_err(|e| AppError::Auth(format!("Failed to build authenticator: {}", e)))?;

    Ok(auth)
}

/// Clear cached Google tokens by deleting the token cache file
#[instrument(name = "Clearing auth tokens for Google Sheets", skip_all)]
pub fn clear_tokens() -> Result<()> {
    let token_path = token_cache_path()?;

    if !token_path.exists() {
        debug!("No Google Sheets tokens to clear");
        return Ok(());
    }

    fs::remove_file(&token_path)
        .map_err(|e| AppError::Auth(format!("Failed to delete tokens file: {}", e)))?;
    debug!("Cleared Google Sheets cached tokens");

    Ok(())
}

fn token_cache_path() -> Result<PathBuf> {
    Config::cache_file("google_tokens.json")
}
