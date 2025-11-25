mod cli;
mod config;
mod error;

use clap::Parser;

use crate::cli::Cli;
use tracing::error;
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() {
    // Initialize logging
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    if let Err(e) = cli.run().await {
        error!("Error: {}", e);
        std::process::exit(1);
    }
}
