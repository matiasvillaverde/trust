use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{ReadTransactionDB, TransactionCategory};
use uuid::Uuid;

pub struct TradeCapitalFunded;

impl TradeCapitalFunded {
    pub fn calculate(
        trade_id: Uuid,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total = dec!(0);

        for tx in database.all_trade_funding_transactions(trade_id)? {
            match tx.category {
                TransactionCategory::FundTrade(_) => {
                    // This is money that we have used to enter the market.
                    total += tx.price.amount
                }
                default => panic!(
                    "TradeCapitalFunded: does not know how to calculate transaction with category: {}",
                    default
                ),
            }
        }

        if total.is_sign_negative() {
            return Err(
                format!("TradeCapitalFunded: capital funded is negative: {}", total).into(),
            );
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

        let result = TradeCapitalFunded::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_with_positive_transactions() {
        let mut database = MockDatabase::new();

        // One deposit transaction in the database
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(83.2));

        let result = TradeCapitalFunded::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(183.2));
    }

    #[test]
    fn test_calculate_with_multiple_transactions() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(30));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(380));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(89));
        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(100));
        database.set_transaction(
            TransactionCategory::FundTrade(Uuid::new_v4()),
            dec!(8293.22),
        );

        let result = TradeCapitalFunded::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(8992.22));
    }

    #[test]
    #[should_panic(
        expected = "TradeCapitalFunded: does not know how to calculate transaction with category: withdrawal_tax"
    )]
    fn test_calculate_with_unknown_category() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::WithdrawalTax, dec!(100));

        TradeCapitalFunded::calculate(Uuid::new_v4(), &mut database).unwrap();
    }

    #[test]
    fn test_calculate_is_negative() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::FundTrade(Uuid::new_v4()), dec!(-100));

        TradeCapitalFunded::calculate(Uuid::new_v4(), &mut database)
            .expect_err("TradeCapitalFunded: capital funded is negative: -100");
    }
}
