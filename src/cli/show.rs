use crate::config::Config;
use crate::error::Result;
use clap::Subcommand;
use tracing::info;

#[derive(Subcommand, Debug)]
pub enum ShowResource {
    /// Show configuration and cache paths
    Paths,
}

impl ShowResource {
    pub async fn execute(&self) -> Result<()> {
        match self {
            ShowResource::Paths => show_paths(),
        }
    }
}

fn show_paths() -> Result<()> {
    let config_path = Config::config_file()?;
    let cache_dir = Config::cache_dir()?;

    info!(path = ?config_path, "Config path");
    info!(path = ?cache_dir, "Cache path");

    Ok(())
}
