//! Advanced financial metrics calculation module for sophisticated trading analytics
//!
//! This module provides functions to calculate advanced trading performance
//! metrics such as profit factor, expectancy, Sharpe ratio, and other
//! sophisticated financial metrics using precise decimal arithmetic.

use model::trade::Trade;
use rust_decimal::prelude::*;
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
        let loss_rate = dec!(1).checked_sub(win_rate).unwrap_or(dec!(0));

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

    /// Calculate win rate: Percentage of profitable trades
    ///
    /// # Arguments
    /// * `closed_trades` - Vector of closed trades to analyze
    ///
    /// # Returns
    /// * `Decimal` - Win rate as percentage (0.0 to 100.0)
    pub fn calculate_win_rate(closed_trades: &[Trade]) -> Decimal {
        if closed_trades.is_empty() {
            return dec!(0);
        }

        let winning_trades = closed_trades
            .iter()
            .filter(|trade| trade.balance.total_performance > dec!(0))
            .count();

        let total_trades = closed_trades.len();
        let win_rate = Decimal::from(winning_trades)
            .checked_div(Decimal::from(total_trades))
            .unwrap_or(dec!(0));

        // Convert to percentage
        win_rate.checked_mul(dec!(100)).unwrap_or(dec!(0))
    }

    /// Calculate average R-multiple: Average risk-adjusted return per trade
    ///
    /// R-multiple measures how many units of risk were gained or lost per trade.
    /// Assumes risk per trade is calculated as |entry_price - stop_price| * quantity
    ///
    /// # Arguments
    /// * `closed_trades` - Vector of closed trades to analyze
    ///
    /// # Returns
    /// * `Decimal` - Average R-multiple across all trades
    pub fn calculate_average_r_multiple(closed_trades: &[Trade]) -> Decimal {
        if closed_trades.is_empty() {
            return dec!(0);
        }

        let mut total_r = dec!(0);
        let mut valid_trades: u32 = 0;

        for trade in closed_trades {
            // Calculate risk per trade based on entry-stop difference
            let entry_price = trade.entry.unit_price;
            let stop_price = trade.safety_stop.unit_price;

            let risk_per_share = entry_price.checked_sub(stop_price).unwrap_or(dec!(0)).abs();

            if risk_per_share > dec!(0) {
                let total_risk = risk_per_share
                    .checked_mul(Decimal::from(trade.entry.quantity))
                    .unwrap_or(dec!(0));

                if total_risk > dec!(0) {
                    let r_multiple = trade
                        .balance
                        .total_performance
                        .checked_div(total_risk)
                        .unwrap_or(dec!(0));

                    total_r = total_r.checked_add(r_multiple).unwrap_or(total_r);
                    valid_trades = valid_trades.checked_add(1u32).unwrap_or(valid_trades);
                }
            }
        }

        if valid_trades > 0 {
            total_r
                .checked_div(Decimal::from(valid_trades))
                .unwrap_or(dec!(0))
        } else {
            dec!(0)
        }
    }

    /// Calculate Sharpe ratio: Risk-adjusted return measure
    ///
    /// Sharpe Ratio = (Average Return - Risk-Free Rate) / Standard Deviation of Returns
    ///
    /// # Arguments
    /// * `closed_trades` - Vector of closed trades to analyze
    /// * `risk_free_rate` - Annual risk-free rate (e.g., 0.05 for 5%)
    ///
    /// # Returns
    /// * `Option<Decimal>` - Sharpe ratio if calculable (None if insufficient data)
    pub fn calculate_sharpe_ratio(
        closed_trades: &[Trade],
        risk_free_rate: Decimal,
    ) -> Option<Decimal> {
        if closed_trades.len() < 2 {
            return None;
        }

        // Extract returns (performance percentages)
        let returns: Vec<Decimal> = closed_trades
            .iter()
            .filter_map(|trade| {
                // Calculate return as percentage of capital risked
                let entry_price = trade.entry.unit_price;
                let stop_price = trade.safety_stop.unit_price;

                let risk_per_share = entry_price.checked_sub(stop_price).unwrap_or(dec!(0)).abs();
                let total_risk = risk_per_share
                    .checked_mul(Decimal::from(trade.entry.quantity))
                    .unwrap_or(dec!(0));

                if total_risk > dec!(0) {
                    trade
                        .balance
                        .total_performance
                        .checked_div(total_risk)
                        .map(|r| r.checked_mul(dec!(100)).unwrap_or(dec!(0)))
                } else {
                    None
                }
            })
            .collect();

        if returns.len() < 2 {
            return None;
        }

        // Calculate average return
        let avg_return = returns
            .iter()
            .sum::<Decimal>()
            .checked_div(Decimal::from(returns.len()))
            .unwrap_or(dec!(0));

        // Calculate standard deviation
        let variance = returns
            .iter()
            .map(|&r| {
                let diff = r.checked_sub(avg_return).unwrap_or(dec!(0));
                diff.checked_mul(diff).unwrap_or(dec!(0))
            })
            .sum::<Decimal>()
            .checked_div(Decimal::from(returns.len()))
            .unwrap_or(dec!(0));

        // Approximate square root using Newton's method for Decimal
        let std_dev = Self::decimal_sqrt(variance)?;

        if std_dev > dec!(0) {
            // Convert risk-free rate to per-trade basis (assuming daily trading)
            let risk_free_per_trade = risk_free_rate.checked_div(dec!(252)).unwrap_or(dec!(0));
            let excess_return = avg_return
                .checked_sub(risk_free_per_trade)
                .unwrap_or(avg_return);

            excess_return.checked_div(std_dev)
        } else {
            None
        }
    }

    /// Calculate Sortino ratio: Downside risk-adjusted return measure
    ///
    /// Similar to Sharpe ratio but only considers downside volatility
    ///
    /// # Arguments
    /// * `closed_trades` - Vector of closed trades to analyze
    /// * `risk_free_rate` - Annual risk-free rate
    ///
    /// # Returns
    /// * `Option<Decimal>` - Sortino ratio if calculable
    pub fn calculate_sortino_ratio(
        closed_trades: &[Trade],
        risk_free_rate: Decimal,
    ) -> Option<Decimal> {
        if closed_trades.len() < 2 {
            return None;
        }

        // Extract returns
        let returns: Vec<Decimal> = closed_trades
            .iter()
            .filter_map(|trade| {
                let entry_price = trade.entry.unit_price;
                let stop_price = trade.safety_stop.unit_price;

                let risk_per_share = entry_price.checked_sub(stop_price).unwrap_or(dec!(0)).abs();
                let total_risk = risk_per_share
                    .checked_mul(Decimal::from(trade.entry.quantity))
                    .unwrap_or(dec!(0));

                if total_risk > dec!(0) {
                    trade
                        .balance
                        .total_performance
                        .checked_div(total_risk)
                        .map(|r| r.checked_mul(dec!(100)).unwrap_or(dec!(0)))
                } else {
                    None
                }
            })
            .collect();

        if returns.len() < 2 {
            return None;
        }

        let avg_return = returns
            .iter()
            .sum::<Decimal>()
            .checked_div(Decimal::from(returns.len()))
            .unwrap_or(dec!(0));

        // Calculate downside deviation (only negative returns)
        let risk_free_per_trade = risk_free_rate.checked_div(dec!(252)).unwrap_or(dec!(0));
        let downside_returns: Vec<Decimal> = returns
            .iter()
            .filter(|&&r| r < risk_free_per_trade)
            .map(|&r| {
                let diff = r.checked_sub(risk_free_per_trade).unwrap_or(dec!(0));
                diff.checked_mul(diff).unwrap_or(dec!(0))
            })
            .collect();

        if downside_returns.is_empty() {
            return None; // No downside risk
        }

        let downside_variance = downside_returns
            .iter()
            .sum::<Decimal>()
            .checked_div(Decimal::from(downside_returns.len()))
            .unwrap_or(dec!(0));

        let downside_deviation = Self::decimal_sqrt(downside_variance)?;

        if downside_deviation > dec!(0) {
            let excess_return = avg_return
                .checked_sub(risk_free_per_trade)
                .unwrap_or(avg_return);
            excess_return.checked_div(downside_deviation)
        } else {
            None
        }
    }

    /// Calculate Calmar ratio: Return to maximum drawdown ratio
    ///
    /// Calmar Ratio = Annualized Return / Maximum Drawdown
    ///
    /// # Arguments
    /// * `closed_trades` - Vector of closed trades to analyze
    ///
    /// # Returns
    /// * `Option<Decimal>` - Calmar ratio if calculable
    pub fn calculate_calmar_ratio(closed_trades: &[Trade]) -> Option<Decimal> {
        if closed_trades.is_empty() {
            return None;
        }

        // Calculate total return
        let total_return = closed_trades
            .iter()
            .map(|trade| trade.balance.total_performance)
            .sum::<Decimal>();

        // Calculate maximum drawdown by tracking running total
        let mut running_total = dec!(0);
        let mut peak = dec!(0);
        let mut max_drawdown = dec!(0);

        for trade in closed_trades {
            running_total = running_total
                .checked_add(trade.balance.total_performance)
                .unwrap_or(running_total);

            if running_total > peak {
                peak = running_total;
            }

            let current_drawdown = peak.checked_sub(running_total).unwrap_or(dec!(0));
            if current_drawdown > max_drawdown {
                max_drawdown = current_drawdown;
            }
        }

        if max_drawdown > dec!(0) {
            // Annualize the return (assume trades span one year for simplicity)
            total_return.checked_div(max_drawdown)
        } else {
            None // No drawdown occurred
        }
    }

    /// Helper function to calculate square root of a Decimal using Newton's method
    fn decimal_sqrt(value: Decimal) -> Option<Decimal> {
        if value < dec!(0) {
            return None;
        }

        if value == dec!(0) {
            return Some(dec!(0));
        }

        let mut x = value;

        // Newton's method: x_{n+1} = (x_n + value/x_n) / 2
        for _ in 0..50 {
            // Max 50 iterations
            let prev_x = x;
            if let Some(div) = value.checked_div(x) {
                if let Some(sum) = x.checked_add(div) {
                    if let Some(next_x) = sum.checked_div(dec!(2)) {
                        x = next_x;

                        // Check for convergence
                        let diff = if x > prev_x {
                            x.checked_sub(prev_x).unwrap_or(dec!(0))
                        } else {
                            prev_x.checked_sub(x).unwrap_or(dec!(0))
                        };
                        if diff < dec!(0.0000001) {
                            return Some(x);
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Some(x)
    }

    /// Calculate Value at Risk (VaR): Potential loss at a given confidence level
    ///
    /// VaR represents the maximum potential loss over a specific time period
    /// at a given confidence level (e.g., 95% confidence means 5% chance of exceeding this loss)
    ///
    /// # Arguments
    /// * `closed_trades` - Vector of closed trades to analyze
    /// * `confidence_level` - Confidence level (e.g., 0.95 for 95%)
    ///
    /// # Returns
    /// * `Option<Decimal>` - VaR value if calculable (negative indicates loss)
    pub fn calculate_value_at_risk(
        closed_trades: &[Trade],
        confidence_level: Decimal,
    ) -> Option<Decimal> {
        if closed_trades.is_empty() {
            return None;
        }

        // Extract returns as percentages
        let mut returns: Vec<Decimal> = closed_trades
            .iter()
            .filter_map(|trade| {
                let entry_price = trade.entry.unit_price;
                let stop_price = trade.safety_stop.unit_price;

                let risk_per_share = entry_price.checked_sub(stop_price).unwrap_or(dec!(0)).abs();
                let total_risk = risk_per_share
                    .checked_mul(Decimal::from(trade.entry.quantity))
                    .unwrap_or(dec!(0));

                if total_risk > dec!(0) {
                    trade
                        .balance
                        .total_performance
                        .checked_div(total_risk)
                        .map(|r| r.checked_mul(dec!(100)).unwrap_or(dec!(0)))
                } else {
                    None
                }
            })
            .collect();

        if returns.len() < 2 {
            return None;
        }

        // Sort returns in ascending order (worst to best)
        returns.sort();

        // Calculate VaR using historical simulation method
        let percentile_index = (dec!(1) - confidence_level)
            .checked_mul(Decimal::from(returns.len()))
            .and_then(|idx| idx.to_usize())
            .unwrap_or(0);

        if percentile_index < returns.len() {
            Some(returns[percentile_index])
        } else {
            returns.first().copied()
        }
    }

    /// Calculate Kelly Criterion: Optimal position sizing formula
    ///
    /// Kelly Criterion = (bp - q) / b
    /// Where: b = odds (average win / average loss), p = win probability, q = loss probability
    ///
    /// # Arguments
    /// * `closed_trades` - Vector of closed trades to analyze
    ///
    /// # Returns
    /// * `Option<Decimal>` - Kelly percentage (0-1 range, where 0.25 = 25%)
    pub fn calculate_kelly_criterion(closed_trades: &[Trade]) -> Option<Decimal> {
        if closed_trades.is_empty() {
            return None;
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

        if wins.is_empty() || losses.is_empty() {
            return None; // Need both wins and losses for Kelly calculation
        }

        let avg_win = wins
            .iter()
            .sum::<Decimal>()
            .checked_div(Decimal::from(wins.len()))
            .unwrap_or(dec!(0));

        let avg_loss = losses
            .iter()
            .sum::<Decimal>()
            .checked_div(Decimal::from(losses.len()))
            .unwrap_or(dec!(0));

        if avg_loss == dec!(0) {
            return None; // Avoid division by zero
        }

        let win_prob = Decimal::from(wins.len())
            .checked_div(Decimal::from(closed_trades.len()))
            .unwrap_or(dec!(0));

        let loss_prob = dec!(1).checked_sub(win_prob).unwrap_or(dec!(0));

        // b = average win / average loss (odds)
        let odds = avg_win.checked_div(avg_loss).unwrap_or(dec!(0));

        // Kelly = (bp - q) / b = p - q/b
        let kelly = win_prob
            .checked_sub(loss_prob.checked_div(odds).unwrap_or(dec!(0)))
            .unwrap_or(dec!(0));

        Some(kelly.max(dec!(0))) // Ensure non-negative result
    }

    /// Calculate maximum consecutive losses for risk assessment
    ///
    /// # Arguments
    /// * `closed_trades` - Vector of closed trades to analyze (should be chronologically ordered)
    ///
    /// # Returns
    /// * `u32` - Maximum number of consecutive losing trades
    pub fn calculate_max_consecutive_losses(closed_trades: &[Trade]) -> u32 {
        if closed_trades.is_empty() {
            return 0;
        }

        let mut max_consecutive = 0;
        let mut current_consecutive = 0;

        for trade in closed_trades {
            if trade.balance.total_performance < dec!(0) {
                current_consecutive += 1;
                max_consecutive = max_consecutive.max(current_consecutive);
            } else {
                current_consecutive = 0;
            }
        }

        max_consecutive
    }

    /// Calculate maximum consecutive wins for system robustness analysis
    ///
    /// # Arguments
    /// * `closed_trades` - Vector of closed trades to analyze (should be chronologically ordered)
    ///
    /// # Returns
    /// * `u32` - Maximum number of consecutive winning trades
    pub fn calculate_max_consecutive_wins(closed_trades: &[Trade]) -> u32 {
        if closed_trades.is_empty() {
            return 0;
        }

        let mut max_consecutive = 0;
        let mut current_consecutive = 0;

        for trade in closed_trades {
            if trade.balance.total_performance > dec!(0) {
                current_consecutive += 1;
                max_consecutive = max_consecutive.max(current_consecutive);
            } else {
                current_consecutive = 0;
            }
        }

        max_consecutive
    }

    /// Calculate Ulcer Index: Downside volatility measure based on drawdowns
    ///
    /// Ulcer Index measures the depth and duration of drawdowns
    ///
    /// # Arguments
    /// * `closed_trades` - Vector of closed trades to analyze
    ///
    /// # Returns
    /// * `Option<Decimal>` - Ulcer Index as percentage
    pub fn calculate_ulcer_index(closed_trades: &[Trade]) -> Option<Decimal> {
        if closed_trades.len() < 2 {
            return None;
        }

        let mut running_total = dec!(0);
        let mut peak = dec!(0);
        let mut squared_drawdowns = Vec::new();

        for trade in closed_trades {
            running_total = running_total
                .checked_add(trade.balance.total_performance)
                .unwrap_or(running_total);

            if running_total > peak {
                peak = running_total;
            }

            let drawdown_percent = if peak > dec!(0) {
                peak.checked_sub(running_total)
                    .and_then(|dd| dd.checked_div(peak))
                    .map(|pct| pct.checked_mul(dec!(100)).unwrap_or(dec!(0)))
                    .unwrap_or(dec!(0))
            } else {
                dec!(0)
            };

            let squared_dd = drawdown_percent
                .checked_mul(drawdown_percent)
                .unwrap_or(dec!(0));
            squared_drawdowns.push(squared_dd);
        }

        if squared_drawdowns.is_empty() {
            return None;
        }

        let mean_squared_dd = squared_drawdowns
            .iter()
            .sum::<Decimal>()
            .checked_div(Decimal::from(squared_drawdowns.len()))
            .unwrap_or(dec!(0));

        Self::decimal_sqrt(mean_squared_dd)
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

    #[test]
    fn test_calculate_win_rate_empty_trades() {
        let trades = vec![];
        let result = AdvancedMetricsCalculator::calculate_win_rate(&trades);
        assert_eq!(result, dec!(0));
    }

    #[test]
    fn test_calculate_win_rate_all_wins() {
        let trades = vec![
            create_test_trade(dec!(100)),
            create_test_trade(dec!(200)),
            create_test_trade(dec!(50)),
        ];
        let result = AdvancedMetricsCalculator::calculate_win_rate(&trades);
        assert_eq!(result, dec!(100));
    }

    #[test]
    fn test_calculate_win_rate_mixed() {
        let trades = vec![
            create_test_trade(dec!(100)), // Win
            create_test_trade(dec!(200)), // Win
            create_test_trade(dec!(-50)), // Loss
        ];
        let result = AdvancedMetricsCalculator::calculate_win_rate(&trades);
        // 2 wins out of 3 trades = 66.67%
        let expected = dec!(66.666666666666666666666666667);
        assert!((result - expected).abs() < dec!(0.1));
    }

    #[test]
    fn test_calculate_average_r_multiple_empty_trades() {
        let trades = vec![];
        let result = AdvancedMetricsCalculator::calculate_average_r_multiple(&trades);
        assert_eq!(result, dec!(0));
    }

    #[test]
    fn test_calculate_sharpe_ratio_insufficient_data() {
        let trades = vec![create_test_trade(dec!(100))];
        let result = AdvancedMetricsCalculator::calculate_sharpe_ratio(&trades, dec!(0.05));
        assert_eq!(result, None);
    }

    #[test]
    fn test_calculate_sortino_ratio_insufficient_data() {
        let trades = vec![create_test_trade(dec!(100))];
        let result = AdvancedMetricsCalculator::calculate_sortino_ratio(&trades, dec!(0.05));
        assert_eq!(result, None);
    }

    #[test]
    fn test_calculate_calmar_ratio_empty_trades() {
        let trades = vec![];
        let result = AdvancedMetricsCalculator::calculate_calmar_ratio(&trades);
        assert_eq!(result, None);
    }

    #[test]
    fn test_calculate_calmar_ratio_no_drawdown() {
        let trades = vec![
            create_test_trade(dec!(100)),
            create_test_trade(dec!(200)),
            create_test_trade(dec!(50)),
        ];
        let result = AdvancedMetricsCalculator::calculate_calmar_ratio(&trades);
        // No drawdown (all positive returns), so should return None
        assert_eq!(result, None);
    }

    #[test]
    fn test_calculate_calmar_ratio_with_drawdown() {
        let trades = vec![
            create_test_trade(dec!(100)), // Running: 100, Peak: 100
            create_test_trade(dec!(-50)), // Running: 50, Drawdown: 50
            create_test_trade(dec!(200)), // Running: 250, Peak: 250
        ];
        let result = AdvancedMetricsCalculator::calculate_calmar_ratio(&trades);

        // Total return: 100 - 50 + 200 = 250
        // Max drawdown: 50
        // Calmar ratio: 250 / 50 = 5.0
        assert_eq!(result, Some(dec!(5.0)));
    }

    #[test]
    fn test_decimal_sqrt_zero() {
        let result = AdvancedMetricsCalculator::decimal_sqrt(dec!(0));
        assert_eq!(result, Some(dec!(0)));
    }

    #[test]
    fn test_decimal_sqrt_positive() {
        let result = AdvancedMetricsCalculator::decimal_sqrt(dec!(4));
        assert!(result.is_some());
        let sqrt_result = result.unwrap();
        // sqrt(4) should be close to 2
        assert!((sqrt_result - dec!(2)).abs() < dec!(0.0001));
    }

    #[test]
    fn test_decimal_sqrt_negative() {
        let result = AdvancedMetricsCalculator::decimal_sqrt(dec!(-1));
        assert_eq!(result, None);
    }

    #[test]
    fn test_decimal_sqrt_precision() {
        let result = AdvancedMetricsCalculator::decimal_sqrt(dec!(2));
        assert!(result.is_some());
        let sqrt_result = result.unwrap();
        // sqrt(2) should be close to 1.414213562373095
        let expected = dec!(1.414213562373095);
        assert!((sqrt_result - expected).abs() < dec!(0.01));
    }
}
