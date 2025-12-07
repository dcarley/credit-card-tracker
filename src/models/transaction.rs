use crate::{
    error::AppError,
    truelayer::types::{TrueLayerTransaction, TrueLayerTransactionType},
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::dec;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

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
    #[serde(default)]
    pub comments: Option<String>,
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
            comments: None,
        }
    }
}

impl Transaction {
    /// Derive the CSV headers from the struct definition by serializing a dummy instance.
    /// https://github.com/BurntSushi/rust-csv/issues/161
    fn get_field_names() -> Vec<String> {
        let mut writer = csv::WriterBuilder::new()
            .has_headers(true)
            .from_writer(Vec::new());

        let dummy = Transaction {
            timestamp: Utc::now(),
            description: String::new(),
            amount: dec!(0),
            currency: String::new(),
            type_: TransactionType::Debit,
            id: String::new(),
            matched_id: None,
            comments: None,
        };

        // Serialize to write headers
        // We unwrap because memory writing shouldn't fail for this struct
        writer.serialize(&dummy).unwrap();

        let data = String::from_utf8(writer.into_inner().unwrap_or_default()).unwrap_or_default();
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(data.as_bytes());

        reader
            .headers()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default()
    }

    /// Get the column index (0-based) for a specific field name.
    pub fn get_column_index(field_name: &str) -> Option<usize> {
        Self::get_field_names()
            .iter()
            .position(|name| name == field_name)
    }

    /// Get the column letter (A-based) for a specific field name.
    pub fn get_column_letter(field_name: &str) -> Option<String> {
        Self::get_column_index(field_name).map(Self::index_to_column_letter)
    }

    fn index_to_column_letter(col_idx: usize) -> String {
        let remainder = col_idx % 26;
        let char = (b'A' + remainder as u8) as char;

        if col_idx < 26 {
            return char.to_string();
        }

        let parent = (col_idx / 26) - 1;
        format!("{}{}", Self::index_to_column_letter(parent), char)
    }

    fn value_to_string(v: &Value) -> String {
        match v {
            Value::String(s) => s.clone(),
            _ => v.to_string(),
        }
    }

    fn normalize_sheet_value(v: &Value) -> Value {
        match v {
            Value::String(s) if s.is_empty() => Value::Null,
            _ => v.clone(),
        }
    }
}

pub trait FromSheetRows: Sized {
    /// Convert a vector of rows (first row as headers) to a list of transactions.
    fn from_sheet_rows(rows: &[Vec<Value>]) -> crate::error::Result<Vec<Self>>;
}

pub trait ToSheetRows {
    /// Convert a list of transactions to a vector of rows (values), always including headers.
    fn to_sheet_rows(&self) -> crate::error::Result<Vec<Vec<Value>>>;
}

impl FromSheetRows for Transaction {
    fn from_sheet_rows(rows: &[Vec<Value>]) -> crate::error::Result<Vec<Self>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // Get headers from the first row
        let headers: Vec<String> = rows[0].iter().map(Self::value_to_string).collect();

        rows.iter()
            .enumerate()
            .skip(1)
            .map(|(idx, row)| {
                let map: Map<String, Value> = headers
                    .iter()
                    .zip(row.iter())
                    .map(|(header, value)| (header.clone(), Self::normalize_sheet_value(value)))
                    .collect();

                serde_json::from_value(Value::Object(map)).map_err(|e| {
                    AppError::Sheets(format!("Failed to parse row {}: {}", idx + 1, e))
                })
            })
            .collect()
    }
}

impl ToSheetRows for [Transaction] {
    fn to_sheet_rows(&self) -> crate::error::Result<Vec<Vec<Value>>> {
        let headers = Transaction::get_field_names();

        let header_row: Vec<Value> = headers.iter().map(|h| Value::String(h.clone())).collect();

        let data_rows: Vec<Vec<Value>> = self
            .iter()
            .enumerate()
            .map(|(idx, t)| {
                let obj = serde_json::to_value(t).map_err(|e| {
                    AppError::Sheets(format!("Failed to serialize row {}: {}", idx, e))
                })?;

                Ok(headers
                    .iter()
                    .map(|header| obj.get(header).cloned().unwrap_or(Value::Null))
                    .collect())
            })
            .collect::<crate::error::Result<_>>()?;

        let mut rows = vec![header_row];
        rows.extend(data_rows);

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
            comments: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::prelude::dec;
    use serde_json::json;

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
                json!("Timestamp"),
                json!("Description"),
                json!("Amount"),
                json!("Currency"),
                json!("Type"),
                json!("ID"),
                json!("Matched ID"),
                json!("Comments"),
            ],
            vec![
                json!("2024-11-23T10:00:00Z"),
                json!("mock transaction: tx_123"),
                json!("-12.34"), // rust_decimal serializes to string by default
                json!("GBP"),
                json!("Debit"),
                json!("tx_123"),
                Value::Null, // Option::None serializes to null
                Value::Null, // Option::None serializes to null
            ],
        ];
        assert_eq!(rows, expected);
    }

    #[test]
    fn test_to_sheet_rows_empty() {
        let transactions = vec![];
        let rows = transactions.as_slice().to_sheet_rows().unwrap();
        let expected = vec![vec![
            json!("Timestamp"),
            json!("Description"),
            json!("Amount"),
            json!("Currency"),
            json!("Type"),
            json!("ID"),
            json!("Matched ID"),
            json!("Comments"),
        ]];
        assert_eq!(rows, expected);
    }

    #[test]
    fn test_from_sheet_rows_with_data() {
        let rows = vec![
            vec![
                json!("Timestamp"),
                json!("Description"),
                json!("Amount"),
                json!("Currency"),
                json!("Type"),
                json!("ID"),
                json!("Matched ID"),
                json!("Comments"),
            ],
            vec![
                json!("2024-11-23T10:00:00Z"),
                json!("mock transaction: tx_123"),
                json!("-12.34"),
                json!("GBP"),
                json!("Debit"),
                json!("tx_123"),
                json!(""),
                json!(""),
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
            comments: None,
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
    fn test_from_sheet_rows_with_matched_id_and_comments() {
        let rows = vec![
            vec![
                json!("Timestamp"),
                json!("Description"),
                json!("Amount"),
                json!("Currency"),
                json!("Type"),
                json!("ID"),
                json!("Matched ID"),
                json!("Comments"),
            ],
            vec![
                json!("2024-11-23T10:00:00Z"),
                json!("Test transaction"),
                json!("100.00"),
                json!("GBP"),
                json!("Credit"),
                json!("tx_123"),
                json!("tx_456"),
                json!("Manually added comment"),
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
            comments: Some("Manually added comment".to_string()),
        }];
        assert_eq!(transactions, expected);
    }

    #[test]
    fn test_from_sheet_rows_headers_only() {
        let rows = vec![vec![
            json!("Timestamp"),
            json!("Description"),
            json!("Amount"),
            json!("Currency"),
            json!("Type"),
            json!("ID"),
            json!("Matched ID"),
            json!("Comments"),
        ]];

        let transactions = Transaction::from_sheet_rows(&rows).unwrap();
        let expected = vec![];
        assert_eq!(transactions, expected);
    }

    #[test]
    fn test_get_column_letter() {
        assert_eq!(
            Transaction::get_column_letter("Timestamp"),
            Some("A".to_string())
        );
        assert_eq!(
            Transaction::get_column_letter("Description"),
            Some("B".to_string())
        );
        assert_eq!(Transaction::get_column_letter("Unknown"), None);
    }

    #[test]
    fn test_index_to_column_letter() {
        assert_eq!(Transaction::index_to_column_letter(0), "A");
        assert_eq!(Transaction::index_to_column_letter(25), "Z");
        assert_eq!(Transaction::index_to_column_letter(26), "AA");
        assert_eq!(Transaction::index_to_column_letter(27), "AB");
        assert_eq!(Transaction::index_to_column_letter(51), "AZ");
        assert_eq!(Transaction::index_to_column_letter(52), "BA");
        assert_eq!(Transaction::index_to_column_letter(701), "ZZ");
        assert_eq!(Transaction::index_to_column_letter(702), "AAA");
    }
}
