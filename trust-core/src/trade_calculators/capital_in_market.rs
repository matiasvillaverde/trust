use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{ReadTransactionDB, TransactionCategory};
use uuid::Uuid;

pub struct TradeCapitalInMarket;

impl TradeCapitalInMarket {
    pub fn calculate(
        trade_id: Uuid,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total = dec!(0);

        for tx in database.all_trade_transactions(trade_id)? {
            match tx.category {
                TransactionCategory::FundTrade(_) | TransactionCategory::PaymentFromTrade(_) => {
                    // Nothing
                }
                TransactionCategory::OpenTrade(_) => {
                    // This is money that we have used to enter the market.
                    total += tx.price.amount
                }
                TransactionCategory::CloseTarget(_)
                | TransactionCategory::CloseSafetyStop(_)
                | TransactionCategory::CloseSafetyStopSlippage(_) => {
                    total = Decimal::from(0) // We have exited the market, so we have no money in the market.
                },
                TransactionCategory::FeeOpen(_) | TransactionCategory::FeeClose(_) | TransactionCategory::PaymentTax(_) | TransactionCategory::PaymentEarnings(_)  => {
                    // We ignore the fees because they are charged from the account and not from the trade.
                }
                default => panic!(
                    "TradeCapitalInMarket: does not know how to calculate transaction with category: {}",
                    default
                ),
            }
        }

        if total.is_sign_negative() {
            return Err(format!("TradeCapitalInMarket: capital is negative: {}", total).into());
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

        let result = TradeCapitalInMarket::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_with_transactions_to_ignore() {
        let mut database = MockDatabase::new();

        // One deposit transaction in the database
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(
            TransactionCategory::PaymentFromTrade(Uuid::new_v4()),
            dec!(100),
        );
        database.set_transaction(TransactionCategory::FeeOpen(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::FeeClose(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(100));
        database.set_transaction(
            TransactionCategory::PaymentEarnings(Uuid::new_v4()),
            dec!(83.2),
        );

        let result = TradeCapitalInMarket::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_with_multiple_positive_transaction() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(30));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(380));

        let result = TradeCapitalInMarket::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(510));
    }

    #[test]
    fn test_calculate_with_transaction_close_target() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::CloseTarget(Uuid::new_v4()), dec!(30));

        let result = TradeCapitalInMarket::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_with_transaction_close_safety_stop() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(
            TransactionCategory::CloseSafetyStop(Uuid::new_v4()),
            dec!(30),
        );

        let result = TradeCapitalInMarket::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_with_transaction_close_safety_stop_slippage() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(
            TransactionCategory::CloseSafetyStopSlippage(Uuid::new_v4()),
            dec!(30),
        );

        let result = TradeCapitalInMarket::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    #[should_panic(
        expected = "TradeCapitalInMarket: does not know how to calculate transaction with category: withdrawal_tax"
    )]
    fn test_calculate_with_unknown_category() {
        let trade_id = Uuid::new_v4();
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::WithdrawalTax, dec!(100));

        TradeCapitalInMarket::calculate(trade_id, &mut database).unwrap();
    }

    #[test]
    fn test_calculate_is_negative() {
        let trade_id = Uuid::new_v4();
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(-100));

        TradeCapitalInMarket::calculate(trade_id, &mut database)
            .expect_err("TradeCapitalInMarket: capital funded is negative: -100");
    }
}
