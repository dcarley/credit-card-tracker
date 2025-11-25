mod auth;
mod show;
mod sync;

use crate::error::Result;
use clap::{Parser, Subcommand};

pub use auth::AuthProvider;
pub use show::ShowResource;

#[derive(Parser, Debug)]
#[command(name = "credit-card-tracker")]
#[command(about = "Sync credit card transactions from TrueLayer to Google Sheets", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Verbose mode (-v for info, -vv for debug)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    pub async fn run(&self) -> Result<()> {
        match &self.command {
            Commands::Auth { provider, reset } => provider.execute(*reset).await,
            Commands::Sync => sync::execute().await,
            Commands::Show { resource } => resource.execute().await,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Authenticate with providers
    Auth {
        #[command(subcommand)]
        provider: AuthProvider,

        /// Clear cached tokens before authenticating
        #[arg(short, long, global = true)]
        reset: bool,
    },

    /// Sync transactions from TrueLayer to Google Sheets
    Sync,

    /// Show resources
    Show {
        #[command(subcommand)]
        resource: ShowResource,
    },
}
