use rust_decimal::{prelude::ToPrimitive, Decimal};
use rust_decimal_macros::dec;
use trust_model::{Currency, Database, RuleName};
use uuid::Uuid;

use super::{RiskCalculator, TransactionsCalculator};

pub struct QuantityCalculator;

impl QuantityCalculator {
    pub fn maximum_quantity(
        // TODO: Test this function
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let total_available =
            TransactionsCalculator::capital_available(account_id, currency, database)?;

        // Get rules by priority
        let mut rules = database.read_all_rules(account_id)?;
        rules.sort_by(|a, b| a.priority.cmp(&b.priority));

        let mut risk_per_month = dec!(100.0); // Default to 100% of the available capital

        // match rules by name
        for rule in rules {
            match rule.name {
                RuleName::RiskPerMonth(risk) => {
                    risk_per_month =
                        RiskCalculator::calculate_max_percentage_to_risk_current_month(
                            risk, account_id, currency, database,
                        )?;
                }
                RuleName::RiskPerTrade(risk) => {
                    if risk_per_month < Decimal::from_f32_retain(risk).unwrap() {
                        return Ok(0); // No capital to risk this month, so quantity is 0. AKA: No trade.
                    } else {
                        let risk_per_trade = QuantityCalculator::max_quantity_per_trade(
                            total_available,
                            entry_price,
                            stop_price,
                            risk,
                        );
                        return Ok(risk_per_trade);
                    }
                }
            }
        }

        // If there are no rules, return the maximum quantity based on available funds
        Ok((total_available / entry_price).to_i64().unwrap())
    }

    fn max_quantity_per_trade(
        available: Decimal,
        entry_price: Decimal,
        stop_price: Decimal,
        risk: f32,
    ) -> i64 {
        assert!(available > dec!(0.0));
        assert!(entry_price - stop_price != dec!(0.0));
        assert!(risk > 0.0);

        let max_quantity = available / entry_price;
        let max_risk = max_quantity * (entry_price - stop_price);

        let risk_capital = available * (Decimal::from_f32_retain(risk).unwrap() / dec!(100.0));

        if risk_capital >= max_risk {
            // The risk capital is greater than the max risk, so return the max quantity
            max_quantity.to_i64().unwrap()
        } else {
            // The risk capital is less than the max risk, so return the max quantity based on the risk capital
            let risk_per_trade = risk_capital / (entry_price - stop_price);
            risk_per_trade.to_i64().unwrap() // We round down to the nearest integer
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_quantity_per_trade_default() {
        // Test case 1: The trade risk is within the available funds
        let available = dec!(10_000);
        let entry_price = dec!(50);
        let stop_price = dec!(45);
        let risk = 2.0; // 2% risk

        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(available, entry_price, stop_price, risk),
            40
        );
    }

    #[test]
    fn test_max_quantity_per_trade_low_risk() {
        // Test case 2: The trade risk is greater than the available funds
        let available = dec!(10_000);
        let entry_price = dec!(100);
        let stop_price = dec!(90);
        let risk = 0.1;

        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(available, entry_price, stop_price, risk),
            1
        );
    }

    #[test]
    fn test_max_quantity_per_trade_high_risk() {
        let available = dec!(10_000);
        let entry_price = dec!(100);
        let stop_price = dec!(90);
        let risk = 90.0;

        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(available, entry_price, stop_price, risk),
            100
        );
    }

    #[test]
    fn test_max_quantity_per_trade_max_risk() {
        let available = dec!(10_000);
        let entry_price = dec!(100);
        let stop_price = dec!(90);
        let risk = 100.0;

        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(available, entry_price, stop_price, risk),
            100
        );
    }

    #[test]
    fn test_max_quantity_per_trade_less_than_maximum_risk() {
        let available = dec!(10_000);
        let entry_price = dec!(100);
        let stop_price = dec!(90);
        let risk = 9.99;

        assert_eq!(
            QuantityCalculator::max_quantity_per_trade(available, entry_price, stop_price, risk),
            99
        );
    }
}
