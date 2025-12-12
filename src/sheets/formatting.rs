use crate::error::{AppError, Result};
use crate::models::Transaction;
use google_sheets4::FieldMask;
use google_sheets4::api::{
    AddConditionalFormatRuleRequest, AddProtectedRangeRequest, BooleanCondition, BooleanRule,
    CellData, CellFormat, Color, ConditionValue, ConditionalFormatRule,
    DeleteConditionalFormatRuleRequest, DeleteProtectedRangeRequest, GridProperties, GridRange,
    ProtectedRange, RepeatCellRequest, Request, Sheet, SheetProperties, TextFormat,
    UpdateSheetPropertiesRequest,
};

/// Make header row bold.
pub(super) fn bold_header_rule(sheet_id: i32) -> Request {
    Request {
        repeat_cell: Some(RepeatCellRequest {
            range: Some(GridRange {
                sheet_id: Some(sheet_id),
                start_row_index: Some(0),
                end_row_index: Some(1),
                start_column_index: None,
                end_column_index: None,
            }),
            cell: Some(CellData {
                user_entered_format: Some(CellFormat {
                    text_format: Some(TextFormat {
                        bold: Some(true),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            fields: Some(FieldMask::new(&["userEnteredFormat.textFormat.bold"])),
        }),
        ..Default::default()
    }
}

/// Freeze header row.
pub(super) fn freeze_header_rule(sheet_id: i32) -> Request {
    Request {
        update_sheet_properties: Some(UpdateSheetPropertiesRequest {
            properties: Some(SheetProperties {
                sheet_id: Some(sheet_id),
                grid_properties: Some(GridProperties {
                    frozen_row_count: Some(1),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            fields: Some(FieldMask::new(&["gridProperties.frozenRowCount"])),
        }),
        ..Default::default()
    }
}

/// Highlight rows where "ID" is filled but "Matched ID" is blank.
pub(super) fn highlight_rules(sheet_id: i32, sheet: &Sheet) -> Result<Vec<Request>> {
    let mut requests = Vec::new();

    let light_yellow = Color {
        red: Some(0.988),
        green: Some(0.910),
        blue: Some(0.698),
        alpha: Some(1.0),
    };
    let id_column = Transaction::get_column_letter("ID")
        .ok_or_else(|| AppError::Sheets("ID column not found".to_string()))?;
    let matched_id_column = Transaction::get_column_letter("Matched ID")
        .ok_or_else(|| AppError::Sheets("Matched ID column not found".to_string()))?;

    for _ in sheet
        .conditional_formats
        .as_deref()
        .unwrap_or_default()
        .iter()
    {
        requests.push(Request {
            delete_conditional_format_rule: Some(DeleteConditionalFormatRuleRequest {
                index: Some(0), // Delete the first rule repeatedly
                sheet_id: Some(sheet_id),
            }),
            ..Default::default()
        });
    }

    requests.push(Request {
        add_conditional_format_rule: Some(AddConditionalFormatRuleRequest {
            index: Some(0),
            rule: Some(ConditionalFormatRule {
                ranges: Some(vec![GridRange {
                    sheet_id: Some(sheet_id),
                    start_row_index: Some(1), // Skip header row
                    end_row_index: None,
                    start_column_index: None,
                    end_column_index: None,
                }]),
                boolean_rule: Some(BooleanRule {
                    condition: Some(BooleanCondition {
                        type_: Some("CUSTOM_FORMULA".to_string()),
                        values: Some(vec![ConditionValue {
                            user_entered_value: Some(format!(
                                "=AND(NOT(ISBLANK(${}2)), ISBLANK(${}2))",
                                id_column, matched_id_column,
                            )),
                            ..Default::default()
                        }]),
                    }),
                    format: Some(CellFormat {
                        background_color: Some(light_yellow),
                        ..Default::default()
                    }),
                }),
                ..Default::default()
            }),
        }),
        ..Default::default()
    });

    Ok(requests)
}

/// Protect all columns up to and including "ID" column.
pub(super) fn protection_rules(sheet_id: i32, sheet: &Sheet) -> Result<Vec<Request>> {
    let mut requests = Vec::new();

    sheet
        .protected_ranges
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter_map(|range| range.protected_range_id)
        .for_each(|id| {
            requests.push(Request {
                delete_protected_range: Some(DeleteProtectedRangeRequest {
                    protected_range_id: Some(id),
                }),
                ..Default::default()
            });
        });

    let id_col_idx = Transaction::get_column_index("ID")
        .ok_or_else(|| AppError::Sheets("ID column not found".to_string()))?;
    let end_col_index = (id_col_idx + 1) as i32;

    requests.push(Request {
        add_protected_range: Some(AddProtectedRangeRequest {
            protected_range: Some(ProtectedRange {
                range: Some(GridRange {
                    sheet_id: Some(sheet_id),
                    start_column_index: Some(0),
                    end_column_index: Some(end_col_index),
                    ..Default::default()
                }),
                description: Some("Managed by credit-card-tracker".to_string()),
                warning_only: Some(true),
                ..Default::default()
            }),
        }),
        ..Default::default()
    });

    Ok(requests)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bold_header_rule() {
        let req = bold_header_rule(123);
        let repeat_cell = req.repeat_cell.unwrap();
        assert_eq!(repeat_cell.range.unwrap().sheet_id, Some(123));
        assert!(
            repeat_cell
                .cell
                .unwrap()
                .user_entered_format
                .unwrap()
                .text_format
                .unwrap()
                .bold
                .unwrap()
        );
    }

    #[test]
    fn test_freeze_header_rule() {
        let req = freeze_header_rule(123);
        let props = req.update_sheet_properties.unwrap().properties.unwrap();
        assert_eq!(props.sheet_id, Some(123));
        assert_eq!(props.grid_properties.unwrap().frozen_row_count, Some(1));
    }

    #[test]
    fn test_highlight_rules() {
        // Mock sheet with one existing rule
        let sheet = Sheet {
            conditional_formats: Some(vec![
                ConditionalFormatRule::default(),
                ConditionalFormatRule::default(),
            ]),
            ..Default::default()
        };

        let reqs = highlight_rules(123, &sheet).unwrap();
        assert_eq!(reqs.len(), 3, "should have 3 requests, got {:?}", reqs);
        let mut reqs = reqs.iter();

        let req = reqs
            .next()
            .unwrap()
            .delete_conditional_format_rule
            .as_ref()
            .unwrap();
        assert_eq!(req.sheet_id, Some(123));
        assert_eq!(req.index, Some(0));

        let req = reqs
            .next()
            .unwrap()
            .delete_conditional_format_rule
            .as_ref()
            .unwrap();
        assert_eq!(req.sheet_id, Some(123));
        assert_eq!(req.index, Some(0));

        let req = reqs
            .next()
            .unwrap()
            .add_conditional_format_rule
            .as_ref()
            .unwrap();
        let rule = req.rule.as_ref().unwrap();
        let boolean_rule = rule.boolean_rule.as_ref().unwrap();
        let condition = boolean_rule.condition.as_ref().unwrap();

        assert_eq!(condition.type_.as_deref(), Some("CUSTOM_FORMULA"));
        let formula = condition.values.as_ref().unwrap()[0]
            .user_entered_value
            .as_ref()
            .unwrap();
        assert!(formula.contains("ISBLANK"));
    }

    #[test]
    fn test_protection_rules() {
        // Mock sheet with one existing protected range
        let sheet = Sheet {
            protected_ranges: Some(vec![
                ProtectedRange {
                    protected_range_id: Some(222),
                    ..Default::default()
                },
                ProtectedRange {
                    protected_range_id: Some(333),
                    ..Default::default()
                },
            ]),
            ..Default::default()
        };

        let reqs = protection_rules(111, &sheet).unwrap();
        assert_eq!(reqs.len(), 3, "should have 3 requests, got {:?}", reqs);
        let mut reqs = reqs.iter();

        let req = reqs
            .next()
            .unwrap()
            .delete_protected_range
            .as_ref()
            .unwrap();
        assert_eq!(req.protected_range_id, Some(222));

        let req = reqs
            .next()
            .unwrap()
            .delete_protected_range
            .as_ref()
            .unwrap();
        assert_eq!(req.protected_range_id, Some(333));

        let req = reqs.next().unwrap().add_protected_range.as_ref().unwrap();
        let protected_range = req.protected_range.as_ref().unwrap();
        assert_eq!(protected_range.warning_only, Some(true));

        let range = protected_range.range.as_ref().unwrap();
        assert_eq!(range.sheet_id, Some(111));
        assert_eq!(range.start_column_index, Some(0));
        assert!(range.end_column_index.unwrap() > 1);
    }
}
