use chrono::{Datelike, Utc};
use model::{Currency, DatabaseFactory, ReadTransactionDB, Status, TransactionCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashSet;
use uuid::Uuid;

pub struct AccountCapitalBeginningOfMonth;

impl AccountCapitalBeginningOfMonth {
    /// Opening ledger capital adjusted for current-month capital flows and
    /// reservations carried into this month. Open trades retain their original
    /// entry basis; this is not a mark-to-market broker-equity calculation.
    pub fn risk_reference(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let now = Utc::now();
        let cutoff = now
            .date_naive()
            .with_day(1)
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .ok_or("Invalid UTC month boundary")?;
        let mut reference =
            Self::calculate(account_id, currency, database.transaction_read().as_mut())?;
        let transactions = database
            .transaction_read()
            .all_transactions(account_id, currency)?;
        let mut carried: HashSet<Uuid> = database
            .trade_read()
            .all_open_trades_for_currency(account_id, currency)?
            .into_iter()
            .map(|t| t.id)
            .collect();
        // A position opened last month and closed this month must not turn its
        // returned collateral into apparent profit. Late fees do not move its exit date.
        for status in [Status::ClosedTarget, Status::ClosedStopLoss] {
            for trade in database
                .trade_read()
                .read_trades_with_status(account_id, status)?
            {
                if trade.currency == *currency && transactions.iter().any(|tx| tx.created_at >= cutoff && matches!(tx.category, TransactionCategory::CloseTarget(id) | TransactionCategory::CloseSafetyStop(id) | TransactionCategory::CloseSafetyStopSlippage(id) if id == trade.id)) {
                    carried.insert(trade.id);
                }
            }
        }
        for tx in transactions {
            let direction = if tx.created_at >= cutoff {
                match tx.category {
                    TransactionCategory::Deposit | TransactionCategory::PaymentEarnings(_) => 1,
                    TransactionCategory::Withdrawal | TransactionCategory::WithdrawalEarnings => -1,
                    _ => 0,
                }
            } else {
                match tx.category {
                    TransactionCategory::FundTrade(id) if carried.contains(&id) => 1,
                    TransactionCategory::PaymentFromTrade(id) if carried.contains(&id) => -1,
                    _ => 0,
                }
            };
            reference = match direction {
                1 => reference.checked_add(tx.amount),
                -1 => reference.checked_sub(tx.amount),
                _ => Some(reference),
            }
            .ok_or("Monthly risk reference overflow")?;
        }
        Ok(reference.max(Decimal::ZERO))
    }

    pub fn calculate(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Calculate all the transactions at the beginning of the month
        let mut total = dec!(0.0);
        for transaction in
            database.all_transaction_excluding_current_month_and_taxes(account_id, currency)?
        {
            match transaction.category {
                TransactionCategory::FundTrade(_)
                | TransactionCategory::Withdrawal
                | TransactionCategory::WithdrawalEarnings
                | TransactionCategory::FeeOpen(_)
                | TransactionCategory::FeeClose(_) => {
                    total = total.checked_sub(transaction.amount)
                        .ok_or_else(|| format!("Arithmetic overflow in subtraction: {} - {}", total, transaction.amount))?
                }
                TransactionCategory::PaymentFromTrade(_) | TransactionCategory::PaymentEarnings(_) => {
                    total = total.checked_add(transaction.amount)
                        .ok_or_else(|| format!("Arithmetic overflow in addition: {} + {}", total, transaction.amount))?
                }
                TransactionCategory::Deposit => {
                    total = total.checked_add(transaction.amount)
                        .ok_or_else(|| format!("Arithmetic overflow in addition: {} + {}", total, transaction.amount))?
                }
                default => return Err(format!(
                    "capital_at_beginning_of_month: does not know how to calculate transaction with category: {default}. Transaction: {transaction:?}"
                ).into()),
            }
        }

        if total.is_sign_negative() {
            return Err(format!(
                "capital_at_beginning_of_month: capital at beginning of the month was negative: {total}"
            )
            .into());
        }

        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::read_transaction_db_mocks::MockDatabase;
    use rust_decimal_macros::dec;

    #[test]
    fn test_capital_at_beginning_of_month_with_empty_transactions() {
        let mut database = MockDatabase::new();

        let result = AccountCapitalBeginningOfMonth::calculate(
            Uuid::new_v4(),
            &Currency::USD,
            &mut database,
        );
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_capital_at_beginning_of_month_with_positive_transactions() {
        let mut database = MockDatabase::new();

        // One deposit transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Deposit, dec!(100));

        let result = AccountCapitalBeginningOfMonth::calculate(
            Uuid::new_v4(),
            &Currency::USD,
            &mut database,
        );
        assert_eq!(result.unwrap(), dec!(200));
    }

    #[test]
    fn test_capital_at_beginning_of_month_with_negative_transactions() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));

        let result = AccountCapitalBeginningOfMonth::calculate(
            Uuid::new_v4(),
            &Currency::USD,
            &mut database,
        );
        assert_eq!(result.unwrap(), dec!(50));
    }

    #[test]
    fn test_capital_at_beginning_of_month_with_multiple_transactions() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(50));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::FeeOpen(Uuid::new_v4()), dec!(1.4));
        database.set_transaction(TransactionCategory::FeeOpen(Uuid::new_v4()), dec!(4.6));
        database.set_transaction(
            TransactionCategory::PaymentFromTrade(Uuid::new_v4()),
            dec!(3432),
        );
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));

        let result = AccountCapitalBeginningOfMonth::calculate(
            Uuid::new_v4(),
            &Currency::USD,
            &mut database,
        );
        assert_eq!(result.unwrap(), dec!(3526));
    }

    #[test]
    fn test_capital_at_beginning_of_month_with_with() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(50));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::FeeOpen(Uuid::new_v4()), dec!(1.4));
        database.set_transaction(TransactionCategory::FeeOpen(Uuid::new_v4()), dec!(4.6));
        database.set_transaction(
            TransactionCategory::PaymentFromTrade(Uuid::new_v4()),
            dec!(3432),
        );
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));

        let result = AccountCapitalBeginningOfMonth::calculate(
            Uuid::new_v4(),
            &Currency::USD,
            &mut database,
        );
        assert_eq!(result.unwrap(), dec!(3526));
    }

    #[test]
    #[should_panic(
        expected = "capital_at_beginning_of_month: does not know how to calculate transaction with category: withdrawal_tax"
    )]
    fn test_capital_at_beginning_of_month_with_unknown_category() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::WithdrawalTax, dec!(100));

        AccountCapitalBeginningOfMonth::calculate(Uuid::new_v4(), &Currency::USD, &mut database)
            .unwrap();
    }

    #[test]
    fn test_capital_at_beginning_of_month_is_negative() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(200));

        AccountCapitalBeginningOfMonth::calculate(Uuid::new_v4(), &Currency::USD, &mut database)
            .expect_err(
            "capital_at_beginning_of_month: capital at beginning of the month was negative -100",
        );
    }

    #[test]
    fn test_capital_at_beginning_of_month_reports_withdrawal_overflow() {
        let mut database = MockDatabase::new();
        database.set_transaction(TransactionCategory::Withdrawal, Decimal::MAX);
        database.set_transaction(TransactionCategory::Withdrawal, dec!(1));

        let error = AccountCapitalBeginningOfMonth::calculate(
            Uuid::new_v4(),
            &Currency::USD,
            &mut database,
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("Arithmetic overflow in subtraction"));
    }

    #[test]
    fn test_capital_at_beginning_of_month_reports_payment_addition_overflow() {
        let mut database = MockDatabase::new();
        database.set_transaction(
            TransactionCategory::PaymentFromTrade(Uuid::new_v4()),
            Decimal::MAX,
        );
        database.set_transaction(
            TransactionCategory::PaymentFromTrade(Uuid::new_v4()),
            Decimal::MAX,
        );

        let error = AccountCapitalBeginningOfMonth::calculate(
            Uuid::new_v4(),
            &Currency::USD,
            &mut database,
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("Arithmetic overflow in addition"));
    }

    #[test]
    fn test_capital_at_beginning_of_month_reports_deposit_addition_overflow() {
        let mut database = MockDatabase::new();
        database.set_transaction(TransactionCategory::Deposit, Decimal::MAX);
        database.set_transaction(TransactionCategory::Deposit, Decimal::MAX);

        let error = AccountCapitalBeginningOfMonth::calculate(
            Uuid::new_v4(),
            &Currency::USD,
            &mut database,
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("Arithmetic overflow in addition"));
    }
}
