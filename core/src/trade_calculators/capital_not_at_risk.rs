use model::{Currency, ReadTradeDB};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

pub struct TradeCapitalNotAtRisk;

impl TradeCapitalNotAtRisk {
    /// This function calculates the total capital not at risk for a given account and currency.
    /// The total capital not at risk is the sum of the capital not at risk for each open trade.
    /// The capital not at risk for a trade is the difference between the entry price and the safety stop price.
    /// The capital not at risk is the amount of money that is not at risk of being lost if the trade is closed.
    ///
    /// IMPORTANT: more capital can be at risk in case the safety stops has slippage.
    ///
    /// The capital not at risk is calculated as follows:
    ///    (entry price - safety stop price) * quantity
    pub fn calculate(
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn ReadTradeDB,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all open trades for the account and currency from the database.
        let open_trades = database.all_open_trades_for_currency(account_id, currency)?;

        // Calculate the total capital not at risk by iterating over the open trades and accumulating the values.
        let total_capital_not_at_risk = open_trades.iter().fold(dec!(0.0), |acc, trade| {
            // Calculate the risk per share for the trade.
            let risk_per_share = trade.entry.unit_price - trade.safety_stop.unit_price;

            // Calculate the total capital not at risk for the trade and add it to the accumulator.
            acc + (trade.entry.unit_price - risk_per_share) * Decimal::from(trade.entry.quantity)
        });

        // Return the total capital not at risk as the result of the function.
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
