use rust_decimal::{prelude::ToPrimitive, Decimal};
use rust_decimal_macros::dec;
use trust_model::{Currency, Database, RuleName};
use uuid::Uuid;

pub struct QuantityWorker;

impl QuantityWorker {
    pub fn maximum_quantity(
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        let overview = database.read_account_overview_currency(account_id, currency)?;
        let available = overview.total_available.amount;

        // Get rules by priority
        let mut rules = database.read_all_rules(account_id)?;
        rules.sort_by(|a, b| a.priority.cmp(&b.priority));

        // match rules by name

        for rule in rules {
            match rule.name {
                RuleName::RiskPerMonth(_risk) => {
                    unimplemented!("risk_per_month")
                }
                RuleName::RiskPerTrade(risk) => {
                    let risk_per_trade =
                        QuantityWorker::risk_per_trade(available, entry_price, stop_price, risk)?;
                    return Ok(risk_per_trade);
                }
            }
        }

        // If there are no rules, return the maximum quantity based on available funds
        Ok((available / entry_price).to_i64().unwrap())
    }

    fn risk_per_trade(
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
