use super::TrueLayerOperations;
use crate::config::TrueLayerConfig;
use crate::error::{AppError, Result};
use crate::models::{Card, Transaction};
use crate::truelayer::auth::TrueLayerAuth;
use crate::truelayer::types::{CardsResponse, TransactionsResponse};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use tracing::instrument;

pub struct TrueLayerClient {
    client: Client,
    access_token: String,
    api_base_url: String,
}

impl TrueLayerClient {
    /// Create a new TrueLayerClient with authenticated access
    ///
    /// This will automatically handle token validation, refresh, or interactive
    /// authentication as needed.
    #[instrument(name = "Authenticating to TrueLayer", skip_all)]
    pub async fn new(config: &TrueLayerConfig) -> Result<Self> {
        let auth = TrueLayerAuth::new(config)?;
        let tokens = auth.get_valid_tokens().await?;
        let api_base_url = config.api_base_url();

        Ok(Self {
            client: auth.http_client(),
            access_token: tokens.access_token,
            api_base_url,
        })
    }
}

#[async_trait]
impl TrueLayerOperations for TrueLayerClient {
    #[instrument(name = "Fetching cards", skip_all)]
    async fn get_cards(&self) -> Result<Vec<Card>> {
        let url = format!("{}/data/v1/cards", self.api_base_url);

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::TrueLayer(format!(
                "Failed to list cards: {} - {}",
                status, body
            )));
        }

        let cards: CardsResponse = response.json().await?;

        Ok(cards.results.into_iter().map(Into::into).collect())
    }

    #[instrument(name = "Fetching card transactions", skip_all, fields(card_id))]
    async fn get_card_transactions(
        &self,
        card_id: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<Transaction>> {
        let url = format!(
            "{}/data/v1/cards/{}/transactions",
            self.api_base_url, card_id
        );

        let from_str = from.format("%Y-%m-%d").to_string();
        let to_str = to.format("%Y-%m-%d").to_string();

        let response = self
            .client
            .get(&url)
            .bearer_auth(&self.access_token)
            .query(&[("from", from_str), ("to", to_str)])
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::TrueLayer(format!(
                "Failed to get card transactions: {} - {}",
                status, body
            )));
        }

        let transactions: TransactionsResponse = response.json().await?;

        Ok(transactions.results.into_iter().map(Into::into).collect())
    }
}
