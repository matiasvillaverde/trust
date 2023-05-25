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
        let total_available = TransactionsCalculator::calculate_total_capital_available(
            account_id, currency, database,
        )?;

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
                        )?;
                        return Ok(risk_per_trade);
                    }
                }
            }
        }

        // If there are no rules, return the maximum quantity based on available funds
        Ok((total_available / entry_price).to_i64().unwrap())
    }

    fn max_quantity_per_trade(
        // TODO: Test this function
        available: Decimal,
        entry_price: Decimal,
        stop_price: Decimal,
        risk: f32,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let risk = available * (Decimal::from_f32_retain(risk).unwrap() / dec!(100.0));
        let risk_per_trade = risk / (entry_price - stop_price);
        let risk_per_trade = risk_per_trade.to_i64().unwrap();
        Ok(risk_per_trade)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
}
