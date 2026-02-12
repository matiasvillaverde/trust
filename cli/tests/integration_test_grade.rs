use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::DatabaseFactory;
use model::{
    Currency, DraftTrade, Environment, OrderStatus, Status, TradeCategory, TradingVehicleCategory,
};
use rust_decimal_macros::dec;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

fn cli_bin_path() -> String {
    std::env::var("CARGO_BIN_EXE_cli").unwrap_or_else(|_| {
        let candidate = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("target")
            .join("debug")
            .join("cli");
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
        .args(args)
        .output()
        .expect("run cli")
}

fn parse_stdout_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON")
}

fn seed_closed_trade_with_grade(database_url: &str) -> (Uuid, Uuid) {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    let account = trust
        .create_account(
            "Grade Test Account",
            "grade test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");

    let _ = trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(50000),
            &Currency::USD,
        )
        .expect("deposit funds");

    let vehicle = trust
        .create_trading_vehicle(
            "AAPL",
            Some("US0378331005"),
            &TradingVehicleCategory::Stock,
            "alpaca",
        )
        .expect("create trading vehicle");

    let trade = trust
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity: 10,
                category: TradeCategory::Long,
                currency: Currency::USD,
                thesis: Some("test thesis".to_string()),
                sector: Some("Technology".to_string()),
                asset_class: Some("Stocks".to_string()),
                context: Some("test context".to_string()),
            },
            dec!(190),
            dec!(200),
            dec!(220),
        )
        .expect("create trade");

    let _ = trust.fund_trade(&trade).expect("fund trade");
    let funded = trust
        .search_trades(account.id, Status::Funded)
        .expect("search funded")
        .first()
        .cloned()
        .expect("funded trade exists");

    // Manual fill: persist average_filled_price so the accounting logic can compute totals.
    {
        let db = SqliteDatabase::new(database_url);
        let mut latest = db.trade_read().read_trade(funded.id).expect("read trade");

        let mut entry = latest.entry.clone();
        entry.average_filled_price = Some(entry.unit_price);
        entry.filled_quantity = entry.quantity;
        entry.status = OrderStatus::Filled;
        db.order_write().update(&entry).expect("update entry");

        let mut target = latest.target.clone();
        target.average_filled_price = Some(target.unit_price);
        target.filled_quantity = target.quantity;
        target.status = OrderStatus::Filled;
        db.order_write().update(&target).expect("update target");

        // Refresh local snapshot for the next step.
        latest = db.trade_read().read_trade(funded.id).expect("read trade");
        assert!(latest.entry.average_filled_price.is_some());
        assert!(latest.target.average_filled_price.is_some());
    }

    let funded = trust
        .search_trades(account.id, Status::Funded)
        .expect("search funded after fill prep")
        .first()
        .cloned()
        .expect("funded trade exists");

    let _ = trust.fill_trade(&funded, dec!(0)).expect("fill trade");
    let filled = trust
        .search_trades(account.id, Status::Filled)
        .expect("search filled")
        .first()
        .cloned()
        .expect("filled trade exists");

    let _ = trust
        .target_acquired(&filled, dec!(0))
        .expect("close target");

    let closed = trust
        .search_trades(account.id, Status::ClosedTarget)
        .expect("search closed")
        .first()
        .cloned()
        .expect("closed trade exists");

    let _ = trust
        .grade_trade(
            closed.id,
            core::services::grading::GradingWeightsPermille::default(),
        )
        .expect("grade trade");

    (account.id, closed.id)
}

fn seed_canceled_trade_without_exit_fill(database_url: &str) -> Uuid {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    let account = trust
        .create_account(
            "Grade Canceled Account",
            "grade canceled test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");

    let _ = trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(50000),
            &Currency::USD,
        )
        .expect("deposit funds");

    let vehicle = trust
        .create_trading_vehicle(
            "MSFT",
            Some("US5949181045"),
            &TradingVehicleCategory::Stock,
            "alpaca",
        )
        .expect("create trading vehicle");

    let trade = trust
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity: 10,
                category: TradeCategory::Long,
                currency: Currency::USD,
                thesis: Some("cancel test".to_string()),
                sector: Some("Technology".to_string()),
                asset_class: Some("Stocks".to_string()),
                context: Some("cancel without exit fill".to_string()),
            },
            dec!(190),
            dec!(200),
            dec!(220),
        )
        .expect("create trade");

    let _ = trust.fund_trade(&trade).expect("fund trade");
    let funded = trust
        .search_trades(account.id, Status::Funded)
        .expect("search funded")
        .first()
        .cloned()
        .expect("funded trade exists");
    let _ = trust
        .cancel_funded_trade(&funded)
        .expect("cancel funded trade");

    trust
        .search_trades(account.id, Status::Canceled)
        .expect("search canceled")
        .first()
        .expect("canceled trade exists")
        .id
}

#[test]
fn test_grade_show_json() {
    let database_url = format!("file:/tmp/trust-grade-show-{}.db", Uuid::new_v4());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let (_account_id, trade_id) = seed_closed_trade_with_grade(&database_url);

    let output = run_cli(
        &database_url,
        &["grade", "show", &trade_id.to_string(), "--format", "json"],
    );
    assert!(output.status.success(), "grade show must succeed");
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["report"], "grade_show");
    assert_eq!(payload["data"]["trade_id"], trade_id.to_string());
    assert!(payload["data"]["overall"]["score"].is_number());
    assert!(payload["data"]["overall"]["grade"].is_string());
    assert!(payload["data"]["recommendations"].is_array());
}

#[test]
fn test_grade_summary_json() {
    let database_url = format!("file:/tmp/trust-grade-summary-{}.db", Uuid::new_v4());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let (account_id, _trade_id) = seed_closed_trade_with_grade(&database_url);

    let output = run_cli(
        &database_url,
        &[
            "grade",
            "summary",
            "--account",
            &account_id.to_string(),
            "--days",
            "90",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "grade summary must succeed");
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["report"], "grade_summary");
    assert_eq!(payload["filters"]["days"], 90);
    assert_eq!(payload["scope"]["account_id"], account_id.to_string());
    assert!(payload["data"]["trade_count"].as_u64().unwrap_or(0) >= 1);
    assert!(payload["data"]["average_overall_score"].is_string());
    assert!(payload["data"]["distribution"].is_object());
}

#[test]
fn test_grade_show_without_weights_reads_existing_custom_weight_grade() {
    let database_url = format!("file:/tmp/trust-grade-custom-weights-{}.db", Uuid::new_v4());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let (_account_id, trade_id) = seed_closed_trade_with_grade(&database_url);

    // Regrade with non-default weights.
    let regrade_output = run_cli(
        &database_url,
        &[
            "grade",
            "show",
            &trade_id.to_string(),
            "--format",
            "json",
            "--regrade",
            "--weights",
            "50,20,20,10",
        ],
    );
    assert!(
        regrade_output.status.success(),
        "regrading with custom weights must succeed"
    );

    // Omitting --weights should read existing grade regardless of stored custom weights.
    let show_output = run_cli(
        &database_url,
        &["grade", "show", &trade_id.to_string(), "--format", "json"],
    );
    assert!(show_output.status.success(), "grade show must succeed");
    let payload = parse_stdout_json(&show_output);
    assert_eq!(payload["data"]["weights_permille"]["process"], 500);
    assert_eq!(payload["data"]["weights_permille"]["risk"], 200);
    assert_eq!(payload["data"]["weights_permille"]["execution"], 200);
    assert_eq!(payload["data"]["weights_permille"]["documentation"], 100);
}

#[test]
fn test_grade_show_fails_for_canceled_trade_without_real_exit_fill() {
    let database_url = format!(
        "file:/tmp/trust-grade-canceled-unfilled-{}.db",
        Uuid::new_v4()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let trade_id = seed_canceled_trade_without_exit_fill(&database_url);

    let output = run_cli(
        &database_url,
        &["grade", "show", &trade_id.to_string(), "--format", "json"],
    );
    assert!(
        !output.status.success(),
        "grade show should fail for canceled trade without exit fill"
    );
    let err: Value = serde_json::from_slice(&output.stderr).expect("valid error JSON");
    assert_eq!(err["error"]["code"], "grade_compute_failed");
    let message = err["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("is not closed"));
}
