use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use trust_model::{Currency, ReadTradeDB};
use uuid::Uuid;

pub struct TradeCapitalNotAtRisk;

impl TradeCapitalNotAtRisk {
    pub fn calculate(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTradeDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get the capital of the open trades that is not at risk to the total available.
        let open_trades = database.all_open_trades_for_currency(account_id, currency)?;
        let mut total_capital_not_at_risk = dec!(0.0);

        for trade in open_trades {
            let risk_per_share =
                trade.entry.unit_price.amount - trade.safety_stop.unit_price.amount;
            let total_risk = risk_per_share * Decimal::from(trade.entry.quantity);
            let total_trade = trade.entry.unit_price.amount * Decimal::from(trade.entry.quantity);
            total_capital_not_at_risk += total_trade - total_risk;
        }
        Ok(total_capital_not_at_risk)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mocks::read_transaction_db_mocks::MockDatabase;
    use rust_decimal_macros::dec;

    #[test]
    fn test_calculate_with_empty_trades() {
        let mut database = MockDatabase::new();

        let result =
            TradeCapitalNotAtRisk::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_with_one_trade() {
        let mut database = MockDatabase::new();

        database.set_trade(dec!(10), dec!(15), dec!(9), 10);

        let result =
            TradeCapitalNotAtRisk::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(90));
    }

    #[test]
    fn test_calculate_with_many_trades() {
        let mut database = MockDatabase::new();

        database.set_trade(dec!(10), dec!(15), dec!(9), 10); // 90
        database.set_trade(dec!(450), dec!(1000), dec!(440), 10); // 4400
        database.set_trade(dec!(323), dec!(1000), dec!(300), 10); // 3000
        database.set_trade(dec!(9), dec!(1000), dec!(6.4), 10); // 64
        database.set_trade(dec!(7.7), dec!(1000), dec!(4.5), 10); // 45

        let result =
            TradeCapitalNotAtRisk::calculate(Uuid::new_v4(), &Currency::USD, &mut database);
        assert_eq!(result.unwrap(), dec!(7599.0));
    }
}
