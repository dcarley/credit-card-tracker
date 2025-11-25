use crate::config::Config;
use crate::error::Result;
use crate::sheets::SheetsClient;
use crate::sync::SyncEngine;
use crate::truelayer::TrueLayerClient;
use tracing::info;

pub async fn execute() -> Result<()> {
    let config = Config::load()?;
    let truelayer_client = TrueLayerClient::new(&config.truelayer).await?;
    let sheets_client = SheetsClient::new(&config.google).await?;
    let url = sheets_client.spreadsheet_url();

    let engine = SyncEngine::new(config.sync, truelayer_client, sheets_client);
    engine.sync().await?;

    info!(url = url, "Sync completed");

    Ok(())
}
