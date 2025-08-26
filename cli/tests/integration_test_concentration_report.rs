use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{Currency, Environment, TradingVehicleCategory};
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
fn test_concentration_calculation_with_mixed_portfolio() {
    let database_url = format!("file:test_concentration_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // Given: An account with mixed portfolio
    let account = trust
        .create_account(
            "Test Account",
            "Test",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
        )
        .expect("Failed to create account");

    // Create trading vehicles
    let stock1 = trust
        .create_trading_vehicle(
            "AAPL",
            "US0378331005",
            &TradingVehicleCategory::Stock,
            "NASDAQ",
        )
        .expect("Failed to create trading vehicle");

    let stock2 = trust
        .create_trading_vehicle(
            "TSLA",
            "US88160R1014",
            &TradingVehicleCategory::Stock,
            "NASDAQ",
        )
        .expect("Failed to create trading vehicle");

    let crypto = trust
        .create_trading_vehicle("BTC", "BTC001", &TradingVehicleCategory::Crypto, "Coinbase")
        .expect("Failed to create trading vehicle");

    // Add capital to account
    let _ = trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(10000.0),
            &Currency::USD,
        )
        .expect("Failed to create transaction");

    // When: Calculating concentration
    let result = trust.calculate_portfolio_concentration(Some(account.id));

    // Then: Should get concentration data
    assert!(result.is_ok(), "Concentration calculation should succeed");
}

#[test]
fn test_concentration_with_empty_portfolio() {
    let database_url = format!(
        "file:test_concentration_empty_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // Given: An account with no positions
    let account = trust
        .create_account(
            "Empty Account",
            "Test",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
        )
        .expect("Failed to create account");

    // When: Calculating concentration
    let result = trust.calculate_portfolio_concentration(Some(account.id));

    // Then: Should handle empty portfolio gracefully
    assert!(
        result.is_ok(),
        "Empty portfolio concentration should succeed"
    );
}

#[test]
fn test_concentration_with_invalid_account() {
    let database_url = format!(
        "file:test_concentration_invalid_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // When: Calculating concentration for non-existent account
    let fake_account_id = Uuid::new_v4();
    let result = trust.calculate_portfolio_concentration(Some(fake_account_id));

    // Then: Should handle invalid account appropriately
    assert!(
        result.is_ok(),
        "Invalid account should be handled gracefully"
    );
}
