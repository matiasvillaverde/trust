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
    use db_sqlite::SqliteDatabase;
    use model::{
        Account, Currency, DraftTrade, Environment, Order, OrderAction, OrderCategory,
        TradeCategory, TradingVehicle, TradingVehicleCategory,
    };

    fn create_test_account(database: &mut SqliteDatabase, name: &str) -> Account {
        database
            .account_write()
            .create(name, name, Environment::Paper, dec!(0), dec!(0))
            .expect("account should be created")
    }

    fn create_test_vehicle(database: &mut SqliteDatabase, symbol: &str) -> TradingVehicle {
        database
            .trading_vehicle_write()
            .create_trading_vehicle(
                symbol,
                Some(symbol),
                &TradingVehicleCategory::Stock,
                "alpaca",
            )
            .expect("trading vehicle should be created")
    }

    fn create_test_order(
        database: &mut SqliteDatabase,
        vehicle: &TradingVehicle,
        action: OrderAction,
        category: OrderCategory,
        price: Decimal,
    ) -> Order {
        database
            .order_write()
            .create(vehicle, 10, price, &Currency::USD, &action, &category)
            .expect("order should be created")
    }

    fn create_test_trade(
        database: &mut SqliteDatabase,
        account: &Account,
        symbol: &str,
        status: Status,
    ) -> Trade {
        let vehicle = create_test_vehicle(database, symbol);
        let stop = create_test_order(
            database,
            &vehicle,
            OrderAction::Sell,
            OrderCategory::Stop,
            dec!(90),
        );
        let entry = create_test_order(
            database,
            &vehicle,
            OrderAction::Buy,
            OrderCategory::Limit,
            dec!(100),
        );
        let target = create_test_order(
            database,
            &vehicle,
            OrderAction::Sell,
            OrderCategory::Limit,
            dec!(120),
        );
        let draft = DraftTrade {
            account: account.clone(),
            trading_vehicle: vehicle,
            quantity: 10,
            currency: Currency::USD,
            category: TradeCategory::Long,
            thesis: None,
            sector: None,
            asset_class: None,
            context: None,
        };
        let trade = database
            .trade_write()
            .create_trade(draft, &stop, &entry, &target)
            .expect("trade should be created");

        database
            .trade_write()
            .update_trade_status(status, &trade)
            .expect("trade status should be updated")
    }

    fn create_trade_transaction(
        database: &mut SqliteDatabase,
        account: &Account,
        amount: Decimal,
        category: TransactionCategory,
    ) -> model::Transaction {
        database
            .transaction_write()
            .create_transaction(account, amount, &Currency::USD, category)
            .expect("transaction should be created")
    }

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

    #[test]
    fn test_calculate_total_capital_at_risk_reports_addition_overflow() {
        let funded_date = Utc::now().naive_utc();
        let positions = vec![
            OpenPosition {
                trade_id: Uuid::new_v4(),
                symbol: "MAX1".to_string(),
                capital_amount: Decimal::MAX,
                status: Status::Funded,
                funded_date,
            },
            OpenPosition {
                trade_id: Uuid::new_v4(),
                symbol: "MAX2".to_string(),
                capital_amount: Decimal::ONE,
                status: Status::Funded,
                funded_date,
            },
        ];

        let error = CapitalAtRiskCalculator::calculate_total_capital_at_risk(&positions)
            .expect_err("portfolio risk summation overflow should be explicit");

        assert_eq!(
            error.to_string(),
            "Arithmetic overflow calculating total capital at risk"
        );
    }

    #[test]
    fn test_calculate_open_positions_includes_funded_trade_with_funding_date() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "risk-open-position");
        let trade = create_test_trade(&mut database, &account, "RISKOPEN", Status::Funded);
        let funding = create_trade_transaction(
            &mut database,
            &account,
            dec!(250),
            TransactionCategory::FundTrade(trade.id),
        );

        let positions =
            CapitalAtRiskCalculator::calculate_open_positions(Some(account.id), &mut database)
                .expect("positions should calculate");

        assert_eq!(positions.len(), 1);
        assert_eq!(
            positions.first().expect("position should exist"),
            &OpenPosition {
                trade_id: trade.id,
                symbol: "RISKOPEN".to_string(),
                capital_amount: dec!(250),
                status: Status::Funded,
                funded_date: funding.created_at,
            }
        );
    }

    #[test]
    fn test_closing_transaction_excludes_trade_from_open_positions() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "risk-closed-position");
        let trade = create_test_trade(&mut database, &account, "RISKCLOSED", Status::Filled);
        create_trade_transaction(
            &mut database,
            &account,
            dec!(250),
            TransactionCategory::FundTrade(trade.id),
        );
        create_trade_transaction(
            &mut database,
            &account,
            dec!(300),
            TransactionCategory::CloseTarget(trade.id),
        );

        let is_open = CapitalAtRiskCalculator::is_trade_open(&trade, &mut database)
            .expect("open state should calculate");
        let positions =
            CapitalAtRiskCalculator::calculate_open_positions(Some(account.id), &mut database)
                .expect("positions should calculate");

        assert!(!is_open);
        assert!(positions.is_empty());
    }

    #[test]
    fn test_funding_date_falls_back_to_trade_created_at_without_funding_transaction() {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&mut database, "risk-funding-fallback");
        let trade = create_test_trade(&mut database, &account, "RISKFALLBACK", Status::Submitted);

        let funding_date = CapitalAtRiskCalculator::get_trade_funding_date(&trade, &mut database)
            .expect("funding date should calculate");

        assert_eq!(funding_date, trade.created_at);
    }

    #[test]
    fn test_calculate_open_positions_without_account_scans_all_accounts() {
        let mut database = SqliteDatabase::new_in_memory();
        let first_account = create_test_account(&mut database, "risk-all-accounts-1");
        let second_account = create_test_account(&mut database, "risk-all-accounts-2");
        let first_trade =
            create_test_trade(&mut database, &first_account, "RISKALL1", Status::Funded);
        let second_trade = create_test_trade(
            &mut database,
            &second_account,
            "RISKALL2",
            Status::Submitted,
        );
        create_trade_transaction(
            &mut database,
            &first_account,
            dec!(100),
            TransactionCategory::FundTrade(first_trade.id),
        );
        create_trade_transaction(
            &mut database,
            &second_account,
            dec!(150),
            TransactionCategory::FundTrade(second_trade.id),
        );

        let positions = CapitalAtRiskCalculator::calculate_open_positions(None, &mut database)
            .expect("positions should calculate across accounts");

        assert_eq!(positions.len(), 2);
        assert!(positions.iter().any(|position| {
            position.trade_id == first_trade.id
                && position.symbol == "RISKALL1"
                && position.capital_amount == dec!(100)
        }));
        assert!(positions.iter().any(|position| {
            position.trade_id == second_trade.id
                && position.symbol == "RISKALL2"
                && position.capital_amount == dec!(150)
        }));
    }
}
