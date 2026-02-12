//! Advanced financial metrics calculation module for sophisticated trading analytics
//!
//! This module provides functions to calculate advanced trading performance
//! metrics such as profit factor, expectancy, Sharpe ratio, and other
//! sophisticated financial metrics using precise decimal arithmetic.

use model::trade::Trade;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

/// Rolling metrics point for a given window.
#[derive(Debug, Clone, PartialEq)]
pub struct RollingMetricsPoint {
    /// Window size in days.
    pub window_days: u32,
    /// Number of trades included in this window.
    pub trade_count: usize,
    /// Window Sharpe ratio.
    pub sharpe_ratio: Option<Decimal>,
    /// Window Sortino ratio.
    pub sortino_ratio: Option<Decimal>,
    /// Window Calmar ratio.
    pub calmar_ratio: Option<Decimal>,
    /// Window expectancy.
    pub expectancy: Decimal,
    /// Window maximum drawdown on cumulative trade PnL.
    pub max_drawdown: Decimal,
}

/// Execution quality metrics (best-effort from available trade/order data).
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionQualityMetrics {
    /// Share of trades with available entry average fill price.
    pub fill_price_coverage_percentage: Decimal,
    /// Average signed entry slippage in bps.
    pub average_entry_slippage_bps: Option<Decimal>,
    /// Average absolute entry slippage in bps.
    pub average_abs_entry_slippage_bps: Option<Decimal>,
    /// Median signed entry slippage in bps.
    pub median_entry_slippage_bps: Option<Decimal>,
    /// Median absolute entry slippage in bps.
    pub median_abs_entry_slippage_bps: Option<Decimal>,
    /// 95th percentile absolute entry slippage in bps.
    pub p95_abs_entry_slippage_bps: Option<Decimal>,

    /// Share of trades with available stop average fill price (when stop is filled).
    pub stop_fill_price_coverage_percentage: Decimal,
    /// Average signed stop slippage in bps.
    pub average_stop_slippage_bps: Option<Decimal>,
    /// Average absolute stop slippage in bps.
    pub average_abs_stop_slippage_bps: Option<Decimal>,

    /// Share of trades with available target average fill price (when target is filled).
    pub target_fill_price_coverage_percentage: Decimal,
    /// Average signed target slippage in bps.
    pub average_target_slippage_bps: Option<Decimal>,
    /// Average absolute target slippage in bps.
    pub average_abs_target_slippage_bps: Option<Decimal>,

    /// Average setup reward/risk ratio from target/stop placement.
    pub average_setup_reward_to_risk: Option<Decimal>,
    /// Average holding period in days.
    pub average_holding_days: Decimal,
    /// Profit generated per holding day.
    pub profit_per_holding_day: Option<Decimal>,
}

/// Journal/record-keeping completeness metrics for trades.
#[derive(Debug, Clone, PartialEq)]
pub struct JournalQualityMetrics {
    /// Percent of trades with thesis populated.
    pub thesis_coverage_percentage: Decimal,
    /// Percent of trades with sector populated.
    pub sector_coverage_percentage: Decimal,
    /// Percent of trades with asset_class populated.
    pub asset_class_coverage_percentage: Decimal,
    /// Percent of trades with context populated.
    pub context_coverage_percentage: Decimal,
    /// Percent of trades with all key fields populated (thesis, sector, asset_class, context).
    pub complete_journal_percentage: Decimal,
}

/// Win/loss streak metrics derived from closed trades ordered by close/update time.
#[derive(Debug, Clone, PartialEq)]
pub struct StreakMetrics {
    pub max_consecutive_wins: u32,
    pub max_consecutive_losses: u32,
    pub average_win_streak: Option<Decimal>,
    pub average_loss_streak: Option<Decimal>,
    pub current_streak_type: Option<&'static str>,
    pub current_streak_len: u32,
}

/// Exposure metrics derived from open positions.
#[derive(Debug, Clone, PartialEq)]
pub struct ExposureMetrics {
    /// Gross exposure (long + short absolute exposure).
    pub gross_exposure: Decimal,
    /// Net exposure (long - short).
    pub net_exposure: Decimal,
    /// Long-side exposure.
    pub long_exposure: Decimal,
    /// Short-side exposure.
    pub short_exposure: Decimal,
    /// Top-3 symbol concentration as percentage of gross exposure.
    pub top_3_symbol_concentration_percentage: Decimal,
    /// Largest sector concentration as percentage of gross exposure.
    pub top_sector_concentration_percentage: Decimal,
}

/// Bootstrap confidence intervals for key metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct BootstrapConfidenceIntervals {
    /// Expectancy confidence interval (2.5%, 97.5%).
    pub expectancy_95: Option<(Decimal, Decimal)>,
    /// Sharpe ratio confidence interval (2.5%, 97.5%).
    pub sharpe_95: Option<(Decimal, Decimal)>,
}

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
    /// Gross profit across closed trades (sum of positive PnL).
    pub fn calculate_gross_profit(closed_trades: &[Trade]) -> Decimal {
        closed_trades
            .iter()
            .filter(|trade| trade.balance.total_performance > dec!(0))
            .map(|trade| trade.balance.total_performance)
            .sum::<Decimal>()
    }

    /// Gross loss across closed trades (sum of absolute losses).
    pub fn calculate_gross_loss(closed_trades: &[Trade]) -> Decimal {
        closed_trades
            .iter()
            .filter(|trade| trade.balance.total_performance < dec!(0))
            .map(|trade| trade.balance.total_performance.abs())
            .sum::<Decimal>()
    }

    /// Net profit across closed trades (sum of PnL).
    pub fn calculate_net_profit(closed_trades: &[Trade]) -> Decimal {
        closed_trades
            .iter()
            .map(|trade| trade.balance.total_performance)
            .sum::<Decimal>()
    }

    /// Payoff ratio = average win / average loss (absolute). None if no losses.
    pub fn calculate_payoff_ratio(closed_trades: &[Trade]) -> Option<Decimal> {
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
            return None;
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
            return None;
        }

        avg_win.checked_div(avg_loss)
    }

    /// Average trade PnL (mean of `total_performance`).
    pub fn calculate_average_trade_pnl(closed_trades: &[Trade]) -> Decimal {
        if closed_trades.is_empty() {
            return dec!(0);
        }

        let sum = Self::calculate_net_profit(closed_trades);
        sum.checked_div(Decimal::from(closed_trades.len()))
            .unwrap_or(dec!(0))
    }

    /// Median trade PnL (median of `total_performance`).
    pub fn calculate_median_trade_pnl(closed_trades: &[Trade]) -> Option<Decimal> {
        if closed_trades.is_empty() {
            return None;
        }

        let mut xs: Vec<Decimal> = closed_trades
            .iter()
            .map(|trade| trade.balance.total_performance)
            .collect();
        xs.sort();

        let n = xs.len();
        let mid = n / 2;
        if n % 2 == 1 {
            Some(xs[mid])
        } else {
            let a = xs[mid.saturating_sub(1)];
            let b = xs[mid];
            a.checked_add(b)
                .and_then(|s| s.checked_div(dec!(2)))
                .or(Some(a))
        }
    }

    fn extract_r_multiple_returns(closed_trades: &[Trade]) -> Vec<Decimal> {
        closed_trades
            .iter()
            .filter_map(|trade| {
                let entry_price = trade.entry.unit_price;
                let stop_price = trade.safety_stop.unit_price;
                let risk_per_share = entry_price.checked_sub(stop_price).unwrap_or(dec!(0)).abs();
                let total_risk = risk_per_share
                    .checked_mul(Decimal::from(trade.entry.quantity))
                    .unwrap_or(dec!(0));

                if total_risk > dec!(0) {
                    trade.balance.total_performance.checked_div(total_risk)
                } else {
                    None
                }
            })
            .collect()
    }

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

        let gross_profit = Self::calculate_gross_profit(closed_trades);
        let gross_loss = Self::calculate_gross_loss(closed_trades);

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
        let returns = Self::extract_r_multiple_returns(closed_trades);
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
        let returns = Self::extract_r_multiple_returns(closed_trades);
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
        let mut returns = Self::extract_r_multiple_returns(closed_trades);
        if returns.len() < 2 {
            return None;
        }

        // Sort returns in ascending order (worst to best)
        returns.sort();

        // Calculate VaR using historical simulation method
        let percentile_index = dec!(1)
            .checked_sub(confidence_level)
            .unwrap_or(dec!(0))
            .checked_mul(Decimal::from(returns.len()))
            .and_then(|idx| idx.to_usize())
            .unwrap_or(0);

        if percentile_index < returns.len() {
            returns.get(percentile_index).copied()
        } else {
            returns.first().copied()
        }
    }

    /// Calculate expected shortfall (CVaR): mean loss beyond VaR threshold.
    pub fn calculate_expected_shortfall(
        closed_trades: &[Trade],
        confidence_level: Decimal,
    ) -> Option<Decimal> {
        let mut returns = Self::extract_r_multiple_returns(closed_trades);
        if returns.len() < 2 {
            return None;
        }
        returns.sort();

        let tail_count = dec!(1)
            .checked_sub(confidence_level)
            .unwrap_or(dec!(0))
            .checked_mul(Decimal::from(returns.len()))
            .and_then(|v| v.to_u32())
            .unwrap_or(0)
            .max(1);

        let tail_values: Vec<Decimal> = returns.into_iter().take(tail_count as usize).collect();
        if tail_values.is_empty() {
            return None;
        }
        tail_values
            .iter()
            .copied()
            .sum::<Decimal>()
            .checked_div(Decimal::from(tail_values.len()))
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

        let mut max_consecutive: u32 = 0;
        let mut current_consecutive: u32 = 0;

        for trade in closed_trades {
            if trade.balance.total_performance < dec!(0) {
                current_consecutive = current_consecutive
                    .checked_add(1u32)
                    .unwrap_or(current_consecutive);
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

        let mut max_consecutive: u32 = 0;
        let mut current_consecutive: u32 = 0;

        for trade in closed_trades {
            if trade.balance.total_performance > dec!(0) {
                current_consecutive = current_consecutive
                    .checked_add(1u32)
                    .unwrap_or(current_consecutive);
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

    /// Calculate rolling metrics over multiple windows.
    pub fn calculate_rolling_metrics(
        closed_trades: &[Trade],
        windows_days: &[u32],
        risk_free_rate: Decimal,
    ) -> Vec<RollingMetricsPoint> {
        use chrono::{Duration, Utc};

        windows_days
            .iter()
            .map(|window_days| {
                let cutoff = Utc::now()
                    .naive_utc()
                    .checked_sub_signed(Duration::days(i64::from(*window_days)))
                    .unwrap_or_else(|| Utc::now().naive_utc());
                let trades: Vec<Trade> = closed_trades
                    .iter()
                    .filter(|trade| trade.updated_at >= cutoff)
                    .cloned()
                    .collect();

                RollingMetricsPoint {
                    window_days: *window_days,
                    trade_count: trades.len(),
                    sharpe_ratio: Self::calculate_sharpe_ratio(&trades, risk_free_rate),
                    sortino_ratio: Self::calculate_sortino_ratio(&trades, risk_free_rate),
                    calmar_ratio: Self::calculate_calmar_ratio(&trades),
                    expectancy: Self::calculate_expectancy(&trades),
                    max_drawdown: Self::calculate_max_drawdown_amount(&trades),
                }
            })
            .collect()
    }

    fn calculate_max_drawdown_amount(closed_trades: &[Trade]) -> Decimal {
        let mut running = dec!(0);
        let mut peak = dec!(0);
        let mut max_drawdown = dec!(0);
        for trade in closed_trades {
            running = running
                .checked_add(trade.balance.total_performance)
                .unwrap_or(running);
            if running > peak {
                peak = running;
            }
            let drawdown = peak.checked_sub(running).unwrap_or(dec!(0));
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
            }
        }
        max_drawdown
    }

    /// Calculate execution quality metrics from currently available trade/order data.
    #[allow(clippy::too_many_lines)]
    pub fn calculate_execution_quality(closed_trades: &[Trade]) -> ExecutionQualityMetrics {
        let mut signed_bps: Vec<Decimal> = Vec::new();
        let mut abs_bps: Vec<Decimal> = Vec::new();
        let mut stop_signed_bps: Vec<Decimal> = Vec::new();
        let mut stop_abs_bps: Vec<Decimal> = Vec::new();
        let mut target_signed_bps: Vec<Decimal> = Vec::new();
        let mut target_abs_bps: Vec<Decimal> = Vec::new();
        let mut setup_rr: Vec<Decimal> = Vec::new();
        let mut total_holding_days = dec!(0);
        let mut holdings_count: u32 = 0;

        for trade in closed_trades {
            if let Some(avg_fill) = trade.entry.average_filled_price {
                if trade.entry.unit_price > dec!(0) {
                    let slippage = avg_fill
                        .checked_sub(trade.entry.unit_price)
                        .unwrap_or(dec!(0));
                    let bps = slippage
                        .checked_div(trade.entry.unit_price)
                        .and_then(|v| v.checked_mul(dec!(10000)))
                        .unwrap_or(dec!(0));
                    signed_bps.push(bps);
                    abs_bps.push(bps.abs());
                }
            }

            if let Some(avg_fill) = trade.safety_stop.average_filled_price {
                if trade.safety_stop.unit_price > dec!(0) {
                    // For stop orders, a worse fill is further away from the stop level.
                    let slippage = avg_fill
                        .checked_sub(trade.safety_stop.unit_price)
                        .unwrap_or(dec!(0));
                    let bps = slippage
                        .checked_div(trade.safety_stop.unit_price)
                        .and_then(|v| v.checked_mul(dec!(10000)))
                        .unwrap_or(dec!(0));
                    stop_signed_bps.push(bps);
                    stop_abs_bps.push(bps.abs());
                }
            }

            if let Some(avg_fill) = trade.target.average_filled_price {
                if trade.target.unit_price > dec!(0) {
                    let slippage = avg_fill
                        .checked_sub(trade.target.unit_price)
                        .unwrap_or(dec!(0));
                    let bps = slippage
                        .checked_div(trade.target.unit_price)
                        .and_then(|v| v.checked_mul(dec!(10000)))
                        .unwrap_or(dec!(0));
                    target_signed_bps.push(bps);
                    target_abs_bps.push(bps.abs());
                }
            }

            let risk = trade
                .entry
                .unit_price
                .checked_sub(trade.safety_stop.unit_price)
                .unwrap_or(dec!(0))
                .abs();
            let reward = trade
                .target
                .unit_price
                .checked_sub(trade.entry.unit_price)
                .unwrap_or(dec!(0))
                .abs();
            if risk > dec!(0) {
                if let Some(rr) = reward.checked_div(risk) {
                    setup_rr.push(rr);
                }
            }

            let holding_days = trade
                .updated_at
                .signed_duration_since(trade.created_at)
                .num_days()
                .max(0);
            total_holding_days = total_holding_days
                .checked_add(Decimal::from(holding_days))
                .unwrap_or(total_holding_days);
            holdings_count = holdings_count.checked_add(1).unwrap_or(holdings_count);
        }

        let avg_signed = Self::mean_opt(&signed_bps);
        let avg_abs = Self::mean_opt(&abs_bps);
        let median_signed = Self::median_opt(&mut signed_bps.clone());
        let median_abs = Self::median_opt(&mut abs_bps.clone());
        let p95_abs = Self::percentile_opt(&mut abs_bps.clone(), dec!(0.95));
        let avg_setup_rr = Self::mean_opt(&setup_rr);
        let coverage = if closed_trades.is_empty() {
            dec!(0)
        } else {
            Decimal::from(signed_bps.len())
                .checked_mul(dec!(100))
                .and_then(|v| v.checked_div(Decimal::from(closed_trades.len())))
                .unwrap_or(dec!(0))
        };

        let stop_coverage = if closed_trades.is_empty() {
            dec!(0)
        } else {
            Decimal::from(stop_signed_bps.len())
                .checked_mul(dec!(100))
                .and_then(|v| v.checked_div(Decimal::from(closed_trades.len())))
                .unwrap_or(dec!(0))
        };
        let target_coverage = if closed_trades.is_empty() {
            dec!(0)
        } else {
            Decimal::from(target_signed_bps.len())
                .checked_mul(dec!(100))
                .and_then(|v| v.checked_div(Decimal::from(closed_trades.len())))
                .unwrap_or(dec!(0))
        };

        let avg_stop_signed = Self::mean_opt(&stop_signed_bps);
        let avg_stop_abs = Self::mean_opt(&stop_abs_bps);
        let avg_target_signed = Self::mean_opt(&target_signed_bps);
        let avg_target_abs = Self::mean_opt(&target_abs_bps);

        let avg_holding_days = if holdings_count == 0 {
            dec!(0)
        } else {
            total_holding_days
                .checked_div(Decimal::from(holdings_count))
                .unwrap_or(dec!(0))
        };
        let total_pnl = closed_trades
            .iter()
            .map(|trade| trade.balance.total_performance)
            .sum::<Decimal>();
        let profit_per_holding_day = if total_holding_days > dec!(0) {
            total_pnl.checked_div(total_holding_days)
        } else {
            None
        };

        ExecutionQualityMetrics {
            fill_price_coverage_percentage: coverage,
            average_entry_slippage_bps: avg_signed,
            average_abs_entry_slippage_bps: avg_abs,
            median_entry_slippage_bps: median_signed,
            median_abs_entry_slippage_bps: median_abs,
            p95_abs_entry_slippage_bps: p95_abs,
            stop_fill_price_coverage_percentage: stop_coverage,
            average_stop_slippage_bps: avg_stop_signed,
            average_abs_stop_slippage_bps: avg_stop_abs,
            target_fill_price_coverage_percentage: target_coverage,
            average_target_slippage_bps: avg_target_signed,
            average_abs_target_slippage_bps: avg_target_abs,
            average_setup_reward_to_risk: avg_setup_rr,
            average_holding_days: avg_holding_days,
            profit_per_holding_day,
        }
    }

    pub fn calculate_journal_quality(trades: &[Trade]) -> JournalQualityMetrics {
        if trades.is_empty() {
            return JournalQualityMetrics {
                thesis_coverage_percentage: dec!(0),
                sector_coverage_percentage: dec!(0),
                asset_class_coverage_percentage: dec!(0),
                context_coverage_percentage: dec!(0),
                complete_journal_percentage: dec!(0),
            };
        }

        let mut thesis = 0u32;
        let mut sector = 0u32;
        let mut asset_class = 0u32;
        let mut context = 0u32;
        let mut complete = 0u32;

        for trade in trades {
            let has_thesis = trade.thesis.as_ref().is_some_and(|v| !v.trim().is_empty());
            let has_sector = trade.sector.as_ref().is_some_and(|v| !v.trim().is_empty());
            let has_asset_class = trade
                .asset_class
                .as_ref()
                .is_some_and(|v| !v.trim().is_empty());
            let has_context = trade.context.as_ref().is_some_and(|v| !v.trim().is_empty());

            if has_thesis {
                thesis = thesis.saturating_add(1);
            }
            if has_sector {
                sector = sector.saturating_add(1);
            }
            if has_asset_class {
                asset_class = asset_class.saturating_add(1);
            }
            if has_context {
                context = context.saturating_add(1);
            }
            if has_thesis && has_sector && has_asset_class && has_context {
                complete = complete.saturating_add(1);
            }
        }

        let n = Decimal::from(trades.len());
        let pct = |count: u32| {
            Decimal::from(count)
                .checked_mul(dec!(100))
                .and_then(|v| v.checked_div(n))
                .unwrap_or(dec!(0))
        };

        JournalQualityMetrics {
            thesis_coverage_percentage: pct(thesis),
            sector_coverage_percentage: pct(sector),
            asset_class_coverage_percentage: pct(asset_class),
            context_coverage_percentage: pct(context),
            complete_journal_percentage: pct(complete),
        }
    }

    pub fn calculate_streak_metrics(closed_trades: &[Trade]) -> StreakMetrics {
        if closed_trades.is_empty() {
            return StreakMetrics {
                max_consecutive_wins: 0,
                max_consecutive_losses: 0,
                average_win_streak: None,
                average_loss_streak: None,
                current_streak_type: None,
                current_streak_len: 0,
            };
        }

        let mut trades: Vec<Trade> = closed_trades.to_vec();
        trades.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));

        let mut win_streaks: Vec<u32> = Vec::new();
        let mut loss_streaks: Vec<u32> = Vec::new();
        let mut current_type: Option<&'static str> = None;
        let mut current_len: u32 = 0;

        for trade in &trades {
            let outcome = if trade.balance.total_performance > dec!(0) {
                Some("win")
            } else if trade.balance.total_performance < dec!(0) {
                Some("loss")
            } else {
                None
            };

            match (current_type, outcome) {
                (None, Some(t)) => {
                    current_type = Some(t);
                    current_len = 1;
                }
                (Some(ct), Some(t)) if ct == t => {
                    current_len = current_len.saturating_add(1);
                }
                (Some(ct), Some(t)) => {
                    if ct == "win" {
                        win_streaks.push(current_len);
                    } else if ct == "loss" {
                        loss_streaks.push(current_len);
                    }
                    current_type = Some(t);
                    current_len = 1;
                }
                (_, None) => {
                    // Zero PnL breaks streak.
                    if let Some(ct) = current_type {
                        if ct == "win" {
                            win_streaks.push(current_len);
                        } else if ct == "loss" {
                            loss_streaks.push(current_len);
                        }
                    }
                    current_type = None;
                    current_len = 0;
                }
            }
        }

        // Close out last streak.
        if let Some(ct) = current_type {
            if ct == "win" {
                win_streaks.push(current_len);
            } else if ct == "loss" {
                loss_streaks.push(current_len);
            }
        }

        let max_wins = win_streaks.iter().copied().max().unwrap_or(0);
        let max_losses = loss_streaks.iter().copied().max().unwrap_or(0);
        let avg = |xs: &[u32]| {
            if xs.is_empty() {
                None
            } else {
                let sum: u32 = xs.iter().copied().sum();
                Decimal::from(sum).checked_div(Decimal::from(xs.len()))
            }
        };

        // Current streak should refer to the most recent trades in chronological order.
        let (current_streak_type, current_streak_len) = {
            let last = trades.last();
            match last {
                None => (None, 0),
                Some(last_trade) => {
                    if last_trade.balance.total_performance > dec!(0) {
                        // Recompute trailing win streak length.
                        let mut len = 0u32;
                        for t in trades.iter().rev() {
                            if t.balance.total_performance > dec!(0) {
                                len = len.saturating_add(1);
                            } else {
                                break;
                            }
                        }
                        (Some("win"), len)
                    } else if last_trade.balance.total_performance < dec!(0) {
                        let mut len = 0u32;
                        for t in trades.iter().rev() {
                            if t.balance.total_performance < dec!(0) {
                                len = len.saturating_add(1);
                            } else {
                                break;
                            }
                        }
                        (Some("loss"), len)
                    } else {
                        (None, 0)
                    }
                }
            }
        };

        StreakMetrics {
            max_consecutive_wins: max_wins,
            max_consecutive_losses: max_losses,
            average_win_streak: avg(&win_streaks),
            average_loss_streak: avg(&loss_streaks),
            current_streak_type,
            current_streak_len,
        }
    }

    fn median_opt(values: &mut [Decimal]) -> Option<Decimal> {
        if values.is_empty() {
            return None;
        }
        values.sort();
        let mid = values.len() / 2;
        if values.len() % 2 == 1 {
            values.get(mid).copied()
        } else {
            let a = values
                .get(mid.saturating_sub(1))
                .copied()
                .unwrap_or(dec!(0));
            let b = values.get(mid).copied().unwrap_or(dec!(0));
            a.checked_add(b).and_then(|v| v.checked_div(dec!(2)))
        }
    }

    fn percentile_opt(values: &mut [Decimal], percentile: Decimal) -> Option<Decimal> {
        if values.is_empty() {
            return None;
        }
        if percentile <= dec!(0) {
            values.sort();
            return values.first().copied();
        }
        if percentile >= dec!(1) {
            values.sort();
            return values.last().copied();
        }
        values.sort();
        // Nearest-rank method.
        let n = values.len();
        let rank = (Decimal::from(n) * percentile)
            .ceil()
            .to_u32()
            .unwrap_or(1)
            .max(1);
        let idx = usize::try_from(rank.saturating_sub(1))
            .unwrap_or(0)
            .min(n - 1);
        values.get(idx).copied()
    }

    /// Calculate exposure metrics from all trades using open capital in market.
    pub fn calculate_exposure_metrics(trades: &[Trade]) -> ExposureMetrics {
        use model::trade::TradeCategory;
        let mut long_exposure = dec!(0);
        let mut short_exposure = dec!(0);
        let mut symbol_exposure: HashMap<String, Decimal> = HashMap::new();
        let mut sector_exposure: HashMap<String, Decimal> = HashMap::new();

        for trade in trades {
            let exposure = trade.balance.capital_in_market.max(dec!(0));
            if exposure == dec!(0) {
                continue;
            }
            match trade.category {
                TradeCategory::Long => {
                    long_exposure = long_exposure.checked_add(exposure).unwrap_or(long_exposure);
                }
                TradeCategory::Short => {
                    short_exposure = short_exposure
                        .checked_add(exposure)
                        .unwrap_or(short_exposure);
                }
            }

            let symbol = trade.trading_vehicle.symbol.clone();
            let symbol_total = symbol_exposure.entry(symbol).or_insert(dec!(0));
            *symbol_total = symbol_total.checked_add(exposure).unwrap_or(*symbol_total);

            let sector = trade
                .sector
                .clone()
                .unwrap_or_else(|| "Unknown".to_string());
            let sector_total = sector_exposure.entry(sector).or_insert(dec!(0));
            *sector_total = sector_total.checked_add(exposure).unwrap_or(*sector_total);
        }

        let gross = long_exposure
            .checked_add(short_exposure)
            .unwrap_or(long_exposure);
        let net = long_exposure.checked_sub(short_exposure).unwrap_or(dec!(0));

        let mut symbol_values: Vec<Decimal> = symbol_exposure.into_values().collect();
        symbol_values.sort_by(|a, b| b.cmp(a));
        let top3 = symbol_values.iter().take(3).copied().sum::<Decimal>();

        let top_sector = sector_exposure.into_values().max().unwrap_or(dec!(0));

        let to_pct = |value: Decimal| {
            if gross > dec!(0) {
                value
                    .checked_mul(dec!(100))
                    .and_then(|v| v.checked_div(gross))
                    .unwrap_or(dec!(0))
            } else {
                dec!(0)
            }
        };

        ExposureMetrics {
            gross_exposure: gross,
            net_exposure: net,
            long_exposure,
            short_exposure,
            top_3_symbol_concentration_percentage: to_pct(top3),
            top_sector_concentration_percentage: to_pct(top_sector),
        }
    }

    /// Risk-of-ruin proxy based on probability of hitting a long losing streak.
    pub fn calculate_risk_of_ruin_proxy(
        closed_trades: &[Trade],
        horizon_trades: u32,
        losing_streak_len: u32,
    ) -> Option<Decimal> {
        if closed_trades.is_empty() || horizon_trades < losing_streak_len || losing_streak_len == 0
        {
            return None;
        }

        let losses = closed_trades
            .iter()
            .filter(|trade| trade.balance.total_performance < dec!(0))
            .count();
        let loss_prob = Decimal::from(losses)
            .checked_div(Decimal::from(closed_trades.len()))
            .unwrap_or(dec!(0));

        let mut streak_prob = dec!(1);
        for _ in 0..losing_streak_len {
            streak_prob = streak_prob.checked_mul(loss_prob).unwrap_or(dec!(0));
        }

        let windows = horizon_trades
            .saturating_sub(losing_streak_len)
            .saturating_add(1);
        let one_minus = dec!(1).checked_sub(streak_prob).unwrap_or(dec!(0));
        let mut avoid_all = dec!(1);
        for _ in 0..windows {
            avoid_all = avoid_all.checked_mul(one_minus).unwrap_or(dec!(0));
        }
        dec!(1).checked_sub(avoid_all)
    }

    /// Bootstrap confidence intervals with deterministic re-sampling.
    pub fn calculate_bootstrap_confidence_intervals(
        closed_trades: &[Trade],
        samples: u32,
        risk_free_rate: Decimal,
    ) -> BootstrapConfidenceIntervals {
        if closed_trades.len() < 2 || samples < 10 {
            return BootstrapConfidenceIntervals {
                expectancy_95: None,
                sharpe_95: None,
            };
        }

        let mut expectancy_samples: Vec<Decimal> = Vec::new();
        let mut sharpe_samples: Vec<Decimal> = Vec::new();
        let n = closed_trades.len();
        for i in 0..samples {
            let mut sample: Vec<Trade> = Vec::with_capacity(n);
            for j in 0..n {
                let i_usize = usize::try_from(i).unwrap_or(0);
                let term_a = i_usize.checked_mul(17).unwrap_or(0);
                let term_b = j.checked_mul(31).unwrap_or(0);
                let idx_raw = term_a.checked_add(term_b).unwrap_or(0);
                let idx = idx_raw.checked_rem(n).unwrap_or(0);
                if let Some(trade) = closed_trades.get(idx) {
                    sample.push(trade.clone());
                }
            }
            expectancy_samples.push(Self::calculate_expectancy(&sample));
            if let Some(sharpe) = Self::calculate_sharpe_ratio(&sample, risk_free_rate) {
                sharpe_samples.push(sharpe);
            }
        }

        BootstrapConfidenceIntervals {
            expectancy_95: Self::percentile_band(&mut expectancy_samples, dec!(0.025), dec!(0.975)),
            sharpe_95: Self::percentile_band(&mut sharpe_samples, dec!(0.025), dec!(0.975)),
        }
    }

    fn percentile_band(
        samples: &mut [Decimal],
        low: Decimal,
        high: Decimal,
    ) -> Option<(Decimal, Decimal)> {
        if samples.is_empty() {
            return None;
        }
        samples.sort();
        let low_idx = low
            .checked_mul(Decimal::from(samples.len().saturating_sub(1)))
            .and_then(|v| v.to_usize())
            .unwrap_or(0)
            .min(samples.len().saturating_sub(1));
        let high_idx = high
            .checked_mul(Decimal::from(samples.len().saturating_sub(1)))
            .and_then(|v| v.to_usize())
            .unwrap_or(samples.len().saturating_sub(1))
            .min(samples.len().saturating_sub(1));
        let low_value = samples.get(low_idx).copied();
        let high_value = samples.get(high_idx).copied();
        low_value.zip(high_value)
    }

    fn mean_opt(values: &[Decimal]) -> Option<Decimal> {
        if values.is_empty() {
            None
        } else {
            values
                .iter()
                .copied()
                .sum::<Decimal>()
                .checked_div(Decimal::from(values.len()))
        }
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
        trade.entry.unit_price = dec!(100);
        trade.safety_stop.unit_price = dec!(95);
        trade.target.unit_price = dec!(110);
        trade.entry.quantity = 10;
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
    fn test_gross_profit_loss_and_net_profit_math_adds_up() {
        let trades = vec![
            create_test_trade(dec!(100)),
            create_test_trade(dec!(50)),
            create_test_trade(dec!(-40)),
            create_test_trade(dec!(-10)),
        ];

        let gross_profit = AdvancedMetricsCalculator::calculate_gross_profit(&trades);
        let gross_loss = AdvancedMetricsCalculator::calculate_gross_loss(&trades);
        let net = AdvancedMetricsCalculator::calculate_net_profit(&trades);

        assert_eq!(gross_profit, dec!(150));
        assert_eq!(gross_loss, dec!(50));
        assert_eq!(net, dec!(100));

        // Identity: net = gross_profit - gross_loss
        assert_eq!(gross_profit - gross_loss, net);
    }

    #[test]
    fn test_payoff_ratio_average_win_over_average_loss() {
        let trades = vec![
            create_test_trade(dec!(100)),
            create_test_trade(dec!(50)),
            create_test_trade(dec!(-30)),
            create_test_trade(dec!(-10)),
        ];

        // avg win = 75, avg loss = 20 => 3.75
        let payoff = AdvancedMetricsCalculator::calculate_payoff_ratio(&trades).unwrap();
        assert!((payoff - dec!(3.75)).abs() < dec!(0.0001));
    }

    #[test]
    fn test_median_trade_pnl_even_and_odd() {
        let odd = vec![
            create_test_trade(dec!(10)),
            create_test_trade(dec!(5)),
            create_test_trade(dec!(-1)),
        ];
        assert_eq!(
            AdvancedMetricsCalculator::calculate_median_trade_pnl(&odd),
            Some(dec!(5))
        );

        let even = vec![
            create_test_trade(dec!(10)),
            create_test_trade(dec!(5)),
            create_test_trade(dec!(-1)),
            create_test_trade(dec!(-3)),
        ];
        // sorted: -3, -1, 5, 10 => median = ( -1 + 5 ) / 2 = 2
        assert_eq!(
            AdvancedMetricsCalculator::calculate_median_trade_pnl(&even),
            Some(dec!(2))
        );
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

    #[test]
    fn test_calculate_expected_shortfall_returns_tail_average() {
        let trades = vec![
            create_test_trade(dec!(-200)),
            create_test_trade(dec!(-100)),
            create_test_trade(dec!(50)),
            create_test_trade(dec!(150)),
        ];
        let es = AdvancedMetricsCalculator::calculate_expected_shortfall(&trades, dec!(0.95));
        assert!(es.is_some());
    }

    #[test]
    fn test_calculate_rolling_metrics_returns_points_per_window() {
        let trades = vec![
            create_test_trade(dec!(100)),
            create_test_trade(dec!(-50)),
            create_test_trade(dec!(80)),
            create_test_trade(dec!(-20)),
        ];
        let points = AdvancedMetricsCalculator::calculate_rolling_metrics(
            &trades,
            &[30, 90, 252],
            dec!(0.05),
        );
        assert_eq!(points.len(), 3);
        let first_window = points.first().expect("first rolling window");
        assert_eq!(first_window.window_days, 30);
        assert!(points.iter().all(|p| p.trade_count <= trades.len()));
    }

    #[test]
    fn test_calculate_execution_quality_handles_missing_fill_prices() {
        let mut trades = vec![create_test_trade(dec!(100)), create_test_trade(dec!(-50))];
        let first = trades.first_mut().expect("first trade");
        first.entry.average_filled_price = Some(dec!(101));
        first.entry.unit_price = dec!(100);
        first.target.unit_price = dec!(110);
        first.safety_stop.unit_price = dec!(95);

        let second = trades.get_mut(1).expect("second trade");
        second.entry.average_filled_price = None;
        second.entry.unit_price = dec!(100);
        second.target.unit_price = dec!(106);
        second.safety_stop.unit_price = dec!(97);

        let metrics = AdvancedMetricsCalculator::calculate_execution_quality(&trades);
        assert!(metrics.average_entry_slippage_bps.is_some());
        assert!(metrics.fill_price_coverage_percentage > dec!(0));
        assert!(metrics.fill_price_coverage_percentage < dec!(100));
        assert!(metrics.median_entry_slippage_bps.is_some());
        assert!(metrics.median_abs_entry_slippage_bps.is_some());
        assert!(metrics.p95_abs_entry_slippage_bps.is_some());
        assert_eq!(metrics.stop_fill_price_coverage_percentage, dec!(0));
        assert_eq!(metrics.target_fill_price_coverage_percentage, dec!(0));
    }

    #[test]
    fn test_calculate_journal_quality_percentages() {
        let mut t1 = create_test_trade(dec!(10));
        t1.thesis = Some("why".to_string());
        t1.sector = Some("Tech".to_string());
        t1.asset_class = Some("Stocks".to_string());
        t1.context = Some("notes".to_string());

        let mut t2 = create_test_trade(dec!(-1));
        t2.thesis = None;
        t2.sector = Some("".to_string()); // should count as missing
        t2.asset_class = Some("Stocks".to_string());
        t2.context = None;

        let metrics = AdvancedMetricsCalculator::calculate_journal_quality(&[t1, t2]);
        assert_eq!(metrics.thesis_coverage_percentage, dec!(50));
        assert_eq!(metrics.sector_coverage_percentage, dec!(50));
        assert_eq!(metrics.asset_class_coverage_percentage, dec!(100));
        assert_eq!(metrics.context_coverage_percentage, dec!(50));
        assert_eq!(metrics.complete_journal_percentage, dec!(50));
    }

    #[test]
    fn test_calculate_streak_metrics_tracks_current_and_max() {
        let now = chrono::Utc::now().naive_utc();

        let mut t1 = create_test_trade(dec!(10)); // win
        t1.updated_at = now;

        let mut t2 = create_test_trade(dec!(-1)); // loss
        t2.updated_at = now + chrono::Duration::seconds(1);

        let mut t3 = create_test_trade(dec!(-2)); // loss
        t3.updated_at = now + chrono::Duration::seconds(2);

        let mut t4 = create_test_trade(dec!(3)); // win
        t4.updated_at = now + chrono::Duration::seconds(3);

        let streaks = AdvancedMetricsCalculator::calculate_streak_metrics(&[t4, t1, t3, t2]);
        assert_eq!(streaks.max_consecutive_losses, 2);
        assert_eq!(streaks.max_consecutive_wins, 1);
        assert_eq!(streaks.current_streak_type, Some("win"));
        assert_eq!(streaks.current_streak_len, 1);
        assert!(streaks.average_loss_streak.is_some());
        assert!(streaks.average_win_streak.is_some());
    }

    #[test]
    fn test_calculate_exposure_metrics_with_open_capital() {
        let mut long = create_test_trade(dec!(100));
        long.category = TradeCategory::Long;
        long.balance.capital_in_market = dec!(1200);
        long.sector = Some("Technology".to_string());
        long.trading_vehicle.symbol = "AAPL".to_string();

        let mut short = create_test_trade(dec!(-50));
        short.category = TradeCategory::Short;
        short.balance.capital_in_market = dec!(800);
        short.sector = Some("Finance".to_string());
        short.trading_vehicle.symbol = "JPM".to_string();

        let metrics = AdvancedMetricsCalculator::calculate_exposure_metrics(&[long, short]);
        assert_eq!(metrics.gross_exposure, dec!(2000));
        assert_eq!(metrics.net_exposure, dec!(400));
        assert!(metrics.top_3_symbol_concentration_percentage <= dec!(100));
    }

    #[test]
    fn test_calculate_risk_of_ruin_proxy_in_bounds() {
        let trades = vec![
            create_test_trade(dec!(100)),
            create_test_trade(dec!(-50)),
            create_test_trade(dec!(-20)),
            create_test_trade(dec!(80)),
            create_test_trade(dec!(-10)),
        ];
        let risk = AdvancedMetricsCalculator::calculate_risk_of_ruin_proxy(&trades, 100, 6)
            .expect("risk-of-ruin proxy");
        assert!(risk >= dec!(0));
        assert!(risk <= dec!(1));
    }

    #[test]
    fn test_calculate_bootstrap_confidence_intervals_returns_bands() {
        let trades = vec![
            create_test_trade(dec!(100)),
            create_test_trade(dec!(-50)),
            create_test_trade(dec!(90)),
            create_test_trade(dec!(-40)),
            create_test_trade(dec!(70)),
        ];
        let ci = AdvancedMetricsCalculator::calculate_bootstrap_confidence_intervals(
            &trades,
            200,
            dec!(0.05),
        );
        assert!(ci.expectancy_95.is_some());
        assert!(ci.sharpe_95.is_some());
        let (low, high) = ci.expectancy_95.expect("expectancy band");
        assert!(high >= low);
    }
}
