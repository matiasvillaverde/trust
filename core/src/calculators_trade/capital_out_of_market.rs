use model::{ReadTransactionDB, TransactionCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

pub struct TradeCapitalOutOfMarket;

impl TradeCapitalOutOfMarket {
    pub fn calculate(
        trade_id: Uuid,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total = dec!(0);

        for tx in database.all_trade_transactions(trade_id)? {
            match tx.category {
                TransactionCategory::FundTrade(_) => {
                    // This is money that we have put into the trade
                    total += tx.amount
                }
                TransactionCategory::PaymentFromTrade(_) => {
                    // This is money that we have extracted from the trade
                    total -= tx.amount
                }
                TransactionCategory::OpenTrade(_) => {
                    // This is money that we have used to enter the market.
                    total -= tx.amount
                }
                TransactionCategory::CloseTarget(_) => {
                    // This is money that we have used to exit the market.
                    total += tx.amount
                }
                TransactionCategory::CloseSafetyStop(_) => {
                    // This is money that we have used to exit the market at a loss.
                    total += tx.amount
                }
                TransactionCategory::CloseSafetyStopSlippage(_) => {
                    // This is money that we have used to exit the market at a loss - slippage.
                    total += tx.amount
                },
                TransactionCategory::FeeOpen(_) | TransactionCategory::FeeClose(_) | TransactionCategory::PaymentTax(_) | TransactionCategory::PaymentEarnings(_) => {
                    // We ignore the fees because they are charged from the account and not from the trade.
                }
                default => panic!(
                    "TradeCapitalOutOfMarket: does not know how to calculate transaction with category: {}",
                    default
                ),
            }
        }

        if total.is_sign_negative() {
            return Err(format!("TradeCapitalOutOfMarket: capital is negative: {}", total).into());
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

        let result = TradeCapitalOutOfMarket::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_with_transactions_to_ignore() {
        let mut database = MockDatabase::new();

        // One deposit transaction in the database
        database.set_transaction(TransactionCategory::FeeOpen(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::FeeClose(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(100));
        database.set_transaction(
            TransactionCategory::PaymentEarnings(Uuid::new_v4()),
            dec!(83.2),
        );

        let result = TradeCapitalOutOfMarket::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_with_multiple_transaction() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::FeeOpen(Uuid::new_v4()), dec!(1));
        database.set_transaction(TransactionCategory::FeeClose(Uuid::new_v4()), dec!(1));
        database.set_transaction(TransactionCategory::CloseTarget(Uuid::new_v4()), dec!(380));

        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(20));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(20));
        database.set_transaction(TransactionCategory::FeeOpen(Uuid::new_v4()), dec!(1));
        database.set_transaction(TransactionCategory::FeeClose(Uuid::new_v4()), dec!(1));
        database.set_transaction(
            TransactionCategory::CloseSafetyStopSlippage(Uuid::new_v4()),
            dec!(10),
        );

        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(5));
        database.set_transaction(TransactionCategory::OpenTrade(Uuid::new_v4()), dec!(5));
        database.set_transaction(TransactionCategory::FeeOpen(Uuid::new_v4()), dec!(1));
        database.set_transaction(TransactionCategory::FeeClose(Uuid::new_v4()), dec!(1));
        database.set_transaction(
            TransactionCategory::CloseSafetyStop(Uuid::new_v4()),
            dec!(3),
        );

        let result = TradeCapitalOutOfMarket::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(393));
    }

    #[test]
    #[should_panic(
        expected = "TradeCapitalOutOfMarket: does not know how to calculate transaction with category: withdrawal_tax"
    )]
    fn test_calculate_with_unknown_category() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::WithdrawalTax, dec!(100));

        TradeCapitalOutOfMarket::calculate(Uuid::new_v4(), &mut database).unwrap();
    }

    #[test]
    fn test_calculate_is_negative() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(-100));

        TradeCapitalOutOfMarket::calculate(Uuid::new_v4(), &mut database)
            .expect_err("TradeCapitalOutOfMarket: out of market is negative: -100");
    }
}
