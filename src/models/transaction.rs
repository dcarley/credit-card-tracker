use crate::truelayer::types::{TrueLayerTransaction, TrueLayerTransactionType};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Transaction {
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub amount: Decimal,
    pub currency: String,
    pub type_: TransactionType,
    pub id: String,
}

impl From<TrueLayerTransaction> for Transaction {
    fn from(tl: TrueLayerTransaction) -> Self {
        Transaction {
            timestamp: tl.timestamp,
            description: tl.description,
            amount: tl.amount,
            currency: tl.currency,
            type_: tl.transaction_type.into(),
            id: tl.normalised_provider_transaction_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TransactionType {
    Debit,
    Credit,
}

impl From<TrueLayerTransactionType> for TransactionType {
    fn from(tl_type: TrueLayerTransactionType) -> Self {
        match tl_type {
            TrueLayerTransactionType::Debit => TransactionType::Debit,
            TrueLayerTransactionType::Credit => TransactionType::Credit,
        }
    }
}
