use rust_decimal::Decimal;
use trust_model::{Currency, ReadTransactionDB, TransactionCategory};
use uuid::Uuid;

pub struct CapitalAvailableCalculator;

impl CapitalAvailableCalculator {
    pub fn capital_available(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all transactions
        let transactions =
            database.all_account_transactions_excluding_taxes(account_id, currency)?;

        // Sum all transactions
        let total_available: Decimal = transactions
            .iter()
            .map(|transaction| match transaction.category {
                TransactionCategory::FundTrade(_) | TransactionCategory::Withdrawal | TransactionCategory::FeeOpen(_) | TransactionCategory::FeeClose(_) => {
                    -transaction.price.amount
                }
                TransactionCategory::PaymentFromTrade(_) | TransactionCategory::Deposit => {
                    transaction.price.amount
                }
                default => panic!(
                    "capital_available: does not know how to calculate transaction with category: {}",
                    default
                ),
            })
            .sum();

        if total_available.is_sign_negative() {
            Err(format!(
                "capital_available: total available is negative: {}",
                total_available
            ))?;
        }

        Ok(total_available)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::read_transaction_db_mocks::MockDatabase;
    use rust_decimal_macros::dec;

    #[test]
    fn test_capital_available_with_empty_transactions() {
        let account_id = Uuid::new_v4();
        let currency = Currency::USD;
        let mut database = MockDatabase::new();

        let result =
            CapitalAvailableCalculator::capital_available(account_id, &currency, &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_capital_available_with_positive_transactions() {
        let account_id = Uuid::new_v4();
        let currency = Currency::USD;
        let mut database = MockDatabase::new();

        // One deposit transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Deposit, dec!(100));

        let result =
            CapitalAvailableCalculator::capital_available(account_id, &currency, &mut database);
        assert_eq!(result.unwrap(), dec!(200));
    }

    #[test]
    fn test_capital_available_with_negative_transactions() {
        let account_id = Uuid::new_v4();
        let currency = Currency::USD;
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));

        let result =
            CapitalAvailableCalculator::capital_available(account_id, &currency, &mut database);
        assert_eq!(result.unwrap(), dec!(50));
    }

    #[test]
    fn test_capital_available_with_multiple_transactions() {
        let account_id = Uuid::new_v4();
        let currency = Currency::USD;
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
            CapitalAvailableCalculator::capital_available(account_id, &currency, &mut database);
        assert_eq!(result.unwrap(), dec!(3526));
    }

    #[test]
    #[should_panic(
        expected = "capital_available: does not know how to calculate transaction with category: withdrawal_tax"
    )]
    fn test_capital_available_with_unknown_category() {
        let account_id = Uuid::new_v4();
        let currency = Currency::USD;
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::WithdrawalTax, dec!(100));

        CapitalAvailableCalculator::capital_available(account_id, &currency, &mut database)
            .unwrap();
    }

    #[test]
    fn test_capital_available_is_negative() {
        let account_id = Uuid::new_v4();
        let currency = Currency::USD;
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(200));

        CapitalAvailableCalculator::capital_available(account_id, &currency, &mut database)
            .expect_err("capital_available: total available is negative: -100");
    }
}
