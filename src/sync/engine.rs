use crate::config::SyncConfig;
use crate::error::Result;
use crate::models::Card;
use crate::models::Transaction;
use crate::sheets::SheetOperations;
use crate::truelayer::TrueLayerOperations;
use chrono::{DateTime, Utc};
use indicatif::ProgressStyle;
use tracing::{Span, info, instrument};
use tracing_indicatif::span_ext::IndicatifSpanExt;

pub struct SyncEngine<TLC, SC> {
    config: SyncConfig,
    truelayer_client: TLC,
    sheets_client: SC,
}

impl<TLC, SC> SyncEngine<TLC, SC>
where
    TLC: TrueLayerOperations + Sync,
    SC: SheetOperations + Sync,
{
    pub fn new(config: SyncConfig, truelayer_client: TLC, sheets_client: SC) -> Self {
        Self {
            config,
            truelayer_client,
            sheets_client,
        }
    }

    #[instrument(name = "Sync", skip_all)]
    pub async fn sync(&self) -> Result<()> {
        let span = Span::current();
        span.pb_set_style(
            &ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
            )
            .map_err(|e| crate::error::AppError::Other(e.into()))?,
        );
        span.pb_set_message("Syncing cards");

        // Normalize to start of day (00:00:00 UTC) to align with API daily resolution and avoid overlaps
        let to_date = Utc::now();
        let from_date = (to_date - Duration::days(self.config.fetch_days as i64))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .ok_or_else(|| {
                crate::error::AppError::Config("Failed to calculate from_date".to_string())
            })?
            .and_utc();

        let cards = self.truelayer_client.get_cards().await?;
        if cards.is_empty() {
            return Err(crate::error::AppError::TrueLayer(
                "No cards found".to_string(),
            ));
        }

        span.pb_set_length(cards.len() as u64);
        for card in &cards {
            self.sync_card(card, from_date, to_date).await?;
            span.pb_inc(1);
        }

        Ok(())
    }

    #[instrument(name = "Syncing card", skip_all, fields(card = %card.name))]
    async fn sync_card(
        &self,
        card: &Card,
        from_date: DateTime<Utc>,
        to_date: DateTime<Utc>,
    ) -> Result<()> {
        let transactions = self
            .truelayer_client
            .get_card_transactions(&card.id, from_date, to_date)
            .await?;

        let sheet_name = &card.name;
        self.sheets_client.ensure_sheet(sheet_name).await?;

        let existing_transactions = self.sheets_client.read_sheet(sheet_name).await?;

        let mut transaction_map: std::collections::HashMap<String, Transaction> =
            existing_transactions
                .into_iter()
                .map(|t| (t.id.clone(), t))
                .collect();

        for t in transactions {
            // Upsert: Overwrite existing entry (to get latest data) or insert new one
            transaction_map.insert(t.id.clone(), t);
        }

        let mut all_transactions: Vec<Transaction> = transaction_map.into_values().collect();
        all_transactions.sort_by_key(|t| t.timestamp);

        self.sheets_client
            .write_sheet(sheet_name, &all_transactions)
            .await?;

        info!("Card synced");

        Ok(())
    }
}

#[cfg(test)]
mod mocks {
    use super::*;
    use crate::models::card::test_helpers::mock_card;
    use crate::models::{Card, Transaction};
    use async_trait::async_trait;
    use chrono::Duration;
    use std::sync::{Arc, Mutex};

    pub(crate) async fn sync_against_mocks(
        sheet_transactions: Vec<Transaction>,
        truelayer_transactions: Vec<Transaction>,
    ) -> Result<MockSheetsClient> {
        let card = mock_card();
        let truelayer_client = MockTrueLayerClient {
            cards: vec![card],
            transactions: truelayer_transactions,
        };
        let sheets_client = MockSheetsClient {
            sheet_transactions: Arc::new(Mutex::new(sheet_transactions)),
            replaced_transactions: Arc::new(Mutex::new(Vec::new())),
        };

        let engine = SyncEngine::new(
            SyncConfig::default(),
            truelayer_client,
            sheets_client.clone(),
        );
        engine
            .sync_card(&mock_card(), Utc::now() - Duration::days(30), Utc::now())
            .await?;
        Ok(sheets_client)
    }

    pub(crate) struct MockTrueLayerClient {
        pub cards: Vec<Card>,
        pub transactions: Vec<Transaction>,
    }

    #[async_trait]
    impl TrueLayerOperations for MockTrueLayerClient {
        async fn get_cards(&self) -> Result<Vec<Card>> {
            Ok(self.cards.clone())
        }

        async fn get_card_transactions(
            &self,
            _card_id: &str,
            _from: DateTime<Utc>,
            _to: DateTime<Utc>,
        ) -> Result<Vec<Transaction>> {
            Ok(self.transactions.clone())
        }
    }

    #[derive(Clone)]
    pub(crate) struct MockSheetsClient {
        pub sheet_transactions: Arc<Mutex<Vec<Transaction>>>,
        pub replaced_transactions: Arc<Mutex<Vec<Transaction>>>,
    }

    #[async_trait]
    impl SheetOperations for MockSheetsClient {
        async fn ensure_sheet(&self, _sheet_name: &str) -> Result<()> {
            Ok(())
        }

        async fn read_sheet(&self, _sheet_name: &str) -> Result<Vec<Transaction>> {
            Ok(self.sheet_transactions.lock().unwrap().clone())
        }

        async fn write_sheet(&self, _sheet_name: &str, transactions: &[Transaction]) -> Result<()> {
            let mut replaced = self.replaced_transactions.lock().unwrap();
            *replaced = transactions.to_vec();
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::transaction::TransactionType;
    use crate::models::transaction::test_helpers::{mock_datetime, mock_transaction};
    use rust_decimal::prelude::dec;

    #[tokio::test]
    async fn test_sync_updates_transactions() {
        let base_datetime = mock_datetime(2025, 1, 1);

        let tx_sheet = mock_transaction(
            "tx_sheet",
            dec!(-10.0),
            TransactionType::Debit,
            base_datetime,
        );
        let tx_truelayer = Transaction {
            description: "Updated description".to_string(),
            ..tx_sheet.clone()
        };

        let sheet_transactions = vec![tx_sheet.clone()];
        let truelayer_transactions = vec![tx_truelayer.clone()];
        let mock_sheets_client =
            mocks::sync_against_mocks(sheet_transactions, truelayer_transactions)
                .await
                .unwrap();

        let final_transactions = mock_sheets_client.replaced_transactions.lock().unwrap();

        assert_eq!(
            *final_transactions,
            vec![tx_truelayer],
            "transactions should be updated with latest data from TrueLayer"
        );
    }

    #[tokio::test]
    async fn test_sync_keeps_historical_data() {
        let base_datetime = mock_datetime(2025, 1, 1);

        let tx_sheet = mock_transaction(
            "tx_sheet",
            dec!(-10.0),
            TransactionType::Debit,
            base_datetime - Duration::days(35),
        );
        let tx_truelayer = mock_transaction(
            "tx_truelayer",
            dec!(10.0),
            TransactionType::Credit,
            base_datetime,
        );

        let sheet_transactions = vec![tx_sheet.clone()];
        let truelayer_transactions = vec![tx_truelayer.clone()];

        let mock_sheets_client =
            mocks::sync_against_mocks(sheet_transactions, truelayer_transactions)
                .await
                .unwrap();

        let final_transactions = mock_sheets_client.replaced_transactions.lock().unwrap();

        assert_eq!(
            *final_transactions,
            vec![tx_sheet, tx_truelayer],
            "historical transactions outside sync window should be preserved"
        );
    }
}
