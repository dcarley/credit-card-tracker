use crate::config::Config;
use crate::error::Result;
use crate::truelayer::{TrueLayerClient, TrueLayerOperations};
use clap::Subcommand;
use tracing::info;

#[derive(Subcommand, Debug)]
pub enum ShowResource {
    /// Show available TrueLayer cards
    Cards,

    /// Show configuration and cache paths
    Paths,
}

impl ShowResource {
    pub async fn execute(&self) -> Result<()> {
        match self {
            ShowResource::Cards => show_cards().await,
            ShowResource::Paths => show_paths(),
        }
    }
}

async fn show_cards() -> Result<()> {
    let config = Config::load()?;
    let client = TrueLayerClient::new(&config.truelayer).await?;
    let cards = client.get_cards().await?;

    for card in cards {
        info!(id = card.id, provider = ?card.provider, "{}", card.name);
    }

    Ok(())
}

fn show_paths() -> Result<()> {
    let config_path = Config::config_file()?;
    let cache_dir = Config::cache_dir()?;

    info!(path = ?config_path, "Config path");
    info!(path = ?cache_dir, "Cache path");

    Ok(())
}
