mod show;

use crate::error::Result;
use clap::{Parser, Subcommand};

pub use show::ShowResource;

#[derive(Parser, Debug)]
#[command(name = "credit-card-tracker")]
#[command(about = "Sync credit card transactions from Truelayer to Google Sheets", long_about = None)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

impl Cli {
    pub async fn run(&self) -> Result<()> {
        match &self.command {
            Commands::Show { resource } => resource.execute().await,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Show {
        #[command(subcommand)]
        resource: ShowResource,
    },
}
