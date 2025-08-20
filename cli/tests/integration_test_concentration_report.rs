use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{Currency, DraftTrade, Environment, Status, TradeCategory, TradingVehicleCategory};
use rust_decimal_macros::dec;
use std::fs;
use std::path::Path;

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
fn test_concentration_report_with_trades() {
    let database_url = "file:test_concentration_report.db";
    let _cleanup = TestDatabaseCleanup::new(database_url);

    // Setup
    let database_factory = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(
        Box::new(database_factory),
        Box::new(alpaca_broker::AlpacaBroker),
    );

    // Create account
    let account = trust
        .create_account(
            "Test Account",
            "Test Description",
            Environment::Paper,
            dec!(0.3),
            dec!(0.1),
        )
        .unwrap();

    // Add funds to the account
    let (_transaction, _balance) = trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(10000),
            &Currency::USD,
        )
        .unwrap();

    // Create trading vehicle
    let vehicle = trust
        .create_trading_vehicle(
            "AAPL",
            "US0378331005",
            &TradingVehicleCategory::Stock,
            "alpaca",
        )
        .unwrap();

    // Create trades with different sectors
    let draft_trade = DraftTrade {
        account: account.clone(),
        trading_vehicle: vehicle.clone(),
        quantity: 10,
        category: TradeCategory::Long,
        currency: Currency::USD,
        sector: Some("Technology".to_string()),
        asset_class: Some("Stocks".to_string()),
        thesis: Some("Test trade".to_string()),
        context: None,
    };

    let trade1 = trust
        .create_trade(draft_trade, dec!(95), dec!(100), dec!(110))
        .unwrap();

    // Fund the trade
    let (_, _, _, _) = trust.fund_trade(&trade1).unwrap();

    // Fetch the funded trade
    let funded_trades = trust.search_trades(account.id, Status::Funded).unwrap();
    assert_eq!(funded_trades.len(), 1);
    let funded_trade = &funded_trades[0];

    // Create another trade in Healthcare
    let draft_trade2 = DraftTrade {
        account: account.clone(),
        trading_vehicle: vehicle.clone(),
        quantity: 10,
        category: TradeCategory::Long,
        currency: Currency::USD,
        sector: Some("Healthcare".to_string()),
        asset_class: Some("Stocks".to_string()),
        thesis: Some("Test trade 2".to_string()),
        context: None,
    };

    let trade2 = trust
        .create_trade(draft_trade2, dec!(45), dec!(50), dec!(60))
        .unwrap();

    // This test just verifies the trades were created successfully
    // The actual CLI command will be tested once implemented
    assert_eq!(trade1.sector, Some("Technology".to_string()));
    assert_eq!(trade2.sector, Some("Healthcare".to_string()));
    // After funding, the trade should have Status::Funded
    assert_eq!(funded_trade.status, Status::Funded);
}

#[test]
fn test_concentration_report_no_trades() {
    let database_url = "file:test_concentration_no_trades.db";
    let _cleanup = TestDatabaseCleanup::new(database_url);

    // Setup
    let database_factory = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(
        Box::new(database_factory),
        Box::new(alpaca_broker::AlpacaBroker),
    );

    // Create account
    let account = trust
        .create_account(
            "Test Account",
            "Test Description",
            Environment::Paper,
            dec!(0.3),
            dec!(0.1),
        )
        .unwrap();

    // Get all trades (should be empty)
    let trades = trust.search_trades(account.id, Status::Filled).unwrap();

    assert_eq!(trades.len(), 0);
}
