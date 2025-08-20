use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{Currency, Environment, TradeCategory, TradingVehicleCategory};
use rust_decimal_macros::dec;
use std::fs;
use std::path::Path;
use uuid::Uuid;

/// RAII helper to cleanup test databases
struct TestDatabaseCleanup {
    database_path: String,
}

impl TestDatabaseCleanup {
    fn new(database_url: &str) -> Self {
        Self {
            database_path: database_url.replace("file:", ""),
        }
    }
}

impl Drop for TestDatabaseCleanup {
    fn drop(&mut self) {
        if Path::new(&self.database_path).exists() {
            let _ = fs::remove_file(&self.database_path);
        }
    }
}

#[test]
fn test_database_cleanup_works() {
    let database_url = format!("file:test_cleanup_{}.db", Uuid::new_v4().simple());
    let cleanup_path = database_url.replace("file:", "");

    // Create database file to simulate test
    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // Create an account to ensure DB file exists
    let _account = trust
        .create_account(
            "Test Account",
            "Test",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
        )
        .unwrap();

    // File should exist now
    assert!(Path::new(&cleanup_path).exists());

    // Create cleanup helper
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    // File still exists during test
    assert!(Path::new(&cleanup_path).exists());

    // Drop the cleanup helper manually to test it works
    drop(_cleanup);

    // File should be cleaned up
    assert!(!Path::new(&cleanup_path).exists());
}

#[test]
fn test_performance_report_no_trades() {
    let database_url = format!("file:perf_test_no_trades_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // Create account
    let account = trust
        .create_account(
            "Test Account",
            "Performance test account",
            Environment::Paper,
            dec!(25.0), // 25% taxes
            dec!(10.0), // 10% earnings
        )
        .unwrap();

    // With no trades, there should be no closed trades
    let closed_target_trades = trust
        .search_trades(account.id, model::Status::ClosedTarget)
        .unwrap();
    let closed_stop_trades = trust
        .search_trades(account.id, model::Status::ClosedStopLoss)
        .unwrap();

    assert!(closed_target_trades.is_empty());
    assert!(closed_stop_trades.is_empty());
}

#[test]
fn test_performance_report_with_trades() {
    let database_url = format!("file:perf_test_with_trades_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // Create account
    let account = trust
        .create_account(
            "Test Account",
            "Performance test account",
            Environment::Paper,
            dec!(25.0), // 25% taxes
            dec!(10.0), // 10% earnings
        )
        .unwrap();

    // Fund the account
    let (_transaction, _balance) = trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(10000.0),
            &Currency::USD,
        )
        .unwrap();

    // Create trading vehicle
    let trading_vehicle = trust
        .create_trading_vehicle(
            "AAPL",
            "US0378331005",
            &TradingVehicleCategory::Stock,
            "TestBroker",
        )
        .unwrap();

    // Create a winning trade
    let draft_trade = model::DraftTrade {
        account: account.clone(),
        trading_vehicle: trading_vehicle.clone(),
        quantity: 10,
        category: TradeCategory::Long,
        currency: Currency::USD,
        thesis: Some("Test winning trade".to_string()),
        sector: Some("Technology".to_string()),
        asset_class: Some("Stocks".to_string()),
        context: Some("Test context".to_string()),
    };

    let winning_trade = trust
        .create_trade(
            draft_trade,
            dec!(95.0),  // stop price
            dec!(100.0), // entry price
            dec!(110.0), // target price
        )
        .unwrap();

    // Fund and simulate the trade lifecycle to ClosedTarget
    let (winning_trade, _tx, _acc_balance, _trade_balance) =
        trust.fund_trade(&winning_trade).unwrap();

    // Manually update trade status and balance for testing
    // In a real scenario, this would be done through broker interactions
    // For testing, we'll directly verify the calculator works with the trade data structure

    // Test the performance calculator directly with sample data
    use core::calculators_performance::PerformanceCalculator;
    use model::trade::Status;

    // Create test trades for the calculator
    let mut test_winning_trade = winning_trade.clone();
    test_winning_trade.status = Status::ClosedTarget;
    test_winning_trade.balance.total_performance = dec!(100.0); // $100 profit

    let mut test_losing_trade = winning_trade.clone();
    test_losing_trade.status = Status::ClosedStopLoss;
    test_losing_trade.balance.total_performance = dec!(-50.0); // $50 loss

    let test_trades = vec![test_winning_trade, test_losing_trade];
    let closed_trades = PerformanceCalculator::filter_closed_trades(&test_trades);

    assert_eq!(closed_trades.len(), 2);

    let stats = PerformanceCalculator::calculate_performance_stats(&closed_trades);
    assert_eq!(stats.total_trades, 2);
    assert_eq!(stats.winning_trades, 1);
    assert_eq!(stats.losing_trades, 1);
    assert_eq!(stats.win_rate, dec!(50));
    assert_eq!(stats.average_win, dec!(100.0));
    assert_eq!(stats.average_loss, dec!(-50.0));
    assert_eq!(stats.best_trade, Some(dec!(100.0)));
    assert_eq!(stats.worst_trade, Some(dec!(-50.0)));
}

#[test]
fn test_performance_calculator_r_multiple() {
    use core::calculators_performance::PerformanceCalculator;
    use model::trade::{Status, Trade};
    use rust_decimal_macros::dec;

    // Create a test trade that hit target
    let mut trade = Trade::default();
    trade.entry.unit_price = dec!(100.0);
    trade.safety_stop.unit_price = dec!(95.0);
    trade.target.unit_price = dec!(110.0);
    trade.status = Status::ClosedTarget;

    let closed_trades = vec![trade];
    let avg_r = PerformanceCalculator::calculate_average_r_multiple(&closed_trades);

    // R-Multiple = (110-100)/(100-95) = 10/5 = 2.0
    assert_eq!(avg_r, dec!(2));
}

#[test]
fn test_performance_calculator_edge_cases() {
    use core::calculators_performance::PerformanceCalculator;

    // Test empty trades
    let empty_trades = vec![];
    let stats = PerformanceCalculator::calculate_performance_stats(&empty_trades);
    assert_eq!(stats.total_trades, 0);
    assert_eq!(stats.win_rate, dec!(0));

    // Test filter with no closed trades
    let open_trade = model::trade::Trade {
        status: model::Status::Filled,
        ..Default::default()
    };
    let trades_with_open = vec![open_trade];
    let closed_only = PerformanceCalculator::filter_closed_trades(&trades_with_open);
    assert!(closed_only.is_empty());
}
