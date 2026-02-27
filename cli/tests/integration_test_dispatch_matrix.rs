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

fn seed_account_with_open_trade(database_url: &str) -> Uuid {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    let account = trust
        .create_account(
            "dispatch-matrix",
            "integration",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
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
        .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
        .expect("create vehicle");

    let trade = trust
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity: 5,
                category: TradeCategory::Long,
                currency: Currency::USD,
                thesis: Some("matrix".to_string()),
                sector: Some("Technology".to_string()),
                asset_class: Some("Stocks".to_string()),
                context: None,
            },
            dec!(190),
            dec!(200),
            dec!(220),
        )
        .expect("create trade");
    let _ = trust.fund_trade(&trade).expect("fund trade");

    account.id
}

#[test]
fn test_dispatch_matrix_reports_levels_advisor_and_keys() {
    let database_url = format!("file:test_dispatch_matrix_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_open_trade(&database_url);
    let account = account_id.to_string();

    let report_commands: Vec<Vec<&str>> = vec![
        vec![
            "report",
            "summary",
            "--format",
            "json",
            "--account",
            account.as_str(),
        ],
        vec![
            "report",
            "risk",
            "--format",
            "json",
            "--account",
            account.as_str(),
        ],
        vec![
            "report",
            "concentration",
            "--format",
            "json",
            "--account",
            account.as_str(),
        ],
        vec![
            "report",
            "drawdown",
            "--format",
            "json",
            "--account",
            account.as_str(),
        ],
        vec![
            "report",
            "performance",
            "--format",
            "json",
            "--account",
            account.as_str(),
        ],
        vec![
            "report",
            "metrics",
            "--format",
            "json",
            "--account",
            account.as_str(),
        ],
        vec![
            "level",
            "status",
            "--format",
            "json",
            "--account",
            account.as_str(),
        ],
        vec!["level", "triggers", "--format", "json"],
        vec![
            "level",
            "progress",
            "--format",
            "json",
            "--account",
            account.as_str(),
            "--profitable-trades",
            "3",
            "--win-rate",
            "55",
            "--monthly-loss=-1",
            "--largest-loss=-0.5",
            "--consecutive-wins",
            "1",
        ],
        vec![
            "level",
            "history",
            "--format",
            "json",
            "--account",
            account.as_str(),
            "--days",
            "30",
        ],
        vec![
            "advisor",
            "configure",
            "--account",
            account.as_str(),
            "--sector-limit",
            "30",
            "--asset-class-limit",
            "40",
            "--single-position-limit",
            "20",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
        vec![
            "advisor",
            "check",
            "--account",
            account.as_str(),
            "--symbol",
            "AAPL",
            "--entry",
            "200",
            "--quantity",
            "1",
            "--sector",
            "Technology",
            "--asset-class",
            "Stocks",
        ],
        vec!["advisor", "status", "--account", account.as_str()],
        vec![
            "advisor",
            "history",
            "--account",
            account.as_str(),
            "--days",
            "7",
        ],
        vec!["keys", "protected-show"],
        vec![
            "keys",
            "protected-set",
            "--value",
            "matrix-secret",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
        vec!["keys", "protected-show"],
        vec![
            "keys",
            "protected-delete",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    ];

    for args in report_commands {
        let output = run_cli(&database_url, &args);
        assert!(
            output.status.success(),
            "command should succeed: {:?}\nstderr={}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn test_dispatch_matrix_market_data_and_trade_size_preview_paths() {
    let database_url = format!(
        "file:test_dispatch_matrix_md_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_open_trade(&database_url);
    let account = account_id.to_string();

    let preview = run_cli(
        &database_url,
        &[
            "trade",
            "size-preview",
            "--account",
            account.as_str(),
            "--entry",
            "200",
            "--stop",
            "190",
            "--currency",
            "usd",
            "--format",
            "json",
        ],
    );
    assert!(
        preview.status.success(),
        "trade size-preview should succeed: {}",
        String::from_utf8_lossy(&preview.stderr)
    );

    let snapshot = run_cli(
        &database_url,
        &[
            "market-data",
            "snapshot",
            "--account",
            account.as_str(),
            "--symbol",
            "AAPL",
            "--format",
            "json",
        ],
    );
    assert!(
        !snapshot.status.success(),
        "market-data snapshot should fail without configured Alpaca keys"
    );

    let bars = run_cli(
        &database_url,
        &[
            "market-data",
            "bars",
            "--account",
            account.as_str(),
            "--symbol",
            "AAPL",
            "--timeframe",
            "1d",
            "--start",
            "2026-02-20T00:00:00Z",
            "--end",
            "2026-02-19T00:00:00Z",
            "--format",
            "json",
        ],
    );
    assert!(
        !bars.status.success(),
        "market-data bars should fail for invalid time range"
    );

    let stream = run_cli(
        &database_url,
        &[
            "market-data",
            "stream",
            "--account",
            account.as_str(),
            "--symbols",
            "AAPL",
            "--channels",
            "foo",
            "--format",
            "json",
        ],
    );
    assert!(
        !stream.status.success(),
        "market-data stream should fail for invalid channels"
    );
}
