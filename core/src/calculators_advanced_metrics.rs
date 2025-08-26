//! Advanced financial metrics calculation module for sophisticated trading analytics
//!
//! This module provides functions to calculate advanced trading performance
//! metrics such as profit factor, expectancy, Sharpe ratio, and other
//! sophisticated financial metrics using precise decimal arithmetic.

use model::trade::Trade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Error types for advanced metrics calculations
#[derive(Debug, PartialEq)]
pub enum AdvancedMetricsError {
    /// Insufficient data for calculation (need minimum number of trades)
    InsufficientData,
    /// Division by zero in calculation
    DivisionByZero,
    /// Arithmetic overflow in calculation
    ArithmeticOverflow,
}

/// Advanced financial metrics calculator
#[derive(Debug)]
pub struct AdvancedMetricsCalculator;

impl AdvancedMetricsCalculator {
    /// Calculate profit factor: Gross Profit / Gross Loss
    ///
    /// Profit factor measures the relationship between winning and losing trades.
    /// A profit factor > 1.0 indicates profitability.
    ///
    /// # Arguments
    /// * `closed_trades` - Vector of closed trades to analyze
    ///
    /// # Returns
    /// * `Some(Decimal)` - The profit factor if calculable
    /// * `None` - If no losing trades exist (infinite profit factor)
    pub fn calculate_profit_factor(closed_trades: &[Trade]) -> Option<Decimal> {
        if closed_trades.is_empty() {
            return None;
        }

        let gross_profit = closed_trades
            .iter()
            .filter(|trade| trade.balance.total_performance > dec!(0))
            .map(|trade| trade.balance.total_performance)
            .sum::<Decimal>();

        let gross_loss = closed_trades
            .iter()
            .filter(|trade| trade.balance.total_performance < dec!(0))
            .map(|trade| trade.balance.total_performance.abs())
            .sum::<Decimal>();

        if gross_loss == dec!(0) {
            // No losses means infinite profit factor
            return None;
        }

        // Profit Factor = Gross Profit / Gross Loss
        gross_profit.checked_div(gross_loss)
    }

    /// Calculate expectancy: Average profit per trade
    ///
    /// Expectancy = (Win Rate * Average Win) - (Loss Rate * Average Loss)
    ///
    /// # Arguments
    /// * `closed_trades` - Vector of closed trades to analyze
    ///
    /// # Returns
    /// * `Decimal` - The expected profit per trade
    pub fn calculate_expectancy(closed_trades: &[Trade]) -> Decimal {
        if closed_trades.is_empty() {
            return dec!(0);
        }

        let wins: Vec<Decimal> = closed_trades
            .iter()
            .filter(|trade| trade.balance.total_performance > dec!(0))
            .map(|trade| trade.balance.total_performance)
            .collect();

        let losses: Vec<Decimal> = closed_trades
            .iter()
            .filter(|trade| trade.balance.total_performance < dec!(0))
            .map(|trade| trade.balance.total_performance.abs())
            .collect();

        let total_trades = Decimal::from(closed_trades.len());
        let win_rate = Decimal::from(wins.len())
            .checked_div(total_trades)
            .unwrap_or(dec!(0));
        let loss_rate = dec!(1) - win_rate;

        let avg_win = if wins.is_empty() {
            dec!(0)
        } else {
            wins.iter()
                .sum::<Decimal>()
                .checked_div(Decimal::from(wins.len()))
                .unwrap_or(dec!(0))
        };

        let avg_loss = if losses.is_empty() {
            dec!(0)
        } else {
            losses
                .iter()
                .sum::<Decimal>()
                .checked_div(Decimal::from(losses.len()))
                .unwrap_or(dec!(0))
        };

        // Expectancy = (Win Rate * Average Win) - (Loss Rate * Average Loss)
        let positive_component = win_rate.checked_mul(avg_win).unwrap_or(dec!(0));
        let negative_component = loss_rate.checked_mul(avg_loss).unwrap_or(dec!(0));

        positive_component
            .checked_sub(negative_component)
            .unwrap_or(dec!(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::trade::{Status, Trade};
    use model::TradeCategory;

    fn create_test_trade(performance: Decimal) -> Trade {
        let mut trade = Trade::default();
        trade.balance.total_performance = performance;
        trade.status = Status::ClosedTarget;
        trade.category = TradeCategory::Long;
        trade
    }

    #[test]
    fn test_calculate_profit_factor_empty_trades() {
        let trades = vec![];
        let result = AdvancedMetricsCalculator::calculate_profit_factor(&trades);
        assert_eq!(result, None);
    }

    #[test]
    fn test_calculate_profit_factor_no_losses() {
        let trades = vec![create_test_trade(dec!(100)), create_test_trade(dec!(200))];
        let result = AdvancedMetricsCalculator::calculate_profit_factor(&trades);
        // Should return None for infinite profit factor (no losses)
        assert_eq!(result, None);
    }

    #[test]
    fn test_calculate_profit_factor_mixed_trades() {
        let trades = vec![
            create_test_trade(dec!(100)), // Win: +$100
            create_test_trade(dec!(200)), // Win: +$200
            create_test_trade(dec!(-50)), // Loss: -$50
            create_test_trade(dec!(-75)), // Loss: -$75
        ];
        let result = AdvancedMetricsCalculator::calculate_profit_factor(&trades);
        // Gross Profit = 100 + 200 = 300
        // Gross Loss = 50 + 75 = 125
        // Profit Factor = 300 / 125 = 2.4
        assert_eq!(result, Some(dec!(2.4)));
    }

    #[test]
    fn test_calculate_profit_factor_only_losses() {
        let trades = vec![create_test_trade(dec!(-100)), create_test_trade(dec!(-50))];
        let result = AdvancedMetricsCalculator::calculate_profit_factor(&trades);
        // No profits, so profit factor should be 0
        assert_eq!(result, Some(dec!(0)));
    }

    #[test]
    fn test_calculate_expectancy_empty_trades() {
        let trades = vec![];
        let result = AdvancedMetricsCalculator::calculate_expectancy(&trades);
        assert_eq!(result, dec!(0));
    }

    #[test]
    fn test_calculate_expectancy_positive() {
        let trades = vec![
            create_test_trade(dec!(100)), // Win
            create_test_trade(dec!(200)), // Win
            create_test_trade(dec!(-50)), // Loss
        ];
        let result = AdvancedMetricsCalculator::calculate_expectancy(&trades);
        // Win Rate = 2/3 = 0.6667
        // Average Win = (100 + 200) / 2 = 150
        // Average Loss = 50 / 1 = 50
        // Expectancy = (0.6667 * 150) - (0.3333 * 50) = 100 - 16.67 = 83.33
        let expected = dec!(83.33);
        assert!((result - expected).abs() < dec!(0.1));
    }

    #[test]
    fn test_calculate_expectancy_negative() {
        let trades = vec![
            create_test_trade(dec!(50)),   // Win
            create_test_trade(dec!(-100)), // Loss
            create_test_trade(dec!(-200)), // Loss
        ];
        let result = AdvancedMetricsCalculator::calculate_expectancy(&trades);
        // Win Rate = 1/3 = 0.3333
        // Average Win = 50
        // Average Loss = (100 + 200) / 2 = 150
        // Expectancy = (0.3333 * 50) - (0.6667 * 150) = 16.67 - 100 = -83.33
        let expected = dec!(-83.33);
        assert!((result - expected).abs() < dec!(0.1));
    }
}
