use model::{Currency, ReadTransactionDB, TransactionCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

pub struct AccountCapitalBeginningOfMonth;

impl AccountCapitalBeginningOfMonth {
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
                | TransactionCategory::FeeOpen(_)
                | TransactionCategory::FeeClose(_) => {
                    total = total.checked_sub(transaction.amount)
                        .ok_or_else(|| format!("Arithmetic overflow in subtraction: {} - {}", total, transaction.amount))?
                }
                TransactionCategory::PaymentFromTrade(_) => {
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
}
