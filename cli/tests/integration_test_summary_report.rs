use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{Currency, Environment};
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
fn test_summary_with_complete_data() {
    let database_url = format!("file:test_summary_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // Given: An account with complete trading data
    let account = trust
        .create_account(
            "Summary Test Account",
            "Test",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
        )
        .expect("Failed to create account");

    // Add capital to account
    let _ = trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(50000.0),
            &Currency::USD,
        )
        .expect("Failed to create transaction");

    // When: Getting summary data (this should fail until we implement it)
    let result = trust.get_trading_summary(Some(account.id));

    // Then: Should get comprehensive summary data
    assert!(result.is_ok(), "Trading summary should succeed");

    let summary = result.unwrap();
    assert_eq!(summary.account_id, account.id);
    // More assertions would go here once we define the summary structure
}

#[test]
fn test_summary_with_empty_account() {
    let database_url = format!("file:test_summary_empty_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // Given: An empty account
    let account = trust
        .create_account(
            "Empty Account",
            "Test",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
        )
        .expect("Failed to create account");

    // When: Getting summary
    let result = trust.get_trading_summary(Some(account.id));

    // Then: Should handle empty account gracefully
    assert!(result.is_ok(), "Empty account summary should succeed");
}

#[test]
fn test_summary_with_invalid_account() {
    let database_url = format!("file:test_summary_invalid_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // When: Getting summary for non-existent account
    let fake_account_id = Uuid::new_v4();
    let result = trust.get_trading_summary(Some(fake_account_id));

    // Then: Should handle invalid account appropriately
    assert!(
        result.is_ok(),
        "Invalid account should be handled gracefully"
    );
}
