use model::{Currency, ReadTransactionDB, TransactionCategory};
use rust_decimal::Decimal;
use uuid::Uuid;

pub struct AccountCapitalAvailable;

impl AccountCapitalAvailable {
    pub fn calculate(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all transactions for the account and currency
        let transactions =
            database.all_account_transactions_excluding_taxes(account_id, currency)?;

        // Sum all transactions based on their category
        let total: Result<Decimal, Box<dyn std::error::Error>> = transactions.iter().try_fold(
            Decimal::ZERO,
            |acc, transaction| {
                match transaction.category {
                    TransactionCategory::FundTrade(_) |
                    TransactionCategory::Withdrawal |
                    TransactionCategory::FeeOpen(_) |
                    TransactionCategory::FeeClose(_) => acc.checked_sub(transaction.amount)
                        .ok_or_else(|| format!("Arithmetic overflow in subtraction: {} - {}", acc, transaction.amount).into()),
                    TransactionCategory::PaymentFromTrade(_) |
                    TransactionCategory::Deposit => acc.checked_add(transaction.amount)
                        .ok_or_else(|| format!("Arithmetic overflow in addition: {} + {}", acc, transaction.amount).into()),
                    _ => Err(format!(
                        "capital_available: does not know how to calculate transaction with category: {}",
                        transaction.category
                    ).into()),
                }
            }
        );

        let total = total?;

        // Check if the total is negative, if it is then return an error
        if total.is_sign_negative() {
            return Err(format!("capital_available: total available is negative: {total}").into());
        }

        // If total is positive, return the value of total
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::read_transaction_db_mocks::MockDatabase;
    use rust_decimal_macros::dec;

    #[test]
    fn test_capital_available_with_empty_transactions() {
        let mut database = MockDatabase::new();

        let result =
            AccountCapitalAvailable::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_capital_available_with_positive_transactions() {
        let mut database = MockDatabase::new();

        // One deposit transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Deposit, dec!(100));

        let result =
            AccountCapitalAvailable::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(200));
    }

    #[test]
    fn test_capital_available_with_negative_transactions() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));

        let result =
            AccountCapitalAvailable::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(50));
    }

    #[test]
    fn test_capital_available_with_remaining_from_trade_entry() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(50));
        database.set_transaction(
            TransactionCategory::PaymentFromTrade(Uuid::new_v4()),
            dec!(1),
        );

        let result =
            AccountCapitalAvailable::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(51));
    }

    #[test]
    fn test_capital_available_with_multiple_transactions() {
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

        let result =
            AccountCapitalAvailable::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(3526));
    }

    #[test]
    fn test_capital_available_with_with() {
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

        let result =
            AccountCapitalAvailable::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(3526));
    }

    #[test]
    #[should_panic(
        expected = "capital_available: does not know how to calculate transaction with category: withdrawal_tax"
    )]
    fn test_capital_available_with_unknown_category() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::WithdrawalTax, dec!(100));

        AccountCapitalAvailable::calculate(Uuid::new_v4(), &Currency::USD, &mut database).unwrap();
    }

    #[test]
    fn test_capital_available_is_negative() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(200));

        AccountCapitalAvailable::calculate(Uuid::new_v4(), &Currency::USD, &mut database)
            .expect_err("capital_available: total available is negative: -100");
    }
}
