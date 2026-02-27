use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{Currency, DraftTrade, Environment, TradeCategory, TradingVehicleCategory};
use rust_decimal_macros::dec;
use std::fs;
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

const PROTECTED_KEYWORD: &str = "I_UNDERSTAND_RISK";

fn cli_bin_path() -> String {
    std::env::var("CARGO_BIN_EXE_trust").unwrap_or_else(|_| {
        let candidate = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("target")
            .join("debug")
            .join("trust");
        candidate.to_string_lossy().to_string()
    })
}

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

fn run_cli(database_url: &str, args: &[&str]) -> std::process::Output {
    Command::new(cli_bin_path())
        .env("TRUST_DB_URL", database_url)
        .env("TRUST_PROTECTED_KEYWORD_EXPECTED", PROTECTED_KEYWORD)
        .env("TRUST_DISABLE_KEYCHAIN", "1")
        .args(args)
        .output()
        .expect("run cli")
}

fn seed_account(database_url: &str, name: &str) -> Uuid {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));
    let account = trust
        .create_account(name, "integration", Environment::Paper, dec!(20), dec!(10))
        .expect("create account");
    account.id
}

fn seed_trade(database_url: &str, name: &str, symbol: &str, funded: bool) -> (Uuid, Uuid) {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));
    let account = trust
        .create_account(name, "integration", Environment::Paper, dec!(20), dec!(10))
        .expect("create account");
    trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(10_000),
            &Currency::USD,
        )
        .expect("deposit");
    let vehicle = trust
        .create_trading_vehicle(symbol, None, &TradingVehicleCategory::Stock, "alpaca")
        .expect("create vehicle");
    let trade = trust
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity: 5,
                category: TradeCategory::Long,
                currency: Currency::USD,
                thesis: Some("dispatcher-commands".to_string()),
                sector: Some("Technology".to_string()),
                asset_class: Some("Stocks".to_string()),
                context: None,
            },
            dec!(190),
            dec!(200),
            dec!(220),
        )
        .expect("create trade");
    let trade = if funded {
        trust.fund_trade(&trade).expect("fund trade").0
    } else {
        trade
    };
    (account.id, trade.id)
}

#[test]
fn test_transaction_non_interactive_cli_round_trip() {
    let database_url = format!("file:test_tx_cli_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account(&database_url, "tx-cli");

    let deposit = run_cli(
        &database_url,
        &[
            "transaction",
            "deposit",
            "--account",
            &account_id.to_string(),
            "--currency",
            "USD",
            "--amount",
            "250.75",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );
    assert!(deposit.status.success(), "deposit should succeed");

    let withdraw = run_cli(
        &database_url,
        &[
            "transaction",
            "withdraw",
            "--account",
            &account_id.to_string(),
            "--currency",
            "USD",
            "--amount",
            "50.25",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );
    assert!(withdraw.status.success(), "withdraw should succeed");

    let database = SqliteDatabase::new(&database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));
    let balance = trust
        .search_balance(account_id, &Currency::USD)
        .expect("USD balance should exist");
    assert_eq!(balance.total_balance, dec!(200.50));
}

#[test]
fn test_db_export_and_import_error_paths_cli() {
    let database_url = format!("file:test_db_cli_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let export_path = format!("/tmp/trust-export-{}.json", Uuid::new_v4().simple());

    let export = run_cli(&database_url, &["db", "export", "--output", &export_path]);
    assert!(export.status.success(), "db export should succeed");
    assert!(
        Path::new(&export_path).exists(),
        "export output should exist"
    );

    let import = run_cli(
        &database_url,
        &[
            "db",
            "import",
            "--input",
            "/tmp/does-not-exist-trust-import.json",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );
    assert!(
        !import.status.success(),
        "db import with missing file should fail"
    );

    let stderr_text = String::from_utf8(import.stderr).expect("stderr utf-8");
    assert!(
        stderr_text.contains("db_import_failed"),
        "structured db import error code should be present in stderr: {stderr_text}"
    );

    let _ = fs::remove_file(export_path);
}

#[test]
fn test_trade_fund_cancel_submit_and_sync_dispatch_paths() {
    let database_url = format!("file:test_trade_dispatch_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let (_new_account, new_trade_id) = seed_trade(&database_url, "trade-new", "AAPL", false);
    let (_funded_account, funded_trade_id) =
        seed_trade(&database_url, "trade-funded", "MSFT", true);

    let fund = run_cli(
        &database_url,
        &["trade", "fund", "--trade-id", &new_trade_id.to_string()],
    );
    assert!(
        fund.status.success(),
        "trade fund should succeed: {}",
        String::from_utf8_lossy(&fund.stderr)
    );

    let cancel = run_cli(
        &database_url,
        &[
            "trade",
            "cancel",
            "--trade-id",
            &funded_trade_id.to_string(),
        ],
    );
    assert!(
        cancel.status.success(),
        "trade cancel should succeed: {}",
        String::from_utf8_lossy(&cancel.stderr)
    );

    let submit = run_cli(
        &database_url,
        &["trade", "submit", "--trade-id", &new_trade_id.to_string()],
    );
    assert!(
        !submit.status.success(),
        "trade submit should fail without broker credentials"
    );
    let submit_stderr = String::from_utf8(submit.stderr).expect("submit stderr utf-8");
    assert!(
        submit_stderr.contains("trade_not_found")
            || submit_stderr.contains("trade_submit_failed")
            || submit_stderr.contains("No API keys found"),
        "submit failure should be structured or broker-related: {submit_stderr}"
    );

    let sync = run_cli(
        &database_url,
        &["trade", "sync", "--trade-id", &funded_trade_id.to_string()],
    );
    assert!(
        !sync.status.success(),
        "trade sync should fail for non-syncable status"
    );
    let sync_stderr = String::from_utf8(sync.stderr).expect("sync stderr utf-8");
    assert!(
        sync_stderr.contains("trade_not_found") || sync_stderr.contains("trade_sync_failed"),
        "sync failure should be structured: {sync_stderr}"
    );
}
