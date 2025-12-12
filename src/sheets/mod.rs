mod auth;
mod client;
mod formatting;

pub use client::SheetsClient;

// Re-export clear_tokens for CLI usage
pub use auth::clear_tokens as clear_sheets_tokens;

use crate::error::Result;
use crate::models::Transaction;
use async_trait::async_trait;
use google_sheets4::api::Sheet;

#[async_trait]
pub trait SheetOperations {
    async fn ensure_sheet(&self, sheet_name: &str) -> Result<Sheet>;

    async fn read_sheet(&self, sheet_name: &str) -> Result<Vec<Transaction>>;

    async fn write_sheet(
        &self,
        sheet: &Sheet,
        sheet_name: &str,
        transactions: &[Transaction],
    ) -> Result<()>;
}
