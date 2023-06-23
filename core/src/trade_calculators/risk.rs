use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use model::{Currency, DatabaseFactory};
use uuid::Uuid;

use crate::account_calculators::{AccountCapitalAvailable, AccountCapitalBeginningOfMonth};
use crate::trade_calculators::TradeCapitalNotAtRisk;

pub struct RiskCalculator;

impl RiskCalculator {
    pub fn calculate_max_percentage_to_risk_current_month(
        risk: f32,
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Calculate the total available this month.
        let total_available = AccountCapitalAvailable::calculate(
            account_id,
            currency,
            database.read_transaction_db().as_mut(),
        )?;

        // Calculate the capital of the open trades that is not at risk.
        let total_capital_not_at_risk = TradeCapitalNotAtRisk::calculate(
            account_id,
            currency,
            database.read_trade_db().as_mut(),
        )?;

        // Calculate the total capital at the beginning of the month.
        let total_beginning_of_month = AccountCapitalBeginningOfMonth::calculate(
            account_id,
            currency,
            database.read_transaction_db().as_mut(),
        )?;

        let available_to_risk = RiskCalculator::calculate_capital_allowed_to_risk(
            total_beginning_of_month,
            total_available,
            total_capital_not_at_risk,
            risk,
        );

        // Calculate the percentage of the total available this month
        Ok((available_to_risk * dec!(100.0)) / total_available)
    }

    fn calculate_capital_allowed_to_risk(
        total_beginning_of_month: Decimal,
        total_balance_current_month: Decimal,
        total_capital_not_at_risk: Decimal,
        risk: f32,
    ) -> Decimal {
        let available_to_risk =
            total_beginning_of_month * Decimal::from_f32_retain(risk).unwrap() / dec!(100.0);
        let total_performance =
            total_beginning_of_month - total_balance_current_month - total_capital_not_at_risk;

        // If there is no change in performance, return the available amount to be risked.
        if total_performance == dec!(0.0) {
            return available_to_risk;
        }

        let mut risked_capital = dec!(0.0);

        // If there is no change in performance, return the available amount to be risked.
        if total_performance < dec!(0.0) {
            let total_available = total_balance_current_month + total_capital_not_at_risk;
            risked_capital =
                total_available * Decimal::from_f32_retain(risk).unwrap() / dec!(100.0);
        } else if total_performance <= available_to_risk {
            // If there is an increase in performance,
            // calculate the difference between available capital and risked capital.
            risked_capital = available_to_risk - total_performance;
        }

        // Return the maximum value of the risked capital or zero.
        risked_capital.max(dec!(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_calculate_capital_allowed_to_risk_is_0() {
        let total_beginning_of_month = Decimal::new(0, 0);
        let total_balance_current_month = Decimal::new(0, 0);
        let total_capital_not_at_risk = Decimal::new(0, 0);
        let risk = 10.0;

        let result = RiskCalculator::calculate_capital_allowed_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(0, 0));
    }

    #[test]
    fn test_calculate_capital_allowed_to_risk_is_0_at_beginning_of_month() {
        let total_beginning_of_month = Decimal::new(0, 0);
        let total_balance_current_month = Decimal::new(10000, 0);
        let total_capital_not_at_risk = Decimal::new(0, 0);
        let risk = 10.0;

        let result = RiskCalculator::calculate_capital_allowed_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(1000, 0));
    }

    #[test]
    fn test_calculate_capital_allowed_to_risk_same_capital() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(1000, 0);
        let total_capital_not_at_risk = Decimal::new(0, 0);
        let risk = 10.0;

        let result = RiskCalculator::calculate_capital_allowed_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(100, 0));
    }

    #[test]
    fn test_calculate_capital_allowed_to_risk_same_capital_with_capital_not_at_risk() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(900, 0);
        let total_capital_not_at_risk = Decimal::new(100, 0);
        let risk = 10.0;

        let result = RiskCalculator::calculate_capital_allowed_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(100, 0));
    }

    #[test]
    fn test_calculate_capital_allowed_to_risk_in_a_loss() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(950, 0);
        let total_capital_not_at_risk = Decimal::new(0, 0);
        let risk = 10.0;

        // In a loss
        let result = RiskCalculator::calculate_capital_allowed_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(50, 0));
    }

    #[test]
    fn test_calculate_capital_allowed_to_risk_in_a_loss_with_capital_not_at_risk() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(900, 0);
        let total_capital_not_at_risk = Decimal::new(50, 0);
        let risk = 10.0;

        // In a loss
        let result = RiskCalculator::calculate_capital_allowed_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(50, 0));
    }

    #[test]
    fn test_calculate_capital_allowed_to_risk_no_more_capital() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(900, 0);
        let total_capital_not_at_risk = Decimal::new(0, 0);
        let risk = 10.0;

        // No more capital to risk
        let result = RiskCalculator::calculate_capital_allowed_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(0, 0));
    }

    #[test]
    fn test_calculate_capital_allowed_to_risk_no_more_capital_with_capital_not_at_risk() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(800, 0);
        let total_capital_not_at_risk = Decimal::new(100, 0);
        let risk = 10.0;

        // No more capital to risk
        let result = RiskCalculator::calculate_capital_allowed_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(0, 0));
    }

    #[test]
    fn test_calculate_capital_allowed_to_risk_in_profit() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(1500, 0);
        let total_capital_not_at_risk = Decimal::new(0, 0);
        let risk = 10.0;

        // In a profit
        let result = RiskCalculator::calculate_capital_allowed_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(150, 0));
    }

    #[test]
    fn test_calculate_capital_allowed_to_risk_in_profit_with_capital_not_at_risk() {
        let total_beginning_of_month = Decimal::new(1000, 0);
        let total_balance_current_month = Decimal::new(1000, 0);
        let total_capital_not_at_risk = Decimal::new(500, 0);
        let risk = 10.0;

        // In a profit
        let result = RiskCalculator::calculate_capital_allowed_to_risk(
            total_beginning_of_month,
            total_balance_current_month,
            total_capital_not_at_risk,
            risk,
        );
        assert_eq!(result, Decimal::new(150, 0));
    }
}
