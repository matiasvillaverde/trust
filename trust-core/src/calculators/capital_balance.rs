use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{Currency, ReadTransactionDB, TransactionCategory};
use uuid::Uuid;

pub struct AccountCapitalBalance;

impl AccountCapitalBalance {
    pub fn calculate(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let total_balance = database
            .all_transactions(account_id, currency)?
            .into_iter()
            .fold(dec!(0), |acc, tx| match tx.category {
                TransactionCategory::Withdrawal
                | TransactionCategory::WithdrawalTax
                | TransactionCategory::WithdrawalEarnings
                | TransactionCategory::FeeOpen(_)
                | TransactionCategory::FeeClose(_)
                | TransactionCategory::OpenTrade(_) => acc - tx.price.amount,
                TransactionCategory::Deposit
                | TransactionCategory::CloseSafetyStop(_)
                | TransactionCategory::CloseTarget(_)
                | TransactionCategory::CloseSafetyStopSlippage(_) => acc + tx.price.amount,
                _ => acc,
            });

        Ok(total_balance)
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

        let result = AccountCapitalBalance::calculate(account_id, &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_total_balance_with_positive_transactions() {
        let account_id = Uuid::new_v4();
        let mut database = MockDatabase::new();

        // One deposit transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Deposit, dec!(100));

        let result = AccountCapitalBalance::calculate(account_id, &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(200));
    }

    #[test]
    fn test_total_balance_with_negative_transactions() {
        let account_id = Uuid::new_v4();
        let mut database = MockDatabase::new();

        // One withdrawal transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));

        let result = AccountCapitalBalance::calculate(account_id, &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(50));
    }

    #[test]
    fn test_total_balance_with_open_trade_transactions() {
        let account_id = Uuid::new_v4();
        let mut database = MockDatabase::new();

        // One open trade transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(250));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));

        let result = AccountCapitalBalance::calculate(account_id, &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(150));
    }

    #[test]
    fn test_total_balance_with_close_trade_transactions() {
        let account_id = Uuid::new_v4();
        let mut database = MockDatabase::new();

        // One close trade transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(250));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(
            TransactionCategory::CloseSafetyStopSlippage(Uuid::new_v4()),
            dec!(90),
        );

        let result = AccountCapitalBalance::calculate(account_id, &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(240));
    }

    #[test]
    fn test_total_balance_with_mixed_transactions() {
        let account_id = Uuid::new_v4();
        let mut database = MockDatabase::new();

        // Mix of transactions in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(1000));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(
            TransactionCategory::CloseSafetyStopSlippage(Uuid::new_v4()),
            dec!(10),
        );

        let result = AccountCapitalBalance::calculate(account_id, &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(860));
    }

    #[test]
    fn test_total_balance_with_mixed_transactions_including_ignored_transactions() {
        let account_id = Uuid::new_v4();
        let mut database = MockDatabase::new();

        // Mix of transactions in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(1000));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(
            TransactionCategory::PaymentEarnings(Uuid::new_v4()),
            dec!(100),
        );

        database.set_transaction(
            TransactionCategory::CloseSafetyStopSlippage(Uuid::new_v4()),
            dec!(10),
        );

        let result = AccountCapitalBalance::calculate(account_id, &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(860));
    }
}
