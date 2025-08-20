//! Drawdown calculation module for realized P&L analysis
//!
//! This module provides functions to calculate drawdown metrics based on
//! closed trades and account transactions using precise decimal arithmetic.

use chrono::NaiveDateTime;
use model::transaction::{Transaction, TransactionCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;

/// A point in time representing the account's equity
#[derive(Debug, Clone, PartialEq)]
pub struct EquityPoint {
    /// Timestamp of the equity calculation
    pub timestamp: NaiveDateTime,
    /// Account balance at this point in time
    pub balance: Decimal,
}

/// Represents the equity curve over time
#[derive(Debug, PartialEq)]
pub struct RealizedEquityCurve {
    /// Chronologically ordered equity points
    pub points: Vec<EquityPoint>,
}

/// Comprehensive drawdown metrics
#[derive(Debug, PartialEq)]
pub struct DrawdownMetrics {
    /// Current account equity
    pub current_equity: Decimal,
    /// All-time peak equity
    pub peak_equity: Decimal,
    /// Current drawdown amount
    pub current_drawdown: Decimal,
    /// Current drawdown percentage
    pub current_drawdown_percentage: Decimal,
    /// Maximum drawdown amount
    pub max_drawdown: Decimal,
    /// Maximum drawdown percentage
    pub max_drawdown_percentage: Decimal,
    /// Date of maximum drawdown
    pub max_drawdown_date: Option<NaiveDateTime>,
    /// Recovery amount from maximum drawdown
    pub recovery_from_max: Decimal,
    /// Recovery percentage from maximum drawdown
    pub recovery_percentage: Decimal,
    /// Number of days since peak
    pub days_since_peak: i64,
    /// Number of days in current drawdown
    pub days_in_drawdown: i64,
}

/// Internal structure for peak and drawdown calculations
struct PeakDrawdownData {
    peak_equity: Decimal,
    peak_date: NaiveDateTime,
    max_drawdown: Decimal,
    max_drawdown_percentage: Decimal,
    max_drawdown_date: Option<NaiveDateTime>,
    max_drawdown_trough: Decimal,
}

/// Calculator for realized drawdown metrics
#[derive(Debug)]
pub struct RealizedDrawdownCalculator;

impl RealizedDrawdownCalculator {
    /// Calculate equity curve from transaction history
    pub fn calculate_equity_curve(
        transactions: &[Transaction],
    ) -> Result<RealizedEquityCurve, Box<dyn Error>> {
        if transactions.is_empty() {
            return Ok(RealizedEquityCurve { points: vec![] });
        }

        // Sort transactions chronologically
        let mut sorted_transactions = transactions.to_vec();
        sorted_transactions.sort_by_key(|t| t.created_at);

        let mut current_balance = dec!(0);
        let mut equity_points = Vec::new();

        for transaction in sorted_transactions {
            // Update balance based on transaction category
            // Key insight: We track the account balance, not the total capital
            match transaction.category {
                TransactionCategory::Deposit => {
                    current_balance = current_balance
                        .checked_add(transaction.amount)
                        .ok_or("Arithmetic overflow in deposit")?;
                }
                TransactionCategory::Withdrawal | TransactionCategory::WithdrawalEarnings => {
                    current_balance = current_balance
                        .checked_sub(transaction.amount)
                        .ok_or("Arithmetic overflow in withdrawal")?;
                }
                // When we fund a trade, money leaves the account
                TransactionCategory::FundTrade(_) => {
                    current_balance = current_balance
                        .checked_sub(transaction.amount)
                        .ok_or("Arithmetic overflow in fund trade")?;
                }
                // When a trade returns money, it comes back to the account
                TransactionCategory::PaymentFromTrade(_) => {
                    current_balance = current_balance
                        .checked_add(transaction.amount)
                        .ok_or("Arithmetic overflow in payment from trade")?;
                }
                // Fees are deducted from the account
                TransactionCategory::FeeOpen(_) | TransactionCategory::FeeClose(_) => {
                    current_balance = current_balance
                        .checked_sub(transaction.amount.abs())
                        .ok_or("Arithmetic overflow in fee")?;
                }
                // These categories represent movements within trades, not account balance changes
                TransactionCategory::CloseTarget(_)
                | TransactionCategory::CloseSafetyStop(_)
                | TransactionCategory::CloseSafetyStopSlippage(_)
                | TransactionCategory::OpenTrade(_) => {
                    // These don't directly affect account balance
                    // The actual money flow happens via PaymentFromTrade
                    continue;
                }
                // Ignore other transaction types for equity calculation
                _ => continue,
            }

            equity_points.push(EquityPoint {
                timestamp: transaction.created_at,
                balance: current_balance,
            });
        }

        Ok(RealizedEquityCurve {
            points: equity_points,
        })
    }

    /// Calculate drawdown metrics from equity curve
    pub fn calculate_drawdown_metrics(
        curve: &RealizedEquityCurve,
    ) -> Result<DrawdownMetrics, Box<dyn Error>> {
        use chrono::Utc;

        if curve.points.is_empty() {
            return Ok(Self::empty_metrics());
        }

        let current_equity = curve
            .points
            .last()
            .map(|p| p.balance)
            .ok_or("No equity points")?;

        let first_point = curve.points.first().ok_or("No first equity point")?;

        let peak_data = Self::calculate_peak_and_drawdown(&curve.points, first_point.timestamp)?;

        let (current_drawdown, current_drawdown_percentage) =
            Self::calculate_current_drawdown(current_equity, peak_data.peak_equity)?;

        let (recovery_from_max, recovery_percentage) = Self::calculate_recovery(
            current_equity,
            peak_data.max_drawdown_trough,
            peak_data.max_drawdown,
        )?;

        let now = Utc::now().naive_utc();
        let days_since_peak = now.signed_duration_since(peak_data.peak_date).num_days();
        let days_in_drawdown = if current_drawdown > dec!(0) {
            days_since_peak
        } else {
            0
        };

        Ok(DrawdownMetrics {
            current_equity,
            peak_equity: peak_data.peak_equity,
            current_drawdown,
            current_drawdown_percentage,
            max_drawdown: peak_data.max_drawdown,
            max_drawdown_percentage: peak_data.max_drawdown_percentage,
            max_drawdown_date: peak_data.max_drawdown_date,
            recovery_from_max,
            recovery_percentage,
            days_since_peak,
            days_in_drawdown,
        })
    }

    fn empty_metrics() -> DrawdownMetrics {
        DrawdownMetrics {
            current_equity: dec!(0),
            peak_equity: dec!(0),
            current_drawdown: dec!(0),
            current_drawdown_percentage: dec!(0),
            max_drawdown: dec!(0),
            max_drawdown_percentage: dec!(0),
            max_drawdown_date: None,
            recovery_from_max: dec!(0),
            recovery_percentage: dec!(0),
            days_since_peak: 0,
            days_in_drawdown: 0,
        }
    }

    fn calculate_peak_and_drawdown(
        points: &[EquityPoint],
        initial_date: NaiveDateTime,
    ) -> Result<PeakDrawdownData, Box<dyn Error>> {
        let mut peak_equity = dec!(0);
        let mut peak_date = initial_date;
        let mut max_drawdown = dec!(0);
        let mut max_drawdown_percentage = dec!(0);
        let mut max_drawdown_date = None;
        let mut max_drawdown_trough = dec!(0);

        for point in points {
            if point.balance > peak_equity {
                peak_equity = point.balance;
                peak_date = point.timestamp;
            }

            let drawdown = peak_equity
                .checked_sub(point.balance)
                .ok_or("Arithmetic overflow in drawdown calculation")?;

            if drawdown > max_drawdown {
                max_drawdown = drawdown;
                max_drawdown_date = Some(point.timestamp);
                max_drawdown_trough = point.balance;

                if peak_equity > dec!(0) {
                    max_drawdown_percentage = drawdown
                        .checked_mul(dec!(100))
                        .and_then(|d| d.checked_div(peak_equity))
                        .ok_or("Arithmetic overflow in percentage calculation")?;
                }
            }
        }

        Ok(PeakDrawdownData {
            peak_equity,
            peak_date,
            max_drawdown,
            max_drawdown_percentage,
            max_drawdown_date,
            max_drawdown_trough,
        })
    }

    fn calculate_current_drawdown(
        current_equity: Decimal,
        peak_equity: Decimal,
    ) -> Result<(Decimal, Decimal), Box<dyn Error>> {
        let current_drawdown = peak_equity
            .checked_sub(current_equity)
            .ok_or("Arithmetic overflow in current drawdown")?;

        let current_drawdown_percentage = if peak_equity > dec!(0) {
            current_drawdown
                .checked_mul(dec!(100))
                .and_then(|d| d.checked_div(peak_equity))
                .ok_or("Arithmetic overflow in percentage")?
        } else {
            dec!(0)
        };

        Ok((current_drawdown, current_drawdown_percentage))
    }

    fn calculate_recovery(
        current_equity: Decimal,
        max_drawdown_trough: Decimal,
        max_drawdown: Decimal,
    ) -> Result<(Decimal, Decimal), Box<dyn Error>> {
        let recovery_from_max = if max_drawdown > dec!(0) {
            current_equity
                .checked_sub(max_drawdown_trough)
                .ok_or("Arithmetic overflow in recovery calculation")?
        } else {
            dec!(0)
        };

        let recovery_percentage = if max_drawdown > dec!(0) {
            recovery_from_max
                .checked_mul(dec!(100))
                .and_then(|r| r.checked_div(max_drawdown))
                .ok_or("Arithmetic overflow in recovery percentage")?
        } else {
            dec!(0)
        };

        Ok((recovery_from_max, recovery_percentage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use model::Currency;
    use uuid::Uuid;

    fn create_transaction(
        category: TransactionCategory,
        amount: Decimal,
        days_ago: i64,
    ) -> Transaction {
        let now = Utc::now().naive_utc();
        let duration = Duration::days(days_ago);
        let created_at = now.checked_sub_signed(duration).unwrap_or(now);
        Transaction {
            id: Uuid::new_v4(),
            created_at,
            updated_at: created_at,
            deleted_at: None,
            category,
            currency: Currency::USD,
            amount,
            account_id: Uuid::new_v4(),
        }
    }

    #[test]
    fn test_empty_transaction_history() {
        let transactions = vec![];
        let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions)
            .expect("Failed to calculate equity curve");

        assert!(curve.points.is_empty());

        let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve)
            .expect("Failed to calculate metrics");

        assert_eq!(metrics.current_equity, dec!(0));
        assert_eq!(metrics.peak_equity, dec!(0));
        assert_eq!(metrics.current_drawdown, dec!(0));
    }

    #[test]
    fn test_single_deposit() {
        let transactions = vec![create_transaction(
            TransactionCategory::Deposit,
            dec!(10000),
            30,
        )];

        let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions)
            .expect("Failed to calculate equity curve");

        assert_eq!(curve.points.len(), 1);
        assert_eq!(curve.points.first().unwrap().balance, dec!(10000));

        let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve)
            .expect("Failed to calculate metrics");

        assert_eq!(metrics.current_equity, dec!(10000));
        assert_eq!(metrics.peak_equity, dec!(10000));
        assert_eq!(metrics.current_drawdown, dec!(0));
        assert_eq!(metrics.current_drawdown_percentage, dec!(0));
    }

    #[test]
    fn test_deposit_and_withdrawal() {
        let transactions = vec![
            create_transaction(TransactionCategory::Deposit, dec!(10000), 30),
            create_transaction(TransactionCategory::Withdrawal, dec!(2000), 20),
        ];

        let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions)
            .expect("Failed to calculate equity curve");

        assert_eq!(curve.points.len(), 2);
        assert_eq!(curve.points.get(1).unwrap().balance, dec!(8000));

        let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve)
            .expect("Failed to calculate metrics");

        assert_eq!(metrics.current_equity, dec!(8000));
        assert_eq!(metrics.peak_equity, dec!(10000));
        assert_eq!(metrics.current_drawdown, dec!(2000));
        assert_eq!(metrics.current_drawdown_percentage, dec!(20));
    }

    #[test]
    fn test_profitable_trades_no_drawdown() {
        let trade_id = Uuid::new_v4();
        let transactions = vec![
            create_transaction(TransactionCategory::Deposit, dec!(10000), 30),
            create_transaction(TransactionCategory::FundTrade(trade_id), dec!(1000), 25),
            create_transaction(TransactionCategory::FeeOpen(trade_id), dec!(10), 24),
            create_transaction(TransactionCategory::FeeClose(trade_id), dec!(10), 20),
            create_transaction(
                TransactionCategory::PaymentFromTrade(trade_id),
                dec!(1480),
                19,
            ),
        ];

        let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions)
            .expect("Failed to calculate equity curve");

        let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve)
            .expect("Failed to calculate metrics");

        // Started with 10000, funded 1000, paid 10 open fee, paid 10 close fee, got back 1480
        // Balance progression: 10000 -> 9000 -> 8990 -> 8980 -> 10460
        assert_eq!(metrics.current_equity, dec!(10460));
        assert_eq!(metrics.peak_equity, dec!(10460));
        assert_eq!(metrics.current_drawdown, dec!(0));
        // During the trade there was a temporary drawdown of 1020
        assert_eq!(metrics.max_drawdown, dec!(1020));
    }

    #[test]
    fn test_losing_trade_creates_drawdown() {
        let trade_id = Uuid::new_v4();
        let transactions = vec![
            create_transaction(TransactionCategory::Deposit, dec!(10000), 30),
            create_transaction(TransactionCategory::FundTrade(trade_id), dec!(1000), 25),
            create_transaction(TransactionCategory::FeeOpen(trade_id), dec!(10), 24),
            create_transaction(TransactionCategory::FeeClose(trade_id), dec!(10), 20),
            create_transaction(
                TransactionCategory::PaymentFromTrade(trade_id),
                dec!(780),
                19,
            ),
        ];

        let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions)
            .expect("Failed to calculate equity curve");

        let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve)
            .expect("Failed to calculate metrics");

        // Balance progression: 10000 -> 9000 -> 8990 -> 8980 -> 9760
        assert_eq!(metrics.current_equity, dec!(9760));
        assert_eq!(metrics.peak_equity, dec!(10000));
        assert_eq!(metrics.current_drawdown, dec!(240));
        assert_eq!(metrics.current_drawdown_percentage, dec!(2.4));
        // Max drawdown was when we were at 8980 (1020 from peak)
        assert_eq!(metrics.max_drawdown, dec!(1020));
        assert_eq!(metrics.max_drawdown_percentage, dec!(10.2));
    }

    #[test]
    fn test_recovery_from_drawdown() {
        let trade1_id = Uuid::new_v4();
        let trade2_id = Uuid::new_v4();

        let transactions = vec![
            create_transaction(TransactionCategory::Deposit, dec!(10000), 30),
            // Losing trade
            create_transaction(TransactionCategory::FundTrade(trade1_id), dec!(1000), 25),
            create_transaction(
                TransactionCategory::PaymentFromTrade(trade1_id),
                dec!(800),
                24,
            ),
            // Winning trade
            create_transaction(TransactionCategory::FundTrade(trade2_id), dec!(1000), 20),
            create_transaction(
                TransactionCategory::PaymentFromTrade(trade2_id),
                dec!(1300),
                15,
            ),
        ];

        let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions)
            .expect("Failed to calculate equity curve");

        let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve)
            .expect("Failed to calculate metrics");

        // Balance progression: 10000 -> 9000 -> 9800 -> 8800 -> 10100
        assert_eq!(metrics.current_equity, dec!(10100));
        assert_eq!(metrics.peak_equity, dec!(10100));
        assert_eq!(metrics.current_drawdown, dec!(0));
        // Max drawdown was 1200 (when at 8800)
        assert_eq!(metrics.max_drawdown, dec!(1200));
        // Recovery from 8800 to 10100 = 1300
        assert_eq!(metrics.recovery_from_max, dec!(1300));
        // Recovered 1300/1200 = 108.33%
        let expected_recovery = dec!(1300)
            .checked_mul(dec!(100))
            .and_then(|n| n.checked_div(dec!(1200)))
            .unwrap();
        assert!((metrics.recovery_percentage - expected_recovery).abs() < dec!(0.01));
    }

    #[test]
    fn test_multiple_drawdowns() {
        let trade1_id = Uuid::new_v4();
        let trade2_id = Uuid::new_v4();
        let trade3_id = Uuid::new_v4();

        let transactions = vec![
            create_transaction(TransactionCategory::Deposit, dec!(10000), 30),
            // First loss (-100)
            create_transaction(TransactionCategory::FundTrade(trade1_id), dec!(1000), 25),
            create_transaction(
                TransactionCategory::PaymentFromTrade(trade1_id),
                dec!(900),
                24,
            ),
            // Recovery (+150)
            create_transaction(TransactionCategory::FundTrade(trade2_id), dec!(1000), 20),
            create_transaction(
                TransactionCategory::PaymentFromTrade(trade2_id),
                dec!(1150),
                19,
            ),
            // Bigger loss (-500)
            create_transaction(TransactionCategory::FundTrade(trade3_id), dec!(2000), 15),
            create_transaction(
                TransactionCategory::PaymentFromTrade(trade3_id),
                dec!(1500),
                14,
            ),
        ];

        let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions)
            .expect("Failed to calculate equity curve");

        let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve)
            .expect("Failed to calculate metrics");

        // Balance progression:
        // 10000 -> 9000 -> 9900 (first trade, net -100)
        // 9900 -> 8900 -> 10050 (second trade, net +150, new peak)
        // 10050 -> 8050 -> 9550 (third trade, net -500)
        assert_eq!(metrics.current_equity, dec!(9550));
        assert_eq!(metrics.peak_equity, dec!(10050)); // Peak after second trade
        assert_eq!(metrics.current_drawdown, dec!(500));
        // Max drawdown was 2000 (when at 8050 during third trade)
        assert_eq!(metrics.max_drawdown, dec!(2000));
    }
}
