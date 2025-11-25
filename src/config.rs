use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CONFIG_DIR_PREFIX: &str = "credit-card-tracker";

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Config {
    pub truelayer: TrueLayerConfig,
    pub google: GoogleConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TrueLayerConfig {
    pub client_id: String,
    pub client_secret: String,
}

impl TrueLayerConfig {
    /// Detect if we're using the sandbox environment based on client_id prefix
    fn is_sandbox(&self) -> bool {
        self.client_id.starts_with("sandbox-")
    }

    pub fn auth_url(&self) -> String {
        match self.is_sandbox() {
            true => "https://auth.truelayer-sandbox.com".to_string(),
            false => "https://auth.truelayer.com".to_string(),
        }
    }

    pub fn api_base_url(&self) -> String {
        match self.is_sandbox() {
            true => "https://api.truelayer-sandbox.com".to_string(),
            false => "https://api.truelayer.com".to_string(),
        }
    }

    pub fn providers(&self) -> String {
        match self.is_sandbox() {
            true => "uk-cs-mock uk-ob-all uk-oauth-all".to_string(),
            false => "uk-ob-all uk-oauth-all".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GoogleConfig {
    pub client_id: String,
    pub client_secret: String,
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_file()?;

        if !config_path.exists() {
            return Err(AppError::Config(format!(
                "Config file not found at {:?}. Please create one.",
                config_path
            )));
        }

        let contents = fs::read_to_string(&config_path)?;
        let config: Config = toml::from_str(&contents)
            .map_err(|e| AppError::Config(format!("Failed to parse config: {}", e)))?;

        if config.truelayer.client_id.is_empty() || config.truelayer.client_secret.is_empty() {
            return Err(AppError::Config(
                "TrueLayer client_id and client_secret must be set in config file".to_string(),
            ));
        }

        if config.google.client_id.is_empty() || config.google.client_secret.is_empty() {
            return Err(AppError::Config(
                "Google client_id and client_secret must be set in config file".to_string(),
            ));
        }

        Ok(config)
    }

    fn xdg_dirs() -> xdg::BaseDirectories {
        xdg::BaseDirectories::with_prefix(CONFIG_DIR_PREFIX)
    }

    /// Get the config file path
    pub fn config_file() -> Result<PathBuf> {
        let xdg_dirs = Self::xdg_dirs();
        xdg_dirs
            .place_config_file("config.toml")
            .map_err(|e| AppError::Config(format!("Failed to create config directory: {}", e)))
    }

    /// Get the cache directory path
    pub fn cache_dir() -> Result<PathBuf> {
        let xdg = Self::xdg_dirs();
        xdg.get_cache_home()
            .ok_or_else(|| AppError::Config("Failed to determine cache directory".to_string()))
    }

    /// Get a cache file path
    pub fn cache_file(filename: &str) -> Result<PathBuf> {
        let xdg = Self::xdg_dirs();
        xdg.place_cache_file(filename)
            .map_err(|e| AppError::Config(format!("Failed to create cache file path: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = Config {
            truelayer: TrueLayerConfig {
                client_id: "test_id".to_string(),
                client_secret: "test_secret".to_string(),
            },
            google: GoogleConfig {
                client_id: "test_client_id".to_string(),
                client_secret: "test_client_secret".to_string(),
            },
        };

        let serialized = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();

        assert_eq!(config.truelayer.client_id, deserialized.truelayer.client_id);
        assert_eq!(config.google.client_id, deserialized.google.client_id);
    }

    #[test]
    fn test_environment_sandbox() {
        let config = TrueLayerConfig {
            client_id: "sandbox-abc123".to_string(),
            client_secret: "secret".to_string(),
        };
        assert!(config.is_sandbox());
        assert_eq!(config.auth_url(), "https://auth.truelayer-sandbox.com");
        assert_eq!(config.api_base_url(), "https://api.truelayer-sandbox.com");
        assert_eq!(config.providers(), "uk-cs-mock uk-ob-all uk-oauth-all");
    }

    #[test]
    fn test_environment_live() {
        let config = TrueLayerConfig {
            client_id: "live-abc123".to_string(),
            client_secret: "secret".to_string(),
        };
        assert!(!config.is_sandbox());
        assert_eq!(config.auth_url(), "https://auth.truelayer.com");
        assert_eq!(config.api_base_url(), "https://api.truelayer.com");
        assert_eq!(config.providers(), "uk-ob-all uk-oauth-all");
    }
}
