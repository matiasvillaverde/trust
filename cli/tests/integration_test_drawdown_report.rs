use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{Currency, Environment, TransactionCategory};
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
fn test_drawdown_report_empty_history() {
    let database_url = format!("file:drawdown_test_empty_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // Create account but no transactions
    let account = trust
        .create_account(
            "Test Account",
            "Drawdown test account",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
        )
        .unwrap();

    // Get transactions (should be empty)
    let transactions = trust.get_account_transactions(account.id).unwrap();
    assert!(transactions.is_empty());

    // Calculate drawdown metrics
    use core::calculators_drawdown::RealizedDrawdownCalculator;
    let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions).unwrap();
    let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap();

    assert_eq!(metrics.current_equity, dec!(0));
    assert_eq!(metrics.max_drawdown, dec!(0));
}

#[test]
fn test_drawdown_report_with_deposits_only() {
    let database_url = format!("file:drawdown_test_deposits_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // Create account and add deposits
    let account = trust
        .create_account(
            "Test Account",
            "Drawdown test account",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
        )
        .unwrap();

    // Make deposits
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(10000),
            &Currency::USD,
        )
        .unwrap();

    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(5000),
            &Currency::USD,
        )
        .unwrap();

    // Get transactions
    let transactions = trust.get_account_transactions(account.id).unwrap();
    assert_eq!(transactions.len(), 2);

    // Calculate drawdown metrics
    use core::calculators_drawdown::RealizedDrawdownCalculator;
    let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions).unwrap();
    let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap();

    // Should have 15000 total with no drawdown
    assert_eq!(metrics.current_equity, dec!(15000));
    assert_eq!(metrics.peak_equity, dec!(15000));
    assert_eq!(metrics.current_drawdown, dec!(0));
    assert_eq!(metrics.max_drawdown, dec!(0));
}

#[test]
fn test_drawdown_report_with_withdrawal() {
    let database_url = format!(
        "file:drawdown_test_withdrawal_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // Create account
    let account = trust
        .create_account(
            "Test Account",
            "Drawdown test account",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
        )
        .unwrap();

    // Deposit then withdraw
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(10000),
            &Currency::USD,
        )
        .unwrap();

    trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(3000),
            &Currency::USD,
        )
        .unwrap();

    // Get transactions
    let transactions = trust.get_account_transactions(account.id).unwrap();

    // Calculate drawdown metrics
    use core::calculators_drawdown::RealizedDrawdownCalculator;
    let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions).unwrap();
    let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap();

    // Should show drawdown from withdrawal
    assert_eq!(metrics.current_equity, dec!(7000));
    assert_eq!(metrics.peak_equity, dec!(10000));
    assert_eq!(metrics.current_drawdown, dec!(3000));
    assert_eq!(metrics.current_drawdown_percentage, dec!(30));
}

#[test]
fn test_drawdown_report_multiple_accounts() {
    let database_url = format!("file:drawdown_test_multi_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    // Create two accounts
    let account1 = trust
        .create_account(
            "Account 1",
            "First account",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
        )
        .unwrap();

    let account2 = trust
        .create_account(
            "Account 2",
            "Second account",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
        )
        .unwrap();

    // Add transactions to both accounts
    trust
        .create_transaction(
            &account1,
            &TransactionCategory::Deposit,
            dec!(10000),
            &Currency::USD,
        )
        .unwrap();

    trust
        .create_transaction(
            &account2,
            &TransactionCategory::Deposit,
            dec!(5000),
            &Currency::USD,
        )
        .unwrap();

    // Get all transactions
    let all_transactions = trust.get_all_transactions().unwrap();
    assert_eq!(all_transactions.len(), 2);

    // Calculate combined drawdown metrics
    use core::calculators_drawdown::RealizedDrawdownCalculator;
    let curve = RealizedDrawdownCalculator::calculate_equity_curve(&all_transactions).unwrap();
    let metrics = RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap();

    // Combined equity should be 15000
    assert_eq!(metrics.current_equity, dec!(15000));
    assert_eq!(metrics.max_drawdown, dec!(0));
}
