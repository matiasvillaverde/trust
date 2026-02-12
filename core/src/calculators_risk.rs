//! Calculator for capital at risk metrics
//!
//! This module provides functionality to calculate the capital at risk
//! from open trading positions.

use chrono::NaiveDateTime;
use model::{DatabaseFactory, Status, Trade, TransactionCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

/// Represents an open trading position with its risk exposure
#[derive(Debug, Clone, PartialEq)]
pub struct OpenPosition {
    /// The unique identifier of the trade
    pub trade_id: Uuid,
    /// The trading symbol (e.g., "AAPL", "MSFT")
    pub symbol: String,
    /// The amount of capital at risk for this position
    pub capital_amount: Decimal,
    /// The current status of the trade
    pub status: Status,
    /// The date when the trade was funded
    pub funded_date: NaiveDateTime,
}

/// Calculator for capital at risk metrics
#[derive(Debug)]
pub struct CapitalAtRiskCalculator;

impl CapitalAtRiskCalculator {
    /// Calculate all open positions for an account
    ///
    /// A position is considered "open" if it has funding/opening transactions
    /// but no closing transactions (CloseTarget, CloseSafetyStop, CloseSafetyStopSlippage)
    pub fn calculate_open_positions(
        account_id: Option<Uuid>,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Vec<OpenPosition>, Box<dyn std::error::Error>> {
        let mut open_positions = Vec::new();

        // Get trades that could be open (Funded, Submitted, Filled)
        let potential_open_statuses = [Status::Funded, Status::Submitted, Status::Filled];

        let trades = if let Some(account_id) = account_id {
            let mut all_trades = Vec::new();
            for status in potential_open_statuses {
                if let Ok(trades) = database
                    .trade_read()
                    .read_trades_with_status(account_id, status)
                {
                    all_trades.extend(trades);
                }
            }
            all_trades
        } else {
            let mut all_trades = Vec::new();
            let accounts = database.account_read().all()?;
            for account in accounts {
                for status in &[Status::Funded, Status::Submitted, Status::Filled] {
                    if let Ok(trades) = database
                        .trade_read()
                        .read_trades_with_status(account.id, *status)
                    {
                        all_trades.extend(trades);
                    }
                }
            }
            all_trades
        };

        // Check each trade for closing transactions
        for trade in trades {
            if Self::is_trade_open(&trade, database)? {
                let capital_amount = Self::calculate_trade_capital_at_risk(&trade, database)?;

                // Only include positions with actual capital at risk
                if capital_amount > dec!(0) {
                    // Get funding date from transactions
                    let funded_date = Self::get_trade_funding_date(&trade, database)?;

                    open_positions.push(OpenPosition {
                        trade_id: trade.id,
                        symbol: trade.trading_vehicle.symbol.clone(),
                        capital_amount,
                        status: trade.status,
                        funded_date,
                    });
                }
            }
        }

        Ok(open_positions)
    }

    /// Calculate the total capital at risk from all open positions
    pub fn calculate_total_capital_at_risk(
        positions: &[OpenPosition],
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let total = positions.iter().try_fold(dec!(0), |acc, pos| {
            acc.checked_add(pos.capital_amount)
                .ok_or("Arithmetic overflow calculating total capital at risk")
        })?;

        Ok(total)
    }

    /// Check if a trade is open (has no closing transactions)
    fn is_trade_open(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let transactions = database
            .transaction_read()
            .all_trade_transactions(trade.id)?;

        // Check for any closing transactions
        for tx in transactions {
            match tx.category {
                TransactionCategory::CloseTarget(_)
                | TransactionCategory::CloseSafetyStop(_)
                | TransactionCategory::CloseSafetyStopSlippage(_) => {
                    return Ok(false); // Trade is closed
                }
                _ => continue,
            }
        }

        Ok(true) // No closing transactions found
    }

    /// Calculate the capital at risk for a specific trade
    fn calculate_trade_capital_at_risk(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<Decimal, Box<dyn std::error::Error>> {
        let transactions = database
            .transaction_read()
            .all_trade_funding_transactions(trade.id)?;

        let total = transactions
            .iter()
            .filter(|tx| matches!(tx.category, TransactionCategory::FundTrade(_)))
            .try_fold(dec!(0), |acc, tx| {
                acc.checked_add(tx.amount)
                    .ok_or("Arithmetic overflow calculating trade capital")
            })?;

        Ok(total)
    }

    /// Get the funding date for a trade
    fn get_trade_funding_date(
        trade: &Trade,
        database: &mut dyn DatabaseFactory,
    ) -> Result<NaiveDateTime, Box<dyn std::error::Error>> {
        let transactions = database
            .transaction_read()
            .all_trade_funding_transactions(trade.id)?;

        // Find the first FundTrade transaction
        for tx in transactions {
            if matches!(tx.category, TransactionCategory::FundTrade(_)) {
                return Ok(tx.created_at);
            }
        }

        // If no funding transaction found, use trade creation date
        Ok(trade.created_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::Utc;

    #[test]
    fn test_calculate_total_capital_at_risk_empty() {
        let positions = vec![];
        let result = CapitalAtRiskCalculator::calculate_total_capital_at_risk(&positions);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), dec!(0));
    }

    #[test]
    fn test_calculate_total_capital_at_risk_single_position() {
        let positions = vec![OpenPosition {
            trade_id: Uuid::new_v4(),
            symbol: "AAPL".to_string(),
            capital_amount: dec!(1000),
            status: Status::Filled,
            funded_date: Utc::now().naive_utc(),
        }];

        let result = CapitalAtRiskCalculator::calculate_total_capital_at_risk(&positions);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), dec!(1000));
    }

    #[test]
    fn test_calculate_total_capital_at_risk_multiple_positions() {
        let positions = vec![
            OpenPosition {
                trade_id: Uuid::new_v4(),
                symbol: "AAPL".to_string(),
                capital_amount: dec!(1000),
                status: Status::Filled,
                funded_date: Utc::now().naive_utc(),
            },
            OpenPosition {
                trade_id: Uuid::new_v4(),
                symbol: "MSFT".to_string(),
                capital_amount: dec!(2500),
                status: Status::Submitted,
                funded_date: Utc::now().naive_utc(),
            },
            OpenPosition {
                trade_id: Uuid::new_v4(),
                symbol: "TSLA".to_string(),
                capital_amount: dec!(1500),
                status: Status::Funded,
                funded_date: Utc::now().naive_utc(),
            },
        ];

        let result = CapitalAtRiskCalculator::calculate_total_capital_at_risk(&positions);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), dec!(5000));
    }
}
