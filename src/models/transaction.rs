use crate::{
    error::AppError,
    truelayer::types::{TrueLayerTransaction, TrueLayerTransactionType},
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::dec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct Transaction {
    pub timestamp: DateTime<Utc>,
    pub description: String,
    pub amount: Decimal,
    pub currency: String,
    pub type_: TransactionType,
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Matched ID", default)]
    pub matched_id: Option<String>,
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
            matched_id: None,
        }
    }
}

pub trait FromSheetRows: Sized {
    /// Convert a vector of rows (first row as headers) to a list of transactions.
    fn from_sheet_rows(rows: &[Vec<String>]) -> crate::error::Result<Vec<Self>>;
}

pub trait ToSheetRows {
    /// Convert a list of transactions to a vector of rows (strings), always including headers.
    fn to_sheet_rows(&self) -> crate::error::Result<Vec<Vec<String>>>;
}

impl FromSheetRows for Transaction {
    fn from_sheet_rows(rows: &[Vec<String>]) -> crate::error::Result<Vec<Self>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // Use the first row as headers for deserialization
        let headers_row = &rows[0];
        let headers = csv::StringRecord::from(headers_row.clone());

        let mut transactions = Vec::new();

        for (idx, row) in rows.iter().enumerate().skip(1) {
            // Pad row with empty strings if needed
            let mut row_vec = row.clone();
            while row_vec.len() < headers.len() {
                row_vec.push(String::new());
            }

            let record = csv::StringRecord::from(row_vec);
            let transaction: Transaction = record
                .deserialize(Some(&headers))
                .map_err(|e| AppError::Sheets(format!("Failed to parse row {}: {}", idx + 1, e)))?;

            transactions.push(transaction);
        }

        Ok(transactions)
    }
}

impl ToSheetRows for [Transaction] {
    fn to_sheet_rows(&self) -> crate::error::Result<Vec<Vec<String>>> {
        let mut writer = csv::WriterBuilder::new()
            .has_headers(true)
            .from_writer(vec![]);

        // Serialize all transactions, or a dummy if empty to get headers
        // https://github.com/BurntSushi/rust-csv/issues/161
        if self.is_empty() {
            let dummy = Transaction {
                timestamp: Utc::now(),
                description: String::new(),
                currency: String::new(),
                amount: dec!(0),
                type_: TransactionType::Debit,
                id: String::new(),
                matched_id: None,
            };
            writer
                .serialize(&dummy)
                .map_err(|e| AppError::Sheets(format!("Failed to serialize: {}", e)))?;
        } else {
            for t in self {
                writer
                    .serialize(t)
                    .map_err(|e| AppError::Sheets(format!("Failed to serialize: {}", e)))?;
            }
        }

        let data = String::from_utf8(
            writer
                .into_inner()
                .map_err(|e| AppError::Sheets(format!("Failed to get CSV data: {}", e)))?,
        )
        .map_err(|e| AppError::Sheets(format!("Invalid UTF-8: {}", e)))?;

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true) // Separates headers from data
            .from_reader(data.as_bytes());

        let mut rows = Vec::new();

        // Add headers
        let headers = reader
            .headers()
            .map_err(|e| AppError::Sheets(format!("Failed to read headers: {}", e)))?;
        rows.push(headers.iter().map(|s| s.to_string()).collect());

        // Add data rows only if we had real data
        if !self.is_empty() {
            for result in reader.records() {
                let record = result
                    .map_err(|e| AppError::Sheets(format!("Failed to read CSV record: {}", e)))?;
                rows.push(record.iter().map(|s| s.to_string()).collect());
            }
        }

        Ok(rows)
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

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;
    use chrono::Utc;
    use chrono::{DateTime, TimeZone};

    pub(crate) fn mock_datetime(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 10, 0, 0).unwrap()
    }

    pub(crate) fn mock_transaction(
        id: &str,
        amount: Decimal,
        type_: TransactionType,
        timestamp: DateTime<Utc>,
    ) -> Transaction {
        Transaction {
            timestamp,
            description: format!("mock transaction: {id}"),
            currency: "GBP".to_string(),
            amount,
            type_,
            id: id.to_string(),
            matched_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::prelude::dec;

    #[test]
    fn test_to_sheet_rows_with_data() {
        let transaction = test_helpers::mock_transaction(
            "tx_123",
            dec!(-12.34),
            TransactionType::Debit,
            test_helpers::mock_datetime(2024, 11, 23),
        );
        let transactions = vec![transaction];
        let rows = transactions.as_slice().to_sheet_rows().unwrap();
        let expected = vec![
            vec![
                "Timestamp",
                "Description",
                "Amount",
                "Currency",
                "Type",
                "ID",
                "Matched ID",
            ],
            vec![
                "2024-11-23T10:00:00Z",
                "mock transaction: tx_123",
                "-12.34",
                "GBP",
                "Debit",
                "tx_123",
                "",
            ],
        ];
        assert_eq!(rows, expected);
    }

    #[test]
    fn test_to_sheet_rows_empty() {
        let transactions = vec![];
        let rows = transactions.as_slice().to_sheet_rows().unwrap();
        let expected = vec![vec![
            "Timestamp",
            "Description",
            "Amount",
            "Currency",
            "Type",
            "ID",
            "Matched ID",
        ]];
        assert_eq!(rows, expected);
    }

    #[test]
    fn test_from_sheet_rows_with_data() {
        let rows = vec![
            vec![
                "Timestamp".to_string(),
                "Description".to_string(),
                "Amount".to_string(),
                "Currency".to_string(),
                "Type".to_string(),
                "ID".to_string(),
                "Matched ID".to_string(),
            ],
            vec![
                "2024-11-23T10:00:00Z".to_string(),
                "mock transaction: tx_123".to_string(),
                "-12.34".to_string(),
                "GBP".to_string(),
                "Debit".to_string(),
                "tx_123".to_string(),
                "".to_string(),
            ],
        ];

        let transactions = Transaction::from_sheet_rows(&rows).unwrap();
        let expected = vec![Transaction {
            timestamp: test_helpers::mock_datetime(2024, 11, 23),
            description: "mock transaction: tx_123".to_string(),
            currency: "GBP".to_string(),
            amount: dec!(-12.34),
            type_: TransactionType::Debit,
            id: "tx_123".to_string(),
            matched_id: None,
        }];
        assert_eq!(transactions, expected);
    }

    #[test]
    fn test_from_sheet_rows_empty() {
        let rows = vec![];
        let transactions = Transaction::from_sheet_rows(&rows).unwrap();
        let expected = vec![];
        assert_eq!(transactions, expected);
    }

    #[test]
    fn test_from_sheet_rows_with_matched_id() {
        let rows = vec![
            vec![
                "Timestamp".to_string(),
                "Description".to_string(),
                "Amount".to_string(),
                "Currency".to_string(),
                "Type".to_string(),
                "ID".to_string(),
                "Matched ID".to_string(),
            ],
            vec![
                "2024-11-23T10:00:00Z".to_string(),
                "Test transaction".to_string(),
                "100.00".to_string(),
                "GBP".to_string(),
                "Credit".to_string(),
                "tx_123".to_string(),
                "tx_456".to_string(),
            ],
        ];

        let transactions = Transaction::from_sheet_rows(&rows).unwrap();
        let expected = vec![Transaction {
            timestamp: test_helpers::mock_datetime(2024, 11, 23),
            description: "Test transaction".to_string(),
            currency: "GBP".to_string(),
            amount: dec!(100.00),
            type_: TransactionType::Credit,
            id: "tx_123".to_string(),
            matched_id: Some("tx_456".to_string()),
        }];
        assert_eq!(transactions, expected);
    }

    #[test]
    fn test_from_sheet_rows_headers_only() {
        let rows = vec![vec![
            "Timestamp".to_string(),
            "Description".to_string(),
            "Amount".to_string(),
            "Currency".to_string(),
            "Type".to_string(),
            "ID".to_string(),
            "Matched ID".to_string(),
        ]];

        let transactions = Transaction::from_sheet_rows(&rows).unwrap();
        let expected = vec![];
        assert_eq!(transactions, expected);
    }
}
