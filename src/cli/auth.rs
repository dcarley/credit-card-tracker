use crate::config::Config;
use crate::error::Result;
use crate::truelayer::{TrueLayerClient, clear_truelayer_tokens};
use clap::Subcommand;
use tracing::info;

#[derive(Subcommand, Debug)]
pub enum AuthProvider {
    /// Authenticate with TrueLayer
    Truelayer,
}

impl AuthProvider {
    pub async fn execute(&self, reset: bool) -> Result<()> {
        match self {
            AuthProvider::Truelayer => authenticate_truelayer(reset).await,
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
