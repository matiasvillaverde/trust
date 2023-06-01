use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{ReadTransactionDB, TransactionCategory};
use uuid::Uuid;

pub struct TradePerformance;

impl TradePerformance {
    pub fn calculate(
        trade_id: Uuid,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total = dec!(0);

        for tx in database.all_trade_transactions(trade_id)? {
            match tx.category {
                TransactionCategory::OpenTrade(_)
                | TransactionCategory::FeeClose(_)
                | TransactionCategory::FeeOpen(_)
                | TransactionCategory::PaymentTax(_) => total -= tx.price.amount,

                TransactionCategory::CloseTarget(_)
                | TransactionCategory::CloseSafetyStop(_)
                | TransactionCategory::CloseSafetyStopSlippage(_) => total += tx.price.amount,
                _ => {} // We don't want to count the transactions paid out of the trade or fund the trade.
            }
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
    fn test_calculate_with_empty_transactions() {
        let mut database = MockDatabase::new();

        let result = TradePerformance::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_with_transactions_to_ignore() {
        let mut database = MockDatabase::new();

        // One deposit transaction in the database
        database.set_transaction(TransactionCategory::Deposit, dec!(100));
        database.set_transaction(TransactionCategory::Withdrawal, dec!(100));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(
            TransactionCategory::PaymentEarnings(Uuid::new_v4()),
            dec!(100),
        );
        database.set_transaction(TransactionCategory::WithdrawalEarnings, dec!(100));

        let result = TradePerformance::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_with_transactions_hit_target() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::FeeClose(Uuid::new_v4()), dec!(1));
        database.set_transaction(TransactionCategory::FeeOpen(Uuid::new_v4()), dec!(1));
        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(20));
        database.set_transaction(TransactionCategory::CloseTarget(Uuid::new_v4()), dec!(200));

        let result = TradePerformance::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(78));
    }

    #[test]
    fn test_calculate_with_transactions_hit_safety_stop() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::FeeClose(Uuid::new_v4()), dec!(1));
        database.set_transaction(TransactionCategory::FeeOpen(Uuid::new_v4()), dec!(1));
        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(0));
        database.set_transaction(
            TransactionCategory::CloseSafetyStop(Uuid::new_v4()),
            dec!(80),
        );

        let result = TradePerformance::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(-22));
    }

    #[test]
    fn test_calculate_with_transactions_hit_safety_stop_slippage() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::FeeClose(Uuid::new_v4()), dec!(1));
        database.set_transaction(TransactionCategory::FeeOpen(Uuid::new_v4()), dec!(1));
        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(0));
        database.set_transaction(
            TransactionCategory::CloseSafetyStopSlippage(Uuid::new_v4()),
            dec!(50),
        );

        let result = TradePerformance::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(-52));
    }
}
