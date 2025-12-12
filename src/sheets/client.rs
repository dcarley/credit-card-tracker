use super::SheetOperations;
use super::formatting::{bold_header_rule, freeze_header_rule, highlight_rules};
use crate::config::GoogleConfig;
use crate::error::{AppError, Result};
use crate::models::{FromSheetRows, ToSheetRows, Transaction};
use crate::sheets::auth::create_and_verify_authenticator;
use async_trait::async_trait;
use google_drive3::api::DriveHub;
use google_sheets4::api::{
    AddSheetRequest, BatchUpdateSpreadsheetRequest, ClearValuesRequest, Request, Scope, Sheet,
    SheetProperties, Sheets, Spreadsheet, SpreadsheetProperties, ValueRange,
};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use tracing::{debug, instrument};

// Access to files created or opened by the app
pub(crate) const AUTH_SCOPE: Scope = Scope::DriveFile;

// Name of the spreadsheet file in Google Drive.
const SPREADSHEET_NAME: &str = "Credit Card Transactions (credit-card-tracker)";

pub struct SheetsClient {
    hub: Sheets<HttpsConnector<HttpConnector>>,
    spreadsheet_id: String,
    spreadsheet_url: String,
}

impl SheetsClient {
    /// Create a new SheetsClient with authenticated access
    #[instrument(name = "Authenticating to Google Sheets", skip_all)]
    pub async fn new(config: &GoogleConfig) -> Result<Self> {
        let auth = create_and_verify_authenticator(config).await?;

        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .unwrap()
            .https_or_http()
            .enable_http1()
            .build();

        let client = Client::builder(hyper_util::rt::TokioExecutor::new()).build(connector);

        let sheets_hub = Sheets::new(client.clone(), auth.clone());
        let drive_hub = DriveHub::new(client, auth);

        let (spreadsheet_id, spreadsheet_url) =
            Self::get_or_create_spreadsheet(&sheets_hub, &drive_hub).await?;

        Ok(Self {
            hub: sheets_hub,
            spreadsheet_id,
            spreadsheet_url,
        })
    }

    pub fn spreadsheet_url(&self) -> String {
        self.spreadsheet_url.to_string()
    }

    async fn get_or_create_spreadsheet(
        sheets: &Sheets<HttpsConnector<HttpConnector>>,
        drive: &DriveHub<HttpsConnector<HttpConnector>>,
    ) -> Result<(String, String)> {
        if let Some(id) = Self::search_spreadsheet_by_name(drive, SPREADSHEET_NAME).await? {
            let url = format!("https://docs.google.com/spreadsheets/d/{}", id);
            return Ok((id, url));
        }

        Self::create_new_spreadsheet(sheets, SPREADSHEET_NAME).await
    }

    #[instrument(name = "Finding existing spreadsheet", skip(drive))]
    async fn search_spreadsheet_by_name(
        drive: &DriveHub<HttpsConnector<HttpConnector>>,
        name: &str,
    ) -> Result<Option<String>> {
        let query = format!(
            "name='{}' and mimeType='application/vnd.google-apps.spreadsheet' and trashed=false",
            name
        );

        let (_, file_list) = drive
            .files()
            .list()
            .q(&query)
            .spaces("drive")
            .page_size(1)
            .add_scope(AUTH_SCOPE)
            .doit()
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to search spreadsheet: {}", e)))?;

        let spreadsheet_id = file_list
            .files
            .and_then(|files| files.into_iter().next())
            .map(|file| file.id.unwrap_or_default());

        Ok(spreadsheet_id)
    }

    #[instrument(name = "Creating new spreadsheet", skip(sheets))]
    async fn create_new_spreadsheet(
        sheets: &Sheets<HttpsConnector<HttpConnector>>,
        name: &str,
    ) -> Result<(String, String)> {
        let spreadsheet = Spreadsheet {
            properties: Some(SpreadsheetProperties {
                title: Some(name.to_string()),
                time_zone: Some("UTC".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let (_, result) = sheets
            .spreadsheets()
            .create(spreadsheet)
            .add_scope(AUTH_SCOPE)
            .doit()
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to create spreadsheet: {}", e)))?;

        let spreadsheet_id = result
            .spreadsheet_id
            .ok_or_else(|| AppError::Sheets("Created spreadsheet has empty ID".to_string()))?;

        let spreadsheet_url = result
            .spreadsheet_url
            .ok_or_else(|| AppError::Sheets("Created spreadsheet has empty URL".to_string()))?;

        Ok((spreadsheet_id, spreadsheet_url))
    }

    async fn get_sheet(&self, sheet_name: &str) -> Result<Option<Sheet>> {
        let (_, spreadsheet) = self
            .hub
            .spreadsheets()
            .get(&self.spreadsheet_id)
            .include_grid_data(false)
            .add_scope(AUTH_SCOPE)
            .doit()
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to get spreadsheet: {}", e)))?;

        let sheet = spreadsheet
            .sheets
            .unwrap_or_default()
            .into_iter()
            .find(|sheet| {
                sheet
                    .properties
                    .as_ref()
                    .map(|props| props.title.as_deref() == Some(sheet_name))
                    .unwrap_or(false)
            });

        Ok(sheet)
    }

    async fn create_sheet(&self, sheet_name: &str) -> Result<Sheet> {
        let request = Request {
            add_sheet: Some(AddSheetRequest {
                properties: Some(SheetProperties {
                    title: Some(sheet_name.to_string()),
                    sheet_type: Some("GRID".to_string()),
                    ..Default::default()
                }),
            }),
            ..Default::default()
        };

        let batch_update = BatchUpdateSpreadsheetRequest {
            requests: Some(vec![request]),
            include_spreadsheet_in_response: Some(true),
            response_include_grid_data: Some(false),
            ..Default::default()
        };

        let (_, response) = self
            .hub
            .spreadsheets()
            .batch_update(batch_update, &self.spreadsheet_id)
            .add_scope(AUTH_SCOPE)
            .doit()
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to create sheet: {}", e)))?;

        let sheet_properties = response
            .replies
            .and_then(|replies| replies.into_iter().next())
            .and_then(|reply| reply.add_sheet)
            .and_then(|add_sheet| add_sheet.properties)
            .ok_or_else(|| {
                AppError::Sheets("Failed to get sheet properties from create response".to_string())
            })?;

        Ok(Sheet {
            properties: Some(sheet_properties),
            ..Default::default()
        })
    }

    async fn apply_formatting(&self, sheet: &Sheet) -> Result<()> {
        let sheet_id = sheet
            .properties
            .as_ref()
            .and_then(|p| p.sheet_id)
            .ok_or_else(|| AppError::Sheets("Sheet ID not found".to_string()))?;

        let mut requests = Vec::new();
        requests.push(bold_header_rule(sheet_id));
        requests.push(freeze_header_rule(sheet_id));
        requests.extend(highlight_rules(sheet_id, sheet)?);

        let batch_update = BatchUpdateSpreadsheetRequest {
            requests: Some(requests),
            ..Default::default()
        };

        self.hub
            .spreadsheets()
            .batch_update(batch_update, &self.spreadsheet_id)
            .add_scope(AUTH_SCOPE)
            .doit()
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to apply formatting: {}", e)))?;
        Ok(())
    }
}

#[async_trait]
impl SheetOperations for SheetsClient {
    #[instrument(name = "Ensuring sheet exists", skip(self))]
    async fn ensure_sheet(&self, sheet_name: &str) -> Result<Sheet> {
        let (sheet, created) = match self.get_sheet(sheet_name).await? {
            Some(sheet) => (sheet, false),
            None => (self.create_sheet(sheet_name).await?, true),
        };
        let sheet_id = sheet.properties.as_ref().and_then(|p| p.sheet_id);
        match created {
            true => debug!(?sheet_id, "Created sheet"),
            false => debug!(?sheet_id, "Found existing sheet"),
        }

        Ok(sheet)
    }

    #[instrument(name = "Fetching sheet", skip(self))]
    async fn read_sheet(&self, sheet_name: &str) -> Result<Vec<Transaction>> {
        let range = format!("{}!A:Z", sheet_name);
        let (_, response) = self
            .hub
            .spreadsheets()
            .values_get(&self.spreadsheet_id, &range)
            .date_time_render_option("FORMATTED_STRING")
            .major_dimension("ROWS")
            .value_render_option("UNFORMATTED_VALUE")
            .add_scope(AUTH_SCOPE)
            .doit()
            .await
            .map_err(|e| {
                AppError::Sheets(format!("Failed to read sheet '{}': {}", sheet_name, e))
            })?;

        // Values are Option<Vec<Vec<serde_json::Value>>>
        let values = response.values.unwrap_or_default();
        Transaction::from_sheet_rows(&values)
    }

    #[instrument(name = "Writing sheet", skip(self, sheet, transactions))]
    async fn write_sheet(
        &self,
        sheet: &Sheet,
        sheet_name: &str,
        transactions: &[Transaction],
    ) -> Result<()> {
        // Clear the entire sheet first
        let range_to_clear = format!("{}!A:Z", sheet_name);
        let clear_request = ClearValuesRequest::default();

        self.hub
            .spreadsheets()
            .values_clear(clear_request, &self.spreadsheet_id, &range_to_clear)
            .add_scope(AUTH_SCOPE)
            .doit()
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to clear sheet: {}", e)))?;

        let rows = transactions.to_sheet_rows()?;

        let data_range = format!("{}!A1", sheet_name);
        let value_range = ValueRange {
            major_dimension: Some("ROWS".to_string()),
            range: Some(data_range.clone()),
            values: Some(rows),
        };

        self.hub
            .spreadsheets()
            .values_update(value_range, &self.spreadsheet_id, &data_range)
            .value_input_option("RAW")
            .add_scope(AUTH_SCOPE)
            .doit()
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to write transactions: {}", e)))?;

        // Apply formatting
        self.apply_formatting(sheet).await?;

        Ok(())
    }
}
