use super::SheetOperations;
use crate::config::GoogleConfig;
use crate::error::{AppError, Result};
use crate::models::{FromSheetRows, ToSheetRows, Transaction};
use crate::sheets::auth::{GOOGLE_REDIRECT_URI, create_and_verify_authenticator};
use async_trait::async_trait;
use google_drive::Client as DriveApiClient;
use serde_json::json;
use sheets::Client as SheetsApiClient;
use sheets::types::{
    BatchUpdateSpreadsheetRequest, ClearValuesRequest, DateTimeRenderOption, Dimension, SheetType,
    Spreadsheet, SpreadsheetProperties, ValueInputOption, ValueRange, ValueRenderOption,
};

use tracing::{debug, instrument};

// Access to files created or opened by the app
pub(crate) const AUTH_SCOPE: &str = "https://www.googleapis.com/auth/drive.file";

// Our auth method doesn't provide refresh tokens.
const EMPTY_REFRESH_TOKEN: &str = "";

// Name of the spreadsheet file in Google Drive.
const SPREADSHEET_NAME: &str = "Credit Card Transactions (credit-card-tracker)";

pub struct SheetsClient {
    client: SheetsApiClient,
    spreadsheet_id: String,
    spreadsheet_url: String,
}

impl SheetsClient {
    /// Create a new SheetsClient with authenticated access
    #[instrument(name = "Authenticating to Google Sheets", skip_all)]
    pub async fn new(config: &GoogleConfig) -> Result<Self> {
        let auth = create_and_verify_authenticator(config).await?;

        // Get the token
        let token = auth
            .token(&[AUTH_SCOPE])
            .await
            .map_err(|e| AppError::Auth(format!("Failed to get token: {}", e)))?;

        let token_str = token
            .token()
            .ok_or_else(|| AppError::Auth("No token value".to_string()))?;

        // Create clients
        let client = SheetsApiClient::new(
            &config.client_id,
            &config.client_secret,
            GOOGLE_REDIRECT_URI,
            token_str,
            EMPTY_REFRESH_TOKEN,
        );
        let drive = DriveApiClient::new(
            &config.client_id,
            &config.client_secret,
            GOOGLE_REDIRECT_URI,
            token_str,
            EMPTY_REFRESH_TOKEN,
        );

        let (spreadsheet_id, spreadsheet_url) =
            Self::get_or_create_spreadsheet(&client, &drive).await?;

        Ok(Self {
            client,
            spreadsheet_id,
            spreadsheet_url,
        })
    }

    pub fn spreadsheet_url(&self) -> String {
        self.spreadsheet_url.to_string()
    }

    async fn get_or_create_spreadsheet(
        sheets: &SheetsApiClient,
        drive: &DriveApiClient,
    ) -> Result<(String, String)> {
        if let Some(id) = Self::search_spreadsheet_by_name(drive, SPREADSHEET_NAME).await? {
            let url = format!("https://docs.google.com/spreadsheets/d/{}", id);
            return Ok((id, url));
        }

        Self::create_new_spreadsheet(sheets, SPREADSHEET_NAME).await
    }

    #[instrument(name = "Finding existing spreadsheet", skip(drive))]
    async fn search_spreadsheet_by_name(
        drive: &DriveApiClient,
        name: &str,
    ) -> Result<Option<String>> {
        let query = format!(
            "name='{}' and mimeType='application/vnd.google-apps.spreadsheet' and trashed=false",
            name
        );

        let result = drive
            .files()
            .list(
                "",     // corpora
                "",     // drive_id
                false,  // include_items_from_all_drives
                "",     // include_labels
                false,  // include_permissions_for_view
                "",     // order_by
                1,      // page_size
                "",     // page_token
                &query, // q
                "",     // spaces
                false,  // supports_all_drives
                false,  // supports_team_drives
                "",     // team_drive_id
            )
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to search spreadsheet: {}", e)))?;

        // TODO: error when multiple found?
        let spreadsheet_id = result.body.first().map(|file| file.id.clone());

        Ok(spreadsheet_id)
    }

    #[instrument(name = "Creating new spreadsheet", skip(sheets))]
    async fn create_new_spreadsheet(
        sheets: &SheetsApiClient,
        name: &str,
    ) -> Result<(String, String)> {
        let props = SpreadsheetProperties {
            auto_recalc: None,
            default_format: None,
            iterative_calculation_settings: None,
            locale: "en_US".to_string(),
            spreadsheet_theme: None,
            time_zone: "UTC".to_string(),
            title: name.to_string(),
        };

        let spreadsheet = Spreadsheet {
            data_source_schedules: vec![],
            data_sources: vec![],
            // TODO: set?
            developer_metadata: vec![],
            named_ranges: vec![],
            properties: Some(props),
            sheets: vec![],
            spreadsheet_id: String::new(),
            spreadsheet_url: String::new(),
        };

        let result = sheets
            .spreadsheets()
            .create(&spreadsheet)
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to create spreadsheet: {}", e)))?;

        let spreadsheet_id = if result.body.spreadsheet_id.is_empty() {
            return Err(AppError::Sheets(
                "Created spreadsheet has empty ID".to_string(),
            ));
        } else {
            result.body.spreadsheet_id.clone()
        };

        let spreadsheet_url = if result.body.spreadsheet_url.is_empty() {
            return Err(AppError::Sheets(
                "Created spreadsheet has empty URL".to_string(),
            ));
        } else {
            result.body.spreadsheet_url.clone()
        };

        Ok((spreadsheet_id, spreadsheet_url))
    }

    async fn sheet_exists(&self, sheet_name: &str) -> Result<bool> {
        let result = self
            .client
            .spreadsheets()
            .get(
                &self.spreadsheet_id,
                false, // include_grid_data
                &[],   // ranges
            )
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to get spreadsheet: {}", e)))?;

        let exists = result.body.sheets.iter().any(|sheet| {
            sheet
                .properties
                .as_ref()
                .map(|props| props.title == sheet_name)
                .unwrap_or(false)
        });

        Ok(exists)
    }

    async fn create_sheet(&self, sheet_name: &str) -> Result<()> {
        use sheets::types::{AddSheetRequest, Request, SheetProperties};

        let add_sheet_request = AddSheetRequest {
            properties: Some(SheetProperties {
                title: sheet_name.to_string(),
                sheet_id: 0,
                index: 0,
                sheet_type: Some(SheetType::Grid),
                grid_properties: None,
                hidden: false,
                tab_color: None,
                right_to_left: false,
                data_source_sheet_properties: None,
                tab_color_style: None,
            }),
        };

        // Use serde_json to create Request
        let request_json = json!({
            "addSheet": add_sheet_request
        });

        let request: Request = serde_json::from_value(request_json)
            .map_err(|e| AppError::Sheets(format!("Failed to build request: {}", e)))?;

        let batch_update = BatchUpdateSpreadsheetRequest {
            requests: vec![request],
            include_spreadsheet_in_response: Some(false),
            response_ranges: vec![],
            response_include_grid_data: Some(false),
        };

        self.client
            .spreadsheets()
            .batch_update(&self.spreadsheet_id, &batch_update)
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to create sheet: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl SheetOperations for SheetsClient {
    #[instrument(name = "Ensuring sheet exists", skip(self))]
    async fn ensure_sheet(&self, sheet_name: &str) -> Result<()> {
        if self.sheet_exists(sheet_name).await? {
            debug!("Sheet '{}' exists.", sheet_name);
        } else {
            debug!("Sheet '{}' doesn't exist, creating it", sheet_name);
            self.create_sheet(sheet_name).await?;
        }
        Ok(())
    }

    #[instrument(name = "Fetching sheet", skip(self))]
    async fn read_sheet(&self, sheet_name: &str) -> Result<Vec<Transaction>> {
        let range = format!("{}!A:G", sheet_name);
        let response = self
            .client
            .spreadsheets()
            .values_get(
                &self.spreadsheet_id,
                &range,
                DateTimeRenderOption::FormattedString,
                Dimension::Rows,
                ValueRenderOption::UnformattedValue,
            )
            .await
            .map_err(|e| {
                AppError::Sheets(format!("Failed to read sheet '{}': {}", sheet_name, e))
            })?;

        Transaction::from_sheet_rows(&response.body.values)
    }

    #[instrument(name = "Writing sheet", skip(self, transactions))]
    async fn write_sheet(&self, sheet_name: &str, transactions: &[Transaction]) -> Result<()> {
        // Clear the entire sheet first
        let range_to_clear = format!("{}!A:Z", sheet_name);
        let clear_request = ClearValuesRequest {};

        self.client
            .spreadsheets()
            .values_clear(&self.spreadsheet_id, &range_to_clear, &clear_request)
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to clear sheet: {}", e)))?;

        let rows = transactions.to_sheet_rows()?;

        let data_range = format!("{}!A1", sheet_name);
        let value_range = ValueRange {
            major_dimension: Some(Dimension::Rows),
            range: data_range.clone(),
            values: rows,
        };

        self.client
            .spreadsheets()
            .values_update(
                &self.spreadsheet_id,
                &data_range,
                false,
                DateTimeRenderOption::FormattedString,
                ValueRenderOption::FormattedValue,
                ValueInputOption::Raw,
                &value_range,
            )
            .await
            .map_err(|e| AppError::Sheets(format!("Failed to write transactions: {}", e)))?;

        Ok(())
    }
}
