use crate::models::transaction::{Transaction, TransactionType};
use rust_decimal::Decimal;
use std::collections::HashMap;
use tracing::instrument;

/// Represents a matched pair of transactions (Debit <-> Credit)
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MatchedPair {
    pub debit_id: String,
    pub credit_id: String,
}

/// Reconciles transactions by matching Debits and Credits with identical amounts
/// within a configurable time window.
#[instrument(name = "Reconciling transactions", skip_all)]
pub fn reconcile_transactions(transactions: &[Transaction], days: u32) -> Vec<MatchedPair> {
    // Identify candidates (unmatched)
    let candidates: Vec<&Transaction> = transactions
        .iter()
        .filter(|t| t.matched_id.is_none())
        .collect();

    // Group candidates by absolute amount
    let mut by_amount: HashMap<Decimal, Vec<&Transaction>> = HashMap::new();
    for t in candidates {
        let key = t.amount.abs();
        by_amount.entry(key).or_default().push(t);
    }

    let mut matches = Vec::new();

    for (_, mut group) in by_amount {
        // Sort group by timestamp to ensure we match the earliest possible pairs
        group.sort_by_key(|t| t.timestamp);

        let mut matched_indexes = vec![false; group.len()];

        for i in 0..group.len() {
            if matched_indexes[i] {
                continue;
            }

            let tx_a = group[i];

            // We only trigger matching from Debits to avoid double counting
            if tx_a.type_ != TransactionType::Debit {
                continue;
            }

            // Find matching Credit in the group.
            for j in 0..group.len() {
                if i == j || matched_indexes[j] {
                    continue;
                }

                let tx_b = group[j];

                if tx_b.type_ != TransactionType::Credit {
                    continue;
                }

                // Explicitly check amount to guard against hash collisions
                if tx_a.amount + tx_b.amount != Decimal::ZERO {
                    continue;
                }

                // Prevent self-match
                if tx_a.id == tx_b.id {
                    continue;
                }

                // Check date window
                let diff = tx_b.timestamp.signed_duration_since(tx_a.timestamp);
                if diff.num_days().abs() <= days as i64 {
                    matched_indexes[i] = true;
                    matched_indexes[j] = true;

                    matches.push(MatchedPair {
                        debit_id: tx_a.id.clone(),
                        credit_id: tx_b.id.clone(),
                    });

                    break; // Proceed to next Debit.
                }
            }
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::transaction::TransactionType;
    use crate::models::transaction::test_helpers::{mock_datetime, mock_transaction};
    use chrono::Duration;
    use rust_decimal::prelude::dec;

    const TEST_RECONCILE_DAYS: u32 = 60;

    #[test]
    fn test_reconcile_basic_match() {
        let tx_debit = mock_transaction(
            "tx_debit",
            dec!(-50.0),
            TransactionType::Debit,
            mock_datetime(2025, 1, 1),
        );
        let tx_credit = mock_transaction(
            "tx_credit",
            dec!(50.0),
            TransactionType::Credit,
            tx_debit.timestamp + Duration::days(1),
        );

        let input = vec![tx_debit, tx_credit];
        let matches = reconcile_transactions(&input, TEST_RECONCILE_DAYS);
        let expected = vec![MatchedPair {
            debit_id: "tx_debit".to_string(),
            credit_id: "tx_credit".to_string(),
        }];
        assert_eq!(matches, expected);
    }

    #[test]
    fn test_reconcile_credit_cleared_first() {
        let tx_credit = mock_transaction(
            "credit_id_1",
            dec!(50.0),
            TransactionType::Credit, // Credit clears first
            mock_datetime(2025, 1, 1),
        );
        let tx_debit = mock_transaction(
            "debit_id_1",
            dec!(-50.0),
            TransactionType::Debit, // Debit clears later
            tx_credit.timestamp + Duration::days(1),
        );

        let input = vec![tx_credit, tx_debit];
        let matches = reconcile_transactions(&input, TEST_RECONCILE_DAYS);
        let expected = vec![MatchedPair {
            debit_id: "debit_id_1".to_string(),
            credit_id: "credit_id_1".to_string(),
        }];
        assert_eq!(matches, expected);
    }

    #[test]
    fn test_reconcile_closest_candidate_by_date() {
        let tx_debit = mock_transaction(
            "tx_debit",
            dec!(-25.0),
            TransactionType::Debit,
            mock_datetime(2025, 1, 1),
        );
        let tx_other = mock_transaction(
            "tx_credit",
            dec!(25.0),
            TransactionType::Credit,
            tx_debit.timestamp + Duration::days(10),
        );
        let tx_credit = mock_transaction(
            "tx3",
            dec!(25.0),
            TransactionType::Credit,
            tx_debit.timestamp + Duration::days(5),
        );

        let input = vec![tx_debit, tx_other, tx_credit];
        let matches = reconcile_transactions(&input, TEST_RECONCILE_DAYS);
        let expected = vec![MatchedPair {
            debit_id: "tx_debit".to_string(),
            credit_id: "tx3".to_string(),
        }];
        assert_eq!(matches, expected);
    }

    #[test]
    fn test_reconcile_ignores_already_matched() {
        let mut tx_debit = mock_transaction(
            "tx_debit",
            dec!(-50.0),
            TransactionType::Debit,
            mock_datetime(2025, 1, 1),
        );
        tx_debit.matched_id = Some("tx_other".to_string());
        let tx_credit = mock_transaction(
            "tx_credit",
            dec!(50.0),
            TransactionType::Credit,
            tx_debit.timestamp + Duration::days(1),
        );

        let input = vec![tx_debit, tx_credit];
        let matches = reconcile_transactions(&input, TEST_RECONCILE_DAYS);
        assert_eq!(matches, vec![]);
    }

    #[test]
    fn test_reconcile_ignores_outside_window() {
        let tx_debit = mock_transaction(
            "tx_debit",
            dec!(-50.0),
            TransactionType::Debit,
            mock_datetime(2025, 1, 1),
        );
        let tx_credit = mock_transaction(
            "tx_credit",
            dec!(50.0),
            TransactionType::Credit,
            tx_debit.timestamp + Duration::days(61),
        );

        let input = vec![tx_debit, tx_credit];
        let matches = reconcile_transactions(&input, TEST_RECONCILE_DAYS);
        assert_eq!(matches, vec![]);
    }

    #[test]
    fn test_reconcile_ignores_amount_mismatch() {
        let tx_debit = mock_transaction(
            "tx_debit",
            dec!(-50.0),
            TransactionType::Debit,
            mock_datetime(2025, 1, 1),
        );
        let tx_credit = mock_transaction(
            "tx_credit",
            dec!(50.01),
            TransactionType::Credit,
            tx_debit.timestamp + Duration::days(1),
        );

        let input = vec![tx_debit, tx_credit];
        let matches = reconcile_transactions(&input, TEST_RECONCILE_DAYS);
        assert_eq!(matches, vec![]);
    }

    #[test]
    fn test_reconcile_multiple() {
        let base_date = mock_datetime(2025, 1, 1);

        let tx1_debit =
            mock_transaction("tx1_debit", dec!(-50.0), TransactionType::Debit, base_date);
        let tx1_credit = mock_transaction(
            "tx1_credit",
            dec!(50.0),
            TransactionType::Credit,
            base_date + Duration::days(1),
        ); // Matches tx1_debit

        let tx2_debit = mock_transaction(
            "tx2_debit",
            dec!(-100.0),
            TransactionType::Debit,
            base_date + Duration::days(4),
        );
        let tx2_credit_unmatched = mock_transaction(
            "tx2_credit_unmatched",
            dec!(100.0),
            TransactionType::Credit,
            base_date + Duration::days(68),
        ); // Too far from tx2_debit
        let tx2_credit = mock_transaction(
            "tx2_credit",
            dec!(100.0),
            TransactionType::Credit,
            base_date + Duration::days(5),
        ); // Matches tx2_debit (closest)

        let tx3_debit = mock_transaction(
            "tx3_debit",
            dec!(-25.0),
            TransactionType::Debit,
            base_date + Duration::days(31),
        );
        let tx3_credit = mock_transaction(
            "tx3_credit",
            dec!(25.0),
            TransactionType::Credit,
            base_date + Duration::days(32),
        ); // Matches tx3_debit

        let tx4_debit_unmatched = mock_transaction(
            "tx4_debit_unmatched",
            dec!(-10.0),
            TransactionType::Debit,
            base_date + Duration::days(14),
        ); // No match
        let tx5_credit_unmatched = mock_transaction(
            "tx5_credit_unmatched",
            dec!(20.0),
            TransactionType::Credit,
            base_date + Duration::days(19),
        ); // No match

        let input = vec![
            tx4_debit_unmatched,
            tx5_credit_unmatched,
            tx2_credit_unmatched,
            tx1_debit,
            tx1_credit,
            tx2_debit,
            tx2_credit,
            tx3_debit,
            tx3_credit,
        ];

        let matches = reconcile_transactions(&input, TEST_RECONCILE_DAYS);
        let expected = vec![
            MatchedPair {
                debit_id: "tx1_debit".to_string(),
                credit_id: "tx1_credit".to_string(),
            },
            MatchedPair {
                debit_id: "tx2_debit".to_string(),
                credit_id: "tx2_credit".to_string(),
            },
            MatchedPair {
                debit_id: "tx3_debit".to_string(),
                credit_id: "tx3_credit".to_string(),
            },
        ];

        let mut matches_sorted = matches;
        matches_sorted.sort();
        let mut expected_sorted = expected;
        expected_sorted.sort();

        assert_eq!(matches_sorted, expected_sorted);
    }
}
