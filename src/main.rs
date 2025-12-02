mod cli;
mod config;
mod error;

use clap::Parser;

use crate::cli::Cli;
use tracing::error;
use tracing_indicatif::IndicatifLayer;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let log_level = match cli.verbose {
        0 => "credit_card_tracker=info,warn", // Default: info for our app, warn for deps
        1 => "credit_card_tracker=debug,info", // -v: debug for our app, info for deps
        _ => "hyper=info,h2=info,debug",      // -vv: debug for everything, except HTTP libs
    };

    let indicatif_layer = IndicatifLayer::new();
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(indicatif_layer.get_stderr_writer()))
        .with(indicatif_layer)
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level)))
        .init();

    if let Err(e) = cli.run().await {
        error!("Error: {}", e);
        std::process::exit(1);
    }
}
