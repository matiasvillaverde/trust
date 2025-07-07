use model::{Currency, DatabaseFactory, RuleName};
use rust_decimal::{prelude::ToPrimitive, Decimal};
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::calculators_account::AccountCapitalAvailable;
use crate::calculators_trade::RiskCalculator;

pub struct QuantityCalculator;

impl QuantityCalculator {
    pub fn maximum_quantity(
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        currency: &Currency,
        database: &mut dyn DatabaseFactory,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let total_available = AccountCapitalAvailable::calculate(
            account_id,
            currency,
            database.transaction_read().as_mut(),
        )?;

        // Get rules by priority
        let mut rules = database.rule_read().read_all_rules(account_id)?;
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
                    let risk_decimal = Decimal::from_f32_retain(risk)
                        .ok_or_else(|| format!("Failed to convert risk {} to Decimal", risk))?;
                    if risk_per_month < risk_decimal {
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
        let max_quantity = total_available.checked_div(entry_price).ok_or_else(|| {
            format!(
                "Division by zero or overflow: {} / {}",
                total_available, entry_price
            )
        })?;
        max_quantity
            .to_i64()
            .ok_or_else(|| format!("Cannot convert {} to i64", max_quantity).into())
    }

    fn max_quantity_per_trade(
        available: Decimal,
        entry_price: Decimal,
        stop_price: Decimal,
        risk: f32,
    ) -> i64 {
        assert!(available > dec!(0.0));
        let price_diff = entry_price
            .checked_sub(stop_price)
            .expect("Entry price must be greater than stop price");
        assert!(price_diff != dec!(0.0));
        assert!(risk > 0.0);

        let max_quantity = available
            .checked_div(entry_price)
            .expect("Division overflow");
        let max_risk = max_quantity
            .checked_mul(price_diff)
            .expect("Multiplication overflow");

        let risk_decimal =
            Decimal::from_f32_retain(risk).expect("Failed to convert risk to Decimal");
        let risk_percent = risk_decimal
            .checked_div(dec!(100.0))
            .expect("Division overflow");
        let risk_capital = available
            .checked_mul(risk_percent)
            .expect("Multiplication overflow");

        if risk_capital >= max_risk {
            // The risk capital is greater than the max risk, so return the max quantity
            max_quantity.to_i64().expect("Cannot convert to i64")
        } else {
            // The risk capital is less than the max risk, so return the max quantity based on the risk capital
            let risk_per_trade = risk_capital
                .checked_div(price_diff)
                .expect("Division overflow");
            risk_per_trade.to_i64().expect("Cannot convert to i64") // We round down to the nearest integer
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
