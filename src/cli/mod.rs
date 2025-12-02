mod show;

use crate::error::Result;
use clap::{Parser, Subcommand};

pub use show::ShowResource;

#[derive(Parser, Debug)]
#[command(name = "credit-card-tracker")]
#[command(about = "Sync credit card transactions from Truelayer to Google Sheets", long_about = None)]
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
