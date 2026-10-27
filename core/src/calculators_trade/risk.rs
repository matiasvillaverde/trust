use model::{Currency, DatabaseFactory, Rule, RuleName, Trade};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

use crate::calculators_account::{AccountCapitalAvailable, AccountCapitalBeginningOfMonth};
use crate::calculators_trade::TradeCapitalNotAtRisk;

pub struct RiskCalculator;

impl RiskCalculator {
    /// Resolve caps by rule type, independent of persisted ordering. Duplicate rules
    /// can only tighten a cap. A monthly-only policy remains an enforced limit.
    pub fn effective_rules(
        account_id: Uuid,
        currency: &Currency,
        rules: &[Rule],
        database: &mut dyn DatabaseFactory,
    ) -> Result<(Option<Decimal>, Decimal), Box<dyn std::error::Error>> {
        Self::effective_rules_for_reservation(account_id, currency, rules, database, None)
    }

    pub(crate) fn effective_rules_for_reservation(
        account_id: Uuid,
        currency: &Currency,
        rules: &[Rule],
        database: &mut dyn DatabaseFactory,
        reservation: Option<&Trade>,
    ) -> Result<(Option<Decimal>, Decimal), Box<dyn std::error::Error>> {
        let mut per_trade: Option<Decimal> = None;
        let mut per_month = dec!(100);
        for rule in rules {
            let risk = rule.name.risk_decimal()?;
            match rule.name {
                RuleName::RiskPerTrade(_) => {
                    per_trade = Some(per_trade.map_or(risk, |previous| previous.min(risk)))
                }
                RuleName::RiskPerMonth(value) => {
                    let monthly = if reservation.is_some() {
                        Self::monthly_percentage(
                            value,
                            account_id,
                            currency,
                            database,
                            reservation,
                        )?
                    } else {
                        Self::calculate_max_percentage_to_risk_current_month(
                            value, account_id, currency, database,
                        )?
                    };
                    per_month = per_month.min(monthly);
                }
            }
        }
        Ok((per_trade, per_month))
    }

    pub fn calculate_max_percentage_to_risk_current_month(
        risk: f32,
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        Self::monthly_percentage(risk, account_id, currency, database, None)
    }

    fn monthly_percentage(
        risk: f32,
        account_id: Uuid,
        currency: &Currency,
        database: &mut dyn DatabaseFactory,
        reservation: Option<&Trade>,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        // Calculate the total available this month.
        let mut total_available = AccountCapitalAvailable::calculate(
            account_id,
            currency,
            database.transaction_read().as_mut(),
        )?;

        // Calculate the capital of the open trades that is not at risk.
        let mut total_capital_not_at_risk =
            TradeCapitalNotAtRisk::calculate(account_id, currency, database.trade_read().as_mut())?;

        // Evaluate an already funded trade as a fresh reservation without writing a
        // refund or changing the monthly opening basis. All other reservations remain.
        if let Some(trade) = reservation {
            total_available = total_available
                .checked_add(trade.balance.capital_out_market)
                .ok_or("Submission capital overflow")?;
            total_capital_not_at_risk = total_capital_not_at_risk
                .checked_sub(super::capital_not_at_risk::protected_capital(trade)?)
                .ok_or("Submission protected capital overflow")?;
        }
        if total_available <= Decimal::ZERO {
            return Ok(Decimal::ZERO);
        }

        // Calculate the total capital at the beginning of the month.
        let total_beginning_of_month =
            AccountCapitalBeginningOfMonth::risk_reference(account_id, currency, database)?;

        let available_to_risk = RiskCalculator::calculate_capital_allowed_to_risk(
            total_beginning_of_month,
            total_available,
            total_capital_not_at_risk,
            risk,
        );

        // Calculate the percentage of the total available this month
        let temp = available_to_risk
            .checked_mul(dec!(100.0))
            .ok_or("Multiplication overflow in risk calculation")?;
        let percentage = temp
            .checked_div(total_available)
            .ok_or("Division by zero or overflow in risk calculation")?;
        Ok(percentage)
    }

    fn calculate_capital_allowed_to_risk(
        total_beginning_of_month: Decimal,
        total_balance_current_month: Decimal,
        total_capital_not_at_risk: Decimal,
        risk: f32,
    ) -> Decimal {
        let Ok(risk_decimal) = RuleName::RiskPerMonth(risk).risk_decimal() else {
            return dec!(0.0); // Failed to convert risk to Decimal
        };

        let Some(temp) = total_beginning_of_month.checked_mul(risk_decimal) else {
            return dec!(0.0); // Multiplication overflow
        };

        let Some(available_to_risk) = temp.checked_div(dec!(100.0)) else {
            return dec!(0.0); // Division overflow
        };

        let Some(temp1) = total_beginning_of_month.checked_sub(total_balance_current_month) else {
            return dec!(0.0); // Subtraction overflow
        };

        let Some(total_performance) = temp1.checked_sub(total_capital_not_at_risk) else {
            return dec!(0.0); // Subtraction overflow
        };

        // If there is no change in performance, return the available amount to be risked.
        if total_performance == dec!(0.0) {
            return available_to_risk;
        }

        let mut risked_capital = dec!(0.0);

        // If there is no change in performance, return the available amount to be risked.
        if total_performance < dec!(0.0) {
            let Some(total_available) =
                total_balance_current_month.checked_add(total_capital_not_at_risk)
            else {
                return dec!(0.0); // Addition overflow
            };

            let Some(temp2) = total_available.checked_mul(risk_decimal) else {
                return dec!(0.0); // Multiplication overflow
            };

            risked_capital = temp2.checked_div(dec!(100.0)).unwrap_or(dec!(0.0));
        } else if total_performance <= available_to_risk {
            // If there is an increase in performance,
            // calculate the difference between available capital and risked capital.
            risked_capital = available_to_risk
                .checked_sub(total_performance)
                .unwrap_or(dec!(0.0));
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

    #[test]
    fn test_calculate_capital_allowed_to_risk_returns_zero_for_invalid_risk_input() {
        let result = RiskCalculator::calculate_capital_allowed_to_risk(
            Decimal::new(1000, 0),
            Decimal::new(1000, 0),
            Decimal::new(0, 0),
            f32::NAN,
        );

        assert_eq!(result, Decimal::ZERO);
    }

    #[test]
    fn test_calculate_capital_allowed_to_risk_returns_zero_for_overflow_paths() {
        let cases = [
            (Decimal::MAX, Decimal::ZERO, Decimal::ZERO, 200.0),
            (Decimal::MAX, dec!(-1), Decimal::ZERO, 0.0),
            (Decimal::ZERO, Decimal::MAX, Decimal::ONE, 0.0),
            (Decimal::MAX, Decimal::MAX, Decimal::ONE, 0.0),
            (Decimal::ZERO, Decimal::MAX, Decimal::ZERO, 200.0),
        ];

        for (beginning, current, not_at_risk, risk) in cases {
            assert_eq!(
                RiskCalculator::calculate_capital_allowed_to_risk(
                    beginning,
                    current,
                    not_at_risk,
                    risk,
                ),
                Decimal::ZERO
            );
        }
    }
}
