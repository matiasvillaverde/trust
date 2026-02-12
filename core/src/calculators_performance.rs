//! Performance calculation module for trading statistics
//!
//! This module provides functions to calculate various trading performance
//! metrics using precise decimal arithmetic for financial safety.

use model::trade::{Status, Trade};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Error types for performance calculations
#[derive(Debug, PartialEq)]
pub enum PerformanceError {
    /// Trade is not in a closed state
    TradeNotClosed,
    /// Zero risk detected (entry price equals stop price)
    ZeroRisk,
    /// Arithmetic overflow in calculation
    ArithmeticOverflow,
}

/// Performance statistics for a collection of trades
#[derive(Debug, PartialEq, Clone)]
pub struct PerformanceStats {
    /// Total number of trades analyzed
    pub total_trades: usize,
    /// Number of profitable trades
    pub winning_trades: usize,
    /// Number of losing trades
    pub losing_trades: usize,
    /// Percentage of winning trades (0.0 to 100.0)
    pub win_rate: Decimal,
    /// Average profit amount for winning trades
    pub average_win: Decimal,
    /// Average loss amount for losing trades
    pub average_loss: Decimal,
    /// Average R-Multiple across all trades
    pub average_r_multiple: Decimal,
    /// Best performing trade (highest profit)
    pub best_trade: Option<Decimal>,
    /// Worst performing trade (largest loss)
    pub worst_trade: Option<Decimal>,
}

/// Calculator for trading performance metrics and statistics
#[derive(Debug)]
pub struct PerformanceCalculator;

impl PerformanceCalculator {
    /// Calculate win rate as percentage (0.0 to 100.0)
    pub fn calculate_win_rate(closed_trades: &[Trade]) -> Decimal {
        if closed_trades.is_empty() {
            return dec!(0);
        }

        let winning_trades = closed_trades
            .iter()
            .filter(|trade| trade.balance.total_performance > dec!(0))
            .count();

        let winning_decimal = Decimal::from(winning_trades);
        let total_decimal = Decimal::from(closed_trades.len());

        const HUNDRED_PERCENT: Decimal = dec!(100);

        winning_decimal
            .checked_div(total_decimal)
            .unwrap_or(dec!(0))
            .checked_mul(HUNDRED_PERCENT)
            .unwrap_or(dec!(0))
    }

    /// Calculate average R-Multiple across all closed trades
    pub fn calculate_average_r_multiple(closed_trades: &[Trade]) -> Decimal {
        if closed_trades.is_empty() {
            return dec!(0);
        }

        let r_multiples: Vec<Decimal> = closed_trades
            .iter()
            .filter_map(Self::calculate_r_multiple)
            .collect();

        if r_multiples.is_empty() {
            return dec!(0);
        }

        let sum = r_multiples.iter().sum::<Decimal>();
        let count = Decimal::from(r_multiples.len());

        sum.checked_div(count).unwrap_or(dec!(0))
    }

    /// Calculate average win and loss amounts
    pub fn calculate_average_win_loss(closed_trades: &[Trade]) -> (Decimal, Decimal) {
        let wins: Vec<Decimal> = closed_trades
            .iter()
            .filter(|trade| trade.balance.total_performance > dec!(0))
            .map(|trade| trade.balance.total_performance)
            .collect();

        let losses: Vec<Decimal> = closed_trades
            .iter()
            .filter(|trade| trade.balance.total_performance < dec!(0))
            .map(|trade| trade.balance.total_performance)
            .collect();

        let avg_win = if wins.is_empty() {
            dec!(0)
        } else {
            let sum = wins.iter().sum::<Decimal>();
            let count = Decimal::from(wins.len());
            sum.checked_div(count).unwrap_or(dec!(0))
        };

        let avg_loss = if losses.is_empty() {
            dec!(0)
        } else {
            let sum = losses.iter().sum::<Decimal>();
            let count = Decimal::from(losses.len());
            sum.checked_div(count).unwrap_or(dec!(0))
        };

        (avg_win, avg_loss)
    }

    /// Find best and worst performing trades
    pub fn calculate_best_worst_trades(
        closed_trades: &[Trade],
    ) -> (Option<Decimal>, Option<Decimal>) {
        if closed_trades.is_empty() {
            return (None, None);
        }

        let performances: Vec<Decimal> = closed_trades
            .iter()
            .map(|trade| trade.balance.total_performance)
            .collect();

        let best = performances.iter().max().copied();
        let worst = performances.iter().min().copied();

        (best, worst)
    }

    /// Calculate R-Multiple for a single trade
    /// For Long trades: R-Multiple = (Exit Price - Entry Price) / (Entry Price - Stop Price)
    /// For Short trades: R-Multiple = (Entry Price - Exit Price) / (Stop Price - Entry Price)
    fn calculate_r_multiple(trade: &Trade) -> Option<Decimal> {
        Self::calculate_r_multiple_with_context(trade).ok()
    }

    /// Calculate R-Multiple with detailed error context
    /// For Long trades: R-Multiple = (Exit Price - Entry Price) / (Entry Price - Stop Price)
    /// For Short trades: R-Multiple = (Entry Price - Exit Price) / (Stop Price - Entry Price)
    fn calculate_r_multiple_with_context(trade: &Trade) -> Result<Decimal, PerformanceError> {
        use model::TradeCategory;

        let entry_price = trade.entry.unit_price;
        let stop_price = trade.safety_stop.unit_price;
        let exit_price = match trade.status {
            Status::ClosedTarget => trade.target.unit_price,
            Status::ClosedStopLoss => stop_price,
            _ => return Err(PerformanceError::TradeNotClosed),
        };

        // Calculate risk per share based on trade direction
        let risk = match trade.category {
            TradeCategory::Long => entry_price
                .checked_sub(stop_price)
                .ok_or(PerformanceError::ArithmeticOverflow)?,
            TradeCategory::Short => stop_price
                .checked_sub(entry_price)
                .ok_or(PerformanceError::ArithmeticOverflow)?,
        };

        if risk == dec!(0) {
            return Err(PerformanceError::ZeroRisk);
        }

        // Calculate profit/loss per share based on trade direction
        let pnl = match trade.category {
            TradeCategory::Long => exit_price
                .checked_sub(entry_price)
                .ok_or(PerformanceError::ArithmeticOverflow)?,
            TradeCategory::Short => entry_price
                .checked_sub(exit_price)
                .ok_or(PerformanceError::ArithmeticOverflow)?,
        };

        // R-Multiple = PnL / Risk
        pnl.checked_div(risk)
            .ok_or(PerformanceError::ArithmeticOverflow)
    }

    /// Filter trades to only include closed ones
    pub fn filter_closed_trades(trades: &[Trade]) -> Vec<Trade> {
        trades
            .iter()
            .filter(|trade| matches!(trade.status, Status::ClosedTarget | Status::ClosedStopLoss))
            .cloned()
            .collect()
    }

    /// Filter trades by date range - only include trades updated within the last N days
    pub fn filter_trades_by_days(trades: &[Trade], days: u32) -> Vec<Trade> {
        use chrono::{Duration, Utc};

        let now = Utc::now().naive_utc();
        let duration = Duration::days(i64::from(days));
        let cutoff_date = now.checked_sub_signed(duration).unwrap_or(now);

        trades
            .iter()
            .filter(|trade| trade.updated_at >= cutoff_date)
            .cloned()
            .collect()
    }

    /// Calculate comprehensive performance statistics
    pub fn calculate_performance_stats(closed_trades: &[Trade]) -> PerformanceStats {
        let total_trades = closed_trades.len();
        let winning_trades = closed_trades
            .iter()
            .filter(|trade| trade.balance.total_performance > dec!(0))
            .count();
        let losing_trades = total_trades.saturating_sub(winning_trades);

        let win_rate = Self::calculate_win_rate(closed_trades);
        let (average_win, average_loss) = Self::calculate_average_win_loss(closed_trades);
        let average_r_multiple = Self::calculate_average_r_multiple(closed_trades);
        let (best_trade, worst_trade) = Self::calculate_best_worst_trades(closed_trades);

        PerformanceStats {
            total_trades,
            winning_trades,
            losing_trades,
            win_rate,
            average_win,
            average_loss,
            average_r_multiple,
            best_trade,
            worst_trade,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::trade::Trade;

    fn create_test_trade(
        entry_price: Decimal,
        stop_price: Decimal,
        target_price: Decimal,
        status: Status,
        performance: Decimal,
    ) -> Trade {
        use model::TradeCategory;

        let mut trade = Trade::default();
        trade.entry.unit_price = entry_price;
        trade.safety_stop.unit_price = stop_price;
        trade.target.unit_price = target_price;
        trade.status = status;
        trade.balance.total_performance = performance;
        trade.category = TradeCategory::Long; // Default to Long
        trade
    }

    fn create_test_trade_with_category(
        entry_price: Decimal,
        stop_price: Decimal,
        target_price: Decimal,
        status: Status,
        performance: Decimal,
        category: model::TradeCategory,
    ) -> Trade {
        let mut trade = Trade::default();
        trade.entry.unit_price = entry_price;
        trade.safety_stop.unit_price = stop_price;
        trade.target.unit_price = target_price;
        trade.status = status;
        trade.balance.total_performance = performance;
        trade.category = category;
        trade
    }

    #[test]
    fn test_calculate_win_rate_empty_trades() {
        let trades = vec![];
        let win_rate = PerformanceCalculator::calculate_win_rate(&trades);
        assert_eq!(win_rate, dec!(0));
    }

    #[test]
    fn test_calculate_win_rate_all_winners() {
        let trades = vec![
            create_test_trade(
                dec!(100),
                dec!(95),
                dec!(110),
                Status::ClosedTarget,
                dec!(100),
            ),
            create_test_trade(
                dec!(200),
                dec!(190),
                dec!(220),
                Status::ClosedTarget,
                dec!(200),
            ),
        ];
        let win_rate = PerformanceCalculator::calculate_win_rate(&trades);
        assert_eq!(win_rate, dec!(100));
    }

    #[test]
    fn test_calculate_win_rate_mixed() {
        let trades = vec![
            create_test_trade(
                dec!(100),
                dec!(95),
                dec!(110),
                Status::ClosedTarget,
                dec!(100),
            ),
            create_test_trade(
                dec!(200),
                dec!(190),
                dec!(220),
                Status::ClosedStopLoss,
                dec!(-50),
            ),
            create_test_trade(
                dec!(300),
                dec!(290),
                dec!(320),
                Status::ClosedTarget,
                dec!(150),
            ),
        ];
        let win_rate = PerformanceCalculator::calculate_win_rate(&trades);
        let expected_rate = dec!(66.67);
        assert!((win_rate - expected_rate).abs() < dec!(0.1)); // 2/3 ≈ 66.67%
    }

    #[test]
    fn test_calculate_average_win_loss_empty() {
        let trades = vec![];
        let (avg_win, avg_loss) = PerformanceCalculator::calculate_average_win_loss(&trades);
        assert_eq!(avg_win, dec!(0));
        assert_eq!(avg_loss, dec!(0));
    }

    #[test]
    fn test_calculate_average_win_loss_mixed() {
        let trades = vec![
            create_test_trade(
                dec!(100),
                dec!(95),
                dec!(110),
                Status::ClosedTarget,
                dec!(100),
            ),
            create_test_trade(
                dec!(200),
                dec!(190),
                dec!(220),
                Status::ClosedStopLoss,
                dec!(-50),
            ),
            create_test_trade(
                dec!(300),
                dec!(290),
                dec!(320),
                Status::ClosedTarget,
                dec!(200),
            ),
        ];
        let (avg_win, avg_loss) = PerformanceCalculator::calculate_average_win_loss(&trades);
        assert_eq!(avg_win, dec!(150)); // (100 + 200) / 2
        assert_eq!(avg_loss, dec!(-50)); // -50 / 1
    }

    #[test]
    fn test_calculate_r_multiple_target_hit() {
        let trade = create_test_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Status::ClosedTarget,
            dec!(0),
        );
        let r_multiple = PerformanceCalculator::calculate_r_multiple(&trade);
        assert_eq!(r_multiple, Some(dec!(2))); // (110-100)/(100-95) = 10/5 = 2.0
    }

    #[test]
    fn test_calculate_r_multiple_stop_hit() {
        let trade = create_test_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Status::ClosedStopLoss,
            dec!(0),
        );
        let r_multiple = PerformanceCalculator::calculate_r_multiple(&trade);
        assert_eq!(r_multiple, Some(dec!(-1))); // (95-100)/(100-95) = -5/5 = -1.0
    }

    #[test]
    fn test_calculate_r_multiple_not_closed() {
        let trade = create_test_trade(dec!(100), dec!(95), dec!(110), Status::Filled, dec!(0));
        let r_multiple = PerformanceCalculator::calculate_r_multiple(&trade);
        assert_eq!(r_multiple, None);
    }

    #[test]
    fn test_calculate_r_multiple_short_target_hit() {
        use model::TradeCategory;

        // Short trade: Entry at 100, Stop at 105, Target at 90
        let trade = create_test_trade_with_category(
            dec!(100), // entry
            dec!(105), // stop (higher for short)
            dec!(90),  // target (lower for short)
            Status::ClosedTarget,
            dec!(0),
            TradeCategory::Short,
        );
        let r_multiple = PerformanceCalculator::calculate_r_multiple(&trade);
        // Risk = 105 - 100 = 5
        // PnL = 100 - 90 = 10 (profit on short)
        // R-Multiple = 10 / 5 = 2.0
        assert_eq!(r_multiple, Some(dec!(2)));
    }

    #[test]
    fn test_calculate_r_multiple_short_stop_hit() {
        use model::TradeCategory;

        // Short trade: Entry at 100, Stop at 105, exits at stop
        let trade = create_test_trade_with_category(
            dec!(100), // entry
            dec!(105), // stop (higher for short)
            dec!(90),  // target (not reached)
            Status::ClosedStopLoss,
            dec!(0),
            TradeCategory::Short,
        );
        let r_multiple = PerformanceCalculator::calculate_r_multiple(&trade);
        // Risk = 105 - 100 = 5
        // PnL = 100 - 105 = -5 (loss on short)
        // R-Multiple = -5 / 5 = -1.0
        assert_eq!(r_multiple, Some(dec!(-1)));
    }

    #[test]
    fn test_calculate_r_multiple_zero_risk() {
        // Edge case: entry equals stop (zero risk)
        let trade = create_test_trade(
            dec!(100),
            dec!(100), // stop equals entry
            dec!(110),
            Status::ClosedTarget,
            dec!(0),
        );
        let r_multiple = PerformanceCalculator::calculate_r_multiple(&trade);
        assert_eq!(r_multiple, None); // Should return None for zero risk
    }

    #[test]
    fn test_calculate_r_multiple_with_result_zero_risk_error() {
        // Test the new Result-based method
        let trade = create_test_trade(
            dec!(100),
            dec!(100), // stop equals entry
            dec!(110),
            Status::ClosedTarget,
            dec!(0),
        );
        let result = PerformanceCalculator::calculate_r_multiple_with_context(&trade);
        assert_eq!(result, Err(PerformanceError::ZeroRisk));
    }

    #[test]
    fn test_calculate_r_multiple_with_result_not_closed_error() {
        // Test the new Result-based method for non-closed trade
        let trade = create_test_trade(dec!(100), dec!(95), dec!(110), Status::Filled, dec!(0));
        let result = PerformanceCalculator::calculate_r_multiple_with_context(&trade);
        assert_eq!(result, Err(PerformanceError::TradeNotClosed));
    }

    #[test]
    fn test_filter_closed_trades() {
        let trades = vec![
            create_test_trade(
                dec!(100),
                dec!(95),
                dec!(110),
                Status::ClosedTarget,
                dec!(100),
            ),
            create_test_trade(dec!(200), dec!(190), dec!(220), Status::Filled, dec!(0)),
            create_test_trade(
                dec!(300),
                dec!(290),
                dec!(320),
                Status::ClosedStopLoss,
                dec!(-50),
            ),
        ];
        let closed = PerformanceCalculator::filter_closed_trades(&trades);
        assert_eq!(closed.len(), 2);
        assert!(matches!(
            closed.first().unwrap().status,
            Status::ClosedTarget
        ));
        assert!(matches!(
            closed.get(1).unwrap().status,
            Status::ClosedStopLoss
        ));
    }

    #[test]
    fn test_filter_trades_by_days() {
        use chrono::{Duration, Utc};

        let now = Utc::now().naive_utc();

        // Create trades with different updated_at dates
        let mut recent_trade = create_test_trade(
            dec!(100),
            dec!(95),
            dec!(110),
            Status::ClosedTarget,
            dec!(100),
        );
        recent_trade.updated_at = now - Duration::days(5);

        let mut old_trade = create_test_trade(
            dec!(200),
            dec!(190),
            dec!(220),
            Status::ClosedTarget,
            dec!(200),
        );
        old_trade.updated_at = now - Duration::days(15);

        let mut very_old_trade = create_test_trade(
            dec!(300),
            dec!(290),
            dec!(320),
            Status::ClosedTarget,
            dec!(300),
        );
        very_old_trade.updated_at = now - Duration::days(40);

        let trades = vec![
            recent_trade.clone(),
            old_trade.clone(),
            very_old_trade.clone(),
        ];

        // Filter last 7 days - should get only recent_trade
        let filtered_7 = PerformanceCalculator::filter_trades_by_days(&trades, 7);
        assert_eq!(filtered_7.len(), 1);
        assert_eq!(filtered_7.first().unwrap().id, recent_trade.id);

        // Filter last 30 days - should get recent_trade and old_trade
        let filtered_30 = PerformanceCalculator::filter_trades_by_days(&trades, 30);
        assert_eq!(filtered_30.len(), 2);

        // Filter last 60 days - should get all trades
        let filtered_60 = PerformanceCalculator::filter_trades_by_days(&trades, 60);
        assert_eq!(filtered_60.len(), 3);
    }

    #[test]
    fn test_calculate_best_worst_trades() {
        let trades = vec![
            create_test_trade(
                dec!(100),
                dec!(95),
                dec!(110),
                Status::ClosedTarget,
                dec!(100),
            ),
            create_test_trade(
                dec!(200),
                dec!(190),
                dec!(220),
                Status::ClosedStopLoss,
                dec!(-50),
            ),
            create_test_trade(
                dec!(300),
                dec!(290),
                dec!(320),
                Status::ClosedTarget,
                dec!(200),
            ),
        ];
        let (best, worst) = PerformanceCalculator::calculate_best_worst_trades(&trades);
        assert_eq!(best, Some(dec!(200)));
        assert_eq!(worst, Some(dec!(-50)));
    }

    #[test]
    fn test_calculate_performance_stats_comprehensive() {
        let trades = vec![
            create_test_trade(
                dec!(100),
                dec!(95),
                dec!(110),
                Status::ClosedTarget,
                dec!(100),
            ),
            create_test_trade(
                dec!(200),
                dec!(190),
                dec!(220),
                Status::ClosedStopLoss,
                dec!(-50),
            ),
            create_test_trade(
                dec!(300),
                dec!(290),
                dec!(320),
                Status::ClosedTarget,
                dec!(200),
            ),
        ];
        let stats = PerformanceCalculator::calculate_performance_stats(&trades);

        assert_eq!(stats.total_trades, 3);
        assert_eq!(stats.winning_trades, 2);
        assert_eq!(stats.losing_trades, 1);
        let expected_rate = dec!(66.67);
        assert!((stats.win_rate - expected_rate).abs() < dec!(0.1));
        assert_eq!(stats.average_win, dec!(150));
        assert_eq!(stats.average_loss, dec!(-50));
        assert_eq!(stats.best_trade, Some(dec!(200)));
        assert_eq!(stats.worst_trade, Some(dec!(-50)));
    }
}
