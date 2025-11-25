use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct CardsResponse {
    pub(super) results: Vec<TrueLayerCard>,
}

// https://docs.truelayer.com/docs/card-data-requests#get-data-for-all-cards
// https://docs.truelayer.com/reference/getcards
#[derive(Debug, Deserialize)]
pub struct TrueLayerCard {
    pub account_id: String,
    pub display_name: String,
    pub provider: TrueLayerProvider,
}

#[derive(Debug, Deserialize)]
pub struct TrueLayerProvider {
    pub provider_id: String,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct TransactionsResponse {
    pub(super) results: Vec<TrueLayerTransaction>,
}

// https://docs.truelayer.com/docs/card-data-requests#get-transaction-data-for-a-card
// https://docs.truelayer.com/reference/getcardtransactions
#[derive(Debug, Deserialize)]
pub struct TrueLayerTransaction {
    // https://support.truelayer.com/hc/en-us/articles/360025889254-Why-are-transaction-ids-subject-to-change
    // pub transaction_id: String,
    // https://support.truelayer.com/hc/en-us/articles/8338166637201-What-is-normalised-provider-transaction-id
    // pub provider_transaction_id: String,
    pub normalised_provider_transaction_id: String,
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub transaction_type: TrueLayerTransactionType,
    pub amount: Decimal,
    pub currency: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrueLayerTransactionType {
    Debit,
    Credit,
}
