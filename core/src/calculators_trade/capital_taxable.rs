use model::{ReadTransactionDB, TransactionCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

pub struct TradeCapitalTaxable;

impl TradeCapitalTaxable {
    pub fn calculate(
        trade_id: Uuid,
        database: &mut dyn ReadTransactionDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let mut total = dec!(0);

        for tx in database.all_trade_taxes_transactions(trade_id)? {
            match tx.category {
                TransactionCategory::PaymentTax(_) => {
                    total += tx.amount
                }
                default => panic!(
                    "TradeCapitalTaxable: does not know how to calculate transaction with category: {default}"
                ),
            }
        }

        if total.is_sign_negative() {
            return Err(format!("TradeCapitalTaxable: is negative: {total}").into());
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

        let result = TradeCapitalTaxable::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_with_multiple_transaction() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(10.5));
        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(100));
        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(1));
        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(1));

        let result = TradeCapitalTaxable::calculate(Uuid::new_v4(), &mut database);
        assert_eq!(result.unwrap(), dec!(112.5));
    }

    #[test]
    #[should_panic(
        expected = "TradeCapitalTaxable: does not know how to calculate transaction with category: withdrawal_tax"
    )]
    fn test_calculate_with_unknown_category() {
        let mut database = MockDatabase::new();

        // Transactions
        database.set_transaction(TransactionCategory::WithdrawalTax, dec!(100));

        TradeCapitalTaxable::calculate(Uuid::new_v4(), &mut database).unwrap();
    }

    #[test]
    fn test_calculate_is_negative() {
        let mut database = MockDatabase::new();

        database.set_transaction(TransactionCategory::PaymentTax(Uuid::new_v4()), dec!(-1));

        TradeCapitalTaxable::calculate(Uuid::new_v4(), &mut database)
            .expect_err("TradeCapitalTaxable: taxable is negative: -1");
    }
}
