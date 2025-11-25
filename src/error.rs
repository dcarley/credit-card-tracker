use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("TrueLayer API error: {0}")]
    TrueLayer(String),

    #[error("OAuth2 authentication error: {0}")]
    Auth(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, AppError>;
