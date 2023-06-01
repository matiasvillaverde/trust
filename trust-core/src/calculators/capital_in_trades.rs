use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{Currency, ReadTransactionDB, TransactionCategory};
use uuid::Uuid;

pub struct AccountCapitalInApprovedTrades;

impl AccountCapitalInApprovedTrades {
    pub fn calculate(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all transactions
        let transactions =
            database.all_account_transactions_funding_in_approved_trades(account_id, currency)?;

        // Sum all transactions
        let total_available: Decimal = transactions
            .iter()
            .map(|transaction| match transaction.category {
                TransactionCategory::FundTrade(_) => transaction.price.amount,
                _ => dec!(0),
            })
            .sum();

        Ok(total_available)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::read_transaction_db_mocks::MockDatabase;
    use rust_decimal_macros::dec;
    #[test]
    fn test_total_balance_with_empty_transactions() {
        let account_id = Uuid::new_v4();
        let mut database = MockDatabase::new();

        let result =
            AccountCapitalInApprovedTrades::calculate(account_id, &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_total_balance_with_deposit_transactions() {
        let account_id = Uuid::new_v4();
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Deposit, dec!(100));

        let result =
            AccountCapitalInApprovedTrades::calculate(account_id, &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_capital_in_trades_with_fund_one_trade() {
        let account_id = Uuid::new_v4();
        let mut database = MockDatabase::new();

        // One deposit and one withdrawal transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(10.99));

        let result =
            AccountCapitalInApprovedTrades::calculate(account_id, &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(10.99));
    }

    #[test]
    fn test_capital_in_trades_with_fund_five_trade() {
        let account_id = Uuid::new_v4();
        let mut database = MockDatabase::new();

        // One deposit and one withdrawal transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(10000));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(10.99));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(299));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(323));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(344));

        let result =
            AccountCapitalInApprovedTrades::calculate(account_id, &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(976.99));
    }
}
