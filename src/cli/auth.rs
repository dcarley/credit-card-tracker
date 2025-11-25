use crate::config::Config;
use crate::error::Result;
use crate::sheets::{SheetsClient, clear_sheets_tokens};
use crate::truelayer::{TrueLayerClient, clear_truelayer_tokens};
use clap::Subcommand;
use tracing::info;

#[derive(Subcommand, Debug)]
pub enum AuthProvider {
    /// Authenticate with TrueLayer
    Truelayer,

    /// Authenticate with Google Sheets
    Sheets,
}

impl AuthProvider {
    pub async fn execute(&self, reset: bool) -> Result<()> {
        match self {
            AuthProvider::Truelayer => authenticate_truelayer(reset).await,
            AuthProvider::Sheets => authenticate_sheets(reset).await,
        }
    }
}

async fn authenticate_truelayer(reset: bool) -> Result<()> {
    if reset {
        clear_truelayer_tokens()?;
    }

    let config = Config::load()?;
    let _client = TrueLayerClient::new(&config.truelayer).await?;

    info!("TrueLayer authentication verified");

    Ok(())
}

async fn authenticate_sheets(reset: bool) -> Result<()> {
    if reset {
        clear_sheets_tokens()?;
    }

    let config = Config::load()?;
    let _client = SheetsClient::new(&config.google).await?;

    info!("Google Sheets authentication verified");

    Ok(())
}
