mod auth;
mod client;
pub mod types;
pub use auth::clear_tokens as clear_truelayer_tokens;
pub use client::TrueLayerClient;

use crate::error::Result;
use crate::models::{Card, Transaction};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

#[async_trait]
pub trait TrueLayerOperations {
    async fn get_cards(&self) -> Result<Vec<Card>>;

    async fn get_card_transactions(
        &self,
        card_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<Transaction>>;
}
