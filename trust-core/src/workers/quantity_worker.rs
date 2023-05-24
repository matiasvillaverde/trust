use std::default;

use rust_decimal::{prelude::ToPrimitive, Decimal};
use rust_decimal_macros::dec;
use trust_model::{Currency, Database, RuleName, TransactionCategory};
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

        let mut risk_per_month = dec!(0.0);

        // match rules by name

        for rule in rules {
            match rule.name {
                RuleName::RiskPerMonth(risk) => {
                    risk_per_month = QuantityWorker::calculate_max_percentage_to_risk_this_month(
                        risk, account_id, currency, database,
                    )?;
                }
                RuleName::RiskPerTrade(risk) => {
                    if risk_per_month < Decimal::from_f32_retain(risk).unwrap() {
                        return Ok(0); // No capital to risk this month, so quantity is 0. AKA: No trade.
                    } else {
                        let risk_per_trade = QuantityWorker::risk_per_trade(
                            available,
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

    fn calculate_max_percentage_to_risk_this_month(
        risk: f32,
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn Database,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Get all transactions from this month
        let transactions =
            database.all_trade_transactions_excluding_taxes(account_id, &currency)?;
        let mut total_balance_current_month = dec!(0.0);

        println!("transaction latest balance: {:?}", transactions.len());

        // Sum all transactions from this month
        for transaction in transactions {
            match transaction.category {
                TransactionCategory::Output(_) => {
                    total_balance_current_month -= transaction.price.amount
                }
                TransactionCategory::Input(_) => {
                    total_balance_current_month += transaction.price.amount
                }
                TransactionCategory::Deposit => {
                    total_balance_current_month += transaction.price.amount
                }
                TransactionCategory::Withdrawal => {
                    total_balance_current_month -= transaction.price.amount
                }
                default => panic!("Unexpected transaction category: {}", default),
            }
        }

        // Get the capital of the open trades that is not at risk to the total available.
        let open_trades = database.all_open_trades(account_id, &currency)?;
        let mut total_capital_not_at_risk = dec!(0.0);

        for trade in open_trades {
            let risk_per_share =
                trade.entry.unit_price.amount - trade.safety_stop.unit_price.amount;
            let total_risk = risk_per_share * Decimal::from(trade.entry.quantity);
            total_capital_not_at_risk += total_risk;
        }

        // Calculate all the transactions at the beginning of the month
        let mut total_beginning_of_month = dec!(0.0);
        for transaction in
            database.all_transaction_excluding_current_month_and_taxes(account_id, &currency)?
        {
            match transaction.category {
                TransactionCategory::Output(_) => {
                    total_beginning_of_month -= transaction.price.amount
                }
                TransactionCategory::Input(_) => {
                    total_beginning_of_month += transaction.price.amount
                }
                TransactionCategory::Deposit => {
                    total_beginning_of_month += transaction.price.amount
                }
                TransactionCategory::Withdrawal => {
                    total_beginning_of_month -= transaction.price.amount
                }
                default => panic!("Unexpected transaction category: {}", default),
            }
        }

        println!("total_beginning_of_month: {}", total_beginning_of_month);
        println!(
            "total_balance_current_month: {}",
            total_balance_current_month
        );
        println!("total_capital_not_at_risk: {}", total_capital_not_at_risk);

        let available_to_risk = QuantityWorker::calculate_percentage_of_capital_available_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );

        // Calculate the percentage of the total available this month
        return Ok((available_to_risk * dec!(100.0)) / total_balance_current_month);
    }

    fn calculate_percentage_of_capital_available_to_risk(
        total_beginning_of_month: Decimal,
        total_balance_current_month: Decimal,
        total_capital_not_at_risk: Decimal,
        risk: f32,
    ) -> Decimal {
        // Calculate available to risk this month
        let available_to_risk =
            (total_beginning_of_month * Decimal::from_f32_retain(risk).unwrap()) / dec!(100.0);

        // Calculate the total capital not at risk this month
        let total_performance =
            total_beginning_of_month - total_balance_current_month - total_capital_not_at_risk;

        println!("total_performance: {}", total_performance);

        if total_performance == dec!(0.0) {
            return available_to_risk; // First trade of the month. No risk yet.
        } else if (total_performance <= available_to_risk) && total_performance > dec!(0.0) {
            // We are in a loss
            let available_to_risk = available_to_risk - total_performance;
            if available_to_risk > dec!(0.0) {
                return available_to_risk; // We still have capital to risk
            }
        } else if total_performance < dec!(0.0) {
            // We are in a profit so we can risk more capital
            let total_available = total_balance_current_month + total_capital_not_at_risk;
            return (total_available * Decimal::from_f32_retain(risk).unwrap()) / dec!(100.0);
        }
        return dec!(0.0); // No more capital to risk
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_calculate_percentage_of_capital_available_to_risk_same_capital() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(1000, 0);
        let total_capital_not_at_risk = Decimal::new(0, 0);
        let risk = 10.0;

        // First trade of the month
        let result = QuantityWorker::calculate_percentage_of_capital_available_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(100, 0));
    }

    #[test]
    fn test_calculate_percentage_of_capital_available_to_risk_same_capital_with_capital_not_at_risk(
    ) {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(900, 0);
        let total_capital_not_at_risk = Decimal::new(100, 0);
        let risk = 10.0;

        // First trade of the month
        let result = QuantityWorker::calculate_percentage_of_capital_available_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(100, 0));
    }

    #[test]
    fn test_calculate_percentage_of_capital_available_to_risk_in_a_loss() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(950, 0);
        let total_capital_not_at_risk = Decimal::new(0, 0);
        let risk = 10.0;

        // In a loss
        let result = QuantityWorker::calculate_percentage_of_capital_available_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(50, 0));
    }

    #[test]
    fn test_calculate_percentage_of_capital_available_to_risk_in_a_loss_with_capital_not_at_risk() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(900, 0);
        let total_capital_not_at_risk = Decimal::new(50, 0);
        let risk = 10.0;

        // In a loss
        let result = QuantityWorker::calculate_percentage_of_capital_available_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(50, 0));
    }

    #[test]
    fn test_calculate_percentage_of_capital_available_to_risk_no_more_capital() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(900, 0);
        let total_capital_not_at_risk = Decimal::new(0, 0);
        let risk = 10.0;

        // No more capital to risk
        let result = QuantityWorker::calculate_percentage_of_capital_available_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(0, 0));
    }

    #[test]
    fn test_calculate_percentage_of_capital_available_to_risk_no_more_capital_with_capital_not_at_risk(
    ) {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(800, 0);
        let total_capital_not_at_risk = Decimal::new(100, 0);
        let risk = 10.0;

        // No more capital to risk
        let result = QuantityWorker::calculate_percentage_of_capital_available_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(0, 0));
    }

    #[test]
    fn test_calculate_percentage_of_capital_available_to_risk_in_profit() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(1500, 0);
        let total_capital_not_at_risk = Decimal::new(0, 0);
        let risk = 10.0;

        // In a profit
        let result = QuantityWorker::calculate_percentage_of_capital_available_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(150, 0));
    }

    #[test]
    fn test_calculate_percentage_of_capital_available_to_risk_in_profit_with_capital_not_at_risk() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(1000, 0);
        let total_capital_not_at_risk = Decimal::new(500, 0);
        let risk = 10.0;

        // In a profit
        let result = QuantityWorker::calculate_percentage_of_capital_available_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(150, 0));
    }
}
