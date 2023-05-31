use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{Currency, ReadTransactionDB, TransactionCategory};
use uuid::Uuid;

pub struct CapitalBalanceCalculator;

impl CapitalBalanceCalculator {
    pub fn total_balance(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total_balance = dec!(0.0);
        // Get all transactions
        for tx in database.all_transactions(account_id, currency)? {
            match tx.category {
                TransactionCategory::Withdrawal
                | TransactionCategory::WithdrawalTax
                | TransactionCategory::WithdrawalEarnings
                | TransactionCategory::FeeOpen(_)
                | TransactionCategory::FeeClose(_) => {
                    total_balance -= tx.price.amount;
                }
                TransactionCategory::Deposit => {
                    total_balance += tx.price.amount;
                }
                TransactionCategory::OpenTrade(_) => {
                    total_balance -= tx.price.amount; // The money is in the market it counts at negative.
                }
                TransactionCategory::CloseSafetyStop(_)
                | TransactionCategory::CloseTarget(_)
                | TransactionCategory::CloseSafetyStopSlippage(_) => {
                    total_balance += tx.price.amount; // We add the money that we get by exit the market.
                }
                _ => { // We don't want to count the transactions for taxes, earnings and funding trades.
                }
            }
        }

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
        let currency = Currency::USD;
        let mut database = MockDatabase::new();

        let result = CapitalBalanceCalculator::total_balance(account_id, &currency, &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_total_balance_with_positive_transactions() {
        let account_id = Uuid::new_v4();
        let currency = Currency::USD;
        let mut database = MockDatabase::new();

        // One deposit transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Deposit, dec!(100));

        let result = CapitalBalanceCalculator::total_balance(account_id, &currency, &mut database);
        assert_eq!(result.unwrap(), dec!(200));
    }

    #[test]
    fn test_total_balance_with_negative_transactions() {
        let account_id = Uuid::new_v4();
        let currency = Currency::USD;
        let mut database = MockDatabase::new();

        // One withdrawal transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(50));

        let result = CapitalBalanceCalculator::total_balance(account_id, &currency, &mut database);
        assert_eq!(result.unwrap(), dec!(50));
    }

    #[test]
    fn test_total_balance_with_open_trade_transactions() {
        let account_id = Uuid::new_v4();
        let currency = Currency::USD;
        let mut database = MockDatabase::new();

        // One open trade transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(250));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));

        let result = CapitalBalanceCalculator::total_balance(account_id, &currency, &mut database);
        assert_eq!(result.unwrap(), dec!(150));
    }

    #[test]
    fn test_total_balance_with_close_trade_transactions() {
        let account_id = Uuid::new_v4();
        let currency = Currency::USD;
        let mut database = MockDatabase::new();

        // One close trade transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(250));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(
            TransactionCategory::CloseSafetyStopSlippage(Uuid::new_v4()),
            dec!(90),
        );

        let result = CapitalBalanceCalculator::total_balance(account_id, &currency, &mut database);
        assert_eq!(result.unwrap(), dec!(240));
    }

    #[test]
    fn test_total_balance_with_mixed_transactions() {
        let account_id = Uuid::new_v4();
        let currency = Currency::USD;
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

        let result = CapitalBalanceCalculator::total_balance(account_id, &currency, &mut database);
        assert_eq!(result.unwrap(), dec!(860));
    }

    #[test]
    fn test_total_balance_with_mixed_transactions_including_ignored_transactions() {
        let account_id = Uuid::new_v4();
        let currency = Currency::USD;
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

        let result = CapitalBalanceCalculator::total_balance(account_id, &currency, &mut database);
        assert_eq!(result.unwrap(), dec!(860));
    }
}
