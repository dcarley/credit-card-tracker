use crate::config::Config;
use crate::error::Result;
use crate::sheets::SheetsClient;
use crate::truelayer::{TrueLayerClient, TrueLayerOperations};
use clap::Subcommand;
use tracing::info;

#[derive(Subcommand, Debug)]
pub enum ShowResource {
    /// Show available TrueLayer cards
    Cards,

    /// Show the Google spreadsheet
    Sheets,

    /// Show configuration and cache paths
    Paths,
}

impl ShowResource {
    pub async fn execute(&self) -> Result<()> {
        match self {
            ShowResource::Cards => show_cards().await,
            ShowResource::Sheets => show_sheets().await,
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

async fn show_sheets() -> Result<()> {
    let config = Config::load()?;
    let sheets_client = SheetsClient::new(&config.google).await?;

    info!(url = sheets_client.spreadsheet_url(), "Spreadsheet");

    Ok(())
}

fn show_paths() -> Result<()> {
    let config_path = Config::config_file()?;
    let cache_dir = Config::cache_dir()?;

    info!(path = ?config_path, "Config path");
    info!(path = ?cache_dir, "Cache path");

    Ok(())
}
