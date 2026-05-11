use chrono::NaiveDate;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    database::TradingVehicleUpsert, Currency, DraftTrade, Environment, FixedIncomeTerms,
    TradeCategory, TradingVehicleCategory,
};
use rust_decimal_macros::dec;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

const PROTECTED_KEYWORD: &str = "I_UNDERSTAND_RISK";

fn cli_bin_path() -> String {
    if let Ok(bin) = std::env::var("CARGO_BIN_EXE_trust") {
        return bin;
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir
            .join("..")
            .join("target")
            .join("debug")
            .join("trust"),
        manifest_dir
            .join("..")
            .join("target")
            .join("llvm-cov-target")
            .join("debug")
            .join("trust"),
    ];
    for candidate in &candidates {
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(debug_dir) = current_exe.parent().and_then(Path::parent) {
            let sibling_bin = debug_dir.join("trust");
            if sibling_bin.exists() {
                return sibling_bin.to_string_lossy().to_string();
            }
        }
    }

    candidates[0].to_string_lossy().to_string()
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

fn account_id_from_create_output(output: &std::process::Output) -> Uuid {
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("id: "))
        .and_then(|raw| Uuid::parse_str(raw).ok())
        .unwrap_or_else(|| panic!("created account id should be present in stdout: {stdout}"))
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
                quantity: 5.into(),
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

fn seed_multi_asset_vehicle_inventory(database_url: &str) {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));
    trust
        .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
        .expect("seed stock trading vehicle");
    trust
        .create_trading_vehicle("SPY", None, &TradingVehicleCategory::Etf, "ibkr")
        .expect("seed ETF trading vehicle");
    trust
        .upsert_trading_vehicle(TradingVehicleUpsert {
            symbol: "912810TL2".to_string(),
            isin: Some("US912810TL26".to_string()),
            category: TradingVehicleCategory::Bond,
            broker: "ibkr".to_string(),
            broker_asset_id: None,
            exchange: Some("SMART".to_string()),
            broker_asset_class: Some("bond".to_string()),
            broker_asset_status: Some("active".to_string()),
            tradable: Some(true),
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: Some(false),
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(4)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2030, 1, 15).unwrap()),
                coupon_frequency_per_year: Some(2),
            }),
        })
        .expect("seed complete bond trading vehicle");
    trust
        .upsert_trading_vehicle(TradingVehicleUpsert {
            symbol: "9128285M8".to_string(),
            isin: Some("US9128285M81".to_string()),
            category: TradingVehicleCategory::Bond,
            broker: "ibkr".to_string(),
            broker_asset_id: None,
            exchange: Some("SMART".to_string()),
            broker_asset_class: Some("bond".to_string()),
            broker_asset_status: Some("active".to_string()),
            tradable: Some(true),
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: Some(false),
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(6)),
                maturity_date: None,
                coupon_frequency_per_year: Some(2),
            }),
        })
        .expect("seed incomplete bond trading vehicle");
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
fn test_account_create_list_balance_and_transfer_cli_round_trip() {
    let database_url = format!("file:test_accounts_cli_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let empty_list = run_cli(&database_url, &["accounts", "list"]);
    assert!(
        empty_list.status.success(),
        "empty account list should succeed"
    );
    assert_eq!(
        String::from_utf8_lossy(&empty_list.stdout).trim(),
        "No accounts found."
    );

    let create_primary = run_cli(
        &database_url,
        &[
            "accounts",
            "create",
            "--name",
            "Primary Account",
            "--description",
            "Main trading account",
            "--environment",
            "paper",
            "--taxes",
            "25",
            "--earnings",
            "10",
            "--type",
            "primary",
            "--broker",
            "alpaca",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );
    assert!(
        create_primary.status.success(),
        "primary account create should succeed: {}",
        String::from_utf8_lossy(&create_primary.stderr)
    );
    let primary_id = account_id_from_create_output(&create_primary);

    let create_tax_reserve = run_cli(
        &database_url,
        &[
            "accounts",
            "create",
            "--name",
            "Tax Reserve",
            "--description",
            "Reserved cash for taxes",
            "--environment",
            "paper",
            "--taxes",
            "0",
            "--earnings",
            "0",
            "--type",
            "tax-reserve",
            "--broker",
            "alpaca",
            "--parent",
            &primary_id.to_string(),
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );
    assert!(
        create_tax_reserve.status.success(),
        "tax reserve account create should succeed: {}",
        String::from_utf8_lossy(&create_tax_reserve.stderr)
    );
    let tax_reserve_id = account_id_from_create_output(&create_tax_reserve);

    let deposit = run_cli(
        &database_url,
        &[
            "transaction",
            "deposit",
            "--account",
            &primary_id.to_string(),
            "--currency",
            "USD",
            "--amount",
            "1000",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );
    assert!(
        deposit.status.success(),
        "deposit should succeed: {}",
        String::from_utf8_lossy(&deposit.stderr)
    );

    let flat_list = run_cli(&database_url, &["accounts", "list"]);
    assert!(
        flat_list.status.success(),
        "flat account list should succeed"
    );
    let flat_stdout = String::from_utf8_lossy(&flat_list.stdout);
    assert!(flat_stdout.contains("primary account"));
    assert!(flat_stdout.contains("tax reserve"));

    let hierarchy = run_cli(&database_url, &["accounts", "list", "--hierarchy"]);
    assert!(
        hierarchy.status.success(),
        "hierarchy account list should succeed"
    );
    let hierarchy_stdout = String::from_utf8_lossy(&hierarchy.stdout);
    assert!(hierarchy_stdout.contains("primary account"));
    assert!(hierarchy_stdout.contains("  - tax reserve"));

    let balance = run_cli(&database_url, &["accounts", "balance"]);
    assert!(balance.status.success(), "account balance should succeed");
    let balance_stdout = String::from_utf8_lossy(&balance.stdout);
    assert!(balance_stdout.contains("primary account"));
    assert!(balance_stdout.contains("total=1000"));

    let transfer = run_cli(
        &database_url,
        &[
            "accounts",
            "transfer",
            "--from",
            &primary_id.to_string(),
            "--to",
            &tax_reserve_id.to_string(),
            "--amount",
            "250",
            "--reason",
            "quarterly-tax-reserve",
        ],
    );
    assert!(
        transfer.status.success(),
        "account transfer should succeed: {}",
        String::from_utf8_lossy(&transfer.stderr)
    );
    let transfer_stdout = String::from_utf8_lossy(&transfer.stdout);
    assert!(transfer_stdout.contains("Transfer completed:"));
    assert!(transfer_stdout.contains("reason: quarterly-tax-reserve"));

    let tax_reserve_deposit = run_cli(
        &database_url,
        &[
            "transaction",
            "deposit",
            "--account",
            &tax_reserve_id.to_string(),
            "--currency",
            "USD",
            "--amount",
            "250",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );
    assert!(
        tax_reserve_deposit.status.success(),
        "tax reserve deposit should succeed: {}",
        String::from_utf8_lossy(&tax_reserve_deposit.stderr)
    );

    let detailed_balance = run_cli(&database_url, &["accounts", "balance", "--detailed"]);
    assert!(
        detailed_balance.status.success(),
        "detailed account balance should succeed"
    );
    let detailed_stdout = String::from_utf8_lossy(&detailed_balance.stdout);
    assert!(detailed_stdout.contains("primary account"));
    assert!(detailed_stdout.contains("tax reserve"));
    assert!(detailed_stdout.contains("USD total=1000"));
    assert!(detailed_stdout.contains("USD total=250"));
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
fn test_bond_terms_update_and_metrics_cli_round_trip() {
    let database_url = format!("file:test_bond_terms_cli_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    {
        let database = SqliteDatabase::new(&database_url);
        let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));
        trust
            .create_trading_vehicle("9128285M8", None, &TradingVehicleCategory::Bond, "ibkr")
            .expect("seed bond trading vehicle");
    }

    let update = run_cli(
        &database_url,
        &[
            "trading-vehicle",
            "update-bond-terms",
            "--symbol",
            "9128285M8",
            "--broker",
            "ibkr",
            "--face-value",
            "1000",
            "--coupon-rate",
            "4.625",
            "--maturity-date",
            "2034-05-15",
            "--coupon-frequency",
            "2",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );
    assert!(
        update.status.success(),
        "bond term update should succeed: {}",
        String::from_utf8_lossy(&update.stderr)
    );

    let metrics = run_cli(
        &database_url,
        &[
            "metrics",
            "bond",
            "--symbol",
            "9128285M8",
            "--broker",
            "ibkr",
            "--market-price",
            "997.50",
            "--quantity",
            "5",
            "--years-to-maturity",
            "7",
            "--settlement-date",
            "2026-04-01",
            "--last-coupon-date",
            "2026-01-01",
            "--next-coupon-date",
            "2026-07-01",
            "--day-count",
            "actual-360",
            "--format",
            "json",
        ],
    );
    assert!(
        metrics.status.success(),
        "stored bond metrics should succeed: {}",
        String::from_utf8_lossy(&metrics.stderr)
    );
    let payload: Value =
        serde_json::from_slice(&metrics.stdout).expect("metrics stdout should be json");
    assert_eq!(payload["report"], "bond_metrics");
    assert_eq!(payload["input"]["face_value"], "1000");
    assert_eq!(payload["input"]["annual_coupon_rate_pct"], "4.625");
    assert_eq!(
        payload["input"]["accrued_interest"]["coupon_frequency_per_year"],
        2
    );
    assert_eq!(
        payload["input"]["accrued_interest"]["day_count_basis"],
        "actual-360"
    );
    assert_eq!(payload["data"]["annual_coupon_income"], "231.25");
    assert_eq!(payload["data"]["accrued_interest_per_unit"], "11.5625");
    assert_eq!(payload["data"]["accrued_interest_total"], "57.8125");
    assert_eq!(payload["data"]["dirty_price"], "1009.0625");
    assert_eq!(payload["data"]["position_dirty_value"], "5045.3125");
}

#[test]
fn test_trading_vehicle_stats_cli_reports_multi_asset_bond_inventory() {
    let database_url = format!(
        "file:test_trading_vehicle_stats_cli_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    seed_multi_asset_vehicle_inventory(&database_url);

    let stats = run_cli(
        &database_url,
        &["trading-vehicle", "stats", "--format", "json"],
    );
    assert!(
        stats.status.success(),
        "trading vehicle stats should succeed: {}",
        String::from_utf8_lossy(&stats.stderr)
    );

    let payload: Value =
        serde_json::from_slice(&stats.stdout).expect("stats stdout should be json");
    assert_eq!(payload["report"], "trading_vehicle_stats");
    assert_eq!(payload["total"], 4);
    assert_eq!(payload["by_category"]["stock"], 1);
    assert_eq!(payload["by_category"]["etf"], 1);
    assert_eq!(payload["by_category"]["bond"], 2);
    assert_eq!(payload["by_broker"]["ibkr"], 3);
    assert_eq!(payload["bonds"]["total"], 2);
    assert_eq!(payload["bonds"]["complete_terms"], 1);
    assert_eq!(payload["bonds"]["missing_terms"], 1);
    assert_eq!(payload["bonds"]["coupon_rate_count"], 2);
    assert_eq!(payload["bonds"]["average_coupon_rate_pct"], "5");
    assert_eq!(payload["bonds"]["earliest_maturity"], "2030-01-15");
}

#[test]
fn test_trading_vehicle_search_cli_filters_missing_bond_terms_json() {
    let database_url = format!(
        "file:test_trading_vehicle_search_cli_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    seed_multi_asset_vehicle_inventory(&database_url);

    let search = run_cli(
        &database_url,
        &[
            "trading-vehicle",
            "search",
            "--all",
            "--category",
            "bond",
            "--broker",
            "ibkr",
            "--symbol",
            "912",
            "--missing-bond-terms",
            "--format",
            "json",
        ],
    );
    assert!(
        search.status.success(),
        "trading vehicle search should succeed: {}",
        String::from_utf8_lossy(&search.stderr)
    );

    let payload: Value =
        serde_json::from_slice(&search.stdout).expect("search stdout should be json");
    assert_eq!(payload["report"], "trading_vehicle_search");
    assert_eq!(payload["filters"]["category"], "bond");
    assert_eq!(payload["filters"]["missing_bond_terms"], true);
    assert_eq!(payload["count"], 1);
    assert_eq!(payload["data"][0]["symbol"], "9128285M8");
    assert_eq!(payload["data"][0]["broker"], "ibkr");
    assert_eq!(payload["data"][0]["category"], "bond");
    assert_eq!(
        payload["data"][0]["fixed_income"]["maturity_date"],
        Value::Null
    );
    assert_eq!(
        payload["data"][0]["fixed_income"]["annual_coupon_rate_pct"],
        "6"
    );
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
