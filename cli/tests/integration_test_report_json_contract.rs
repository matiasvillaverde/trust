use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{Currency, DraftTrade, Environment, TradeCategory, TradingVehicleCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use uuid::Uuid;

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

fn decimal_from_json(value: &Value) -> Decimal {
    let as_str = value
        .as_str()
        .expect("decimal values in report JSON must be strings");
    Decimal::from_str(as_str).expect("valid decimal string in JSON")
}

fn seed_account_with_open_and_closed_trade(database_url: &str) -> Uuid {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    let account = trust
        .create_account(
            "Report JSON Test Account",
            "contract test",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
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

    let open_trade = trust
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity: 5,
                category: TradeCategory::Long,
                currency: Currency::USD,
                sector: Some("Technology".to_string()),
                asset_class: Some("Stocks".to_string()),
                thesis: Some("open trade".to_string()),
                context: None,
            },
            dec!(190),
            dec!(200),
            dec!(220),
        )
        .expect("create open trade");

    let _ = trust.fund_trade(&open_trade).expect("fund open trade");

    account.id
}

fn run_report(database_url: &str, args: &[&str]) -> std::process::Output {
    Command::new(cli_bin_path())
        .env("TRUST_DB_URL", database_url)
        .args(args)
        .output()
        .expect("run cli report command")
}

fn parse_stdout_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON")
}

fn assert_report_envelope(payload: &Value, report_name: &str) {
    assert_eq!(payload["report"], report_name);
    assert_eq!(payload["format_version"], 1);
    assert!(payload["generated_at"].is_string());
    assert!(payload["scope"].is_object());
}

fn seed_empty_account(database_url: &str) -> Uuid {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    let account = trust
        .create_account(
            "Empty Report Account",
            "empty contract test",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
        )
        .expect("create account");

    let _ = trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(10000),
            &Currency::USD,
        )
        .expect("deposit funds");

    account.id
}

#[test]
fn test_risk_report_json_math_consistency() {
    let database_url = format!("file:test_risk_report_json_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_open_and_closed_trade(&database_url);

    let output = run_report(
        &database_url,
        &[
            "report",
            "risk",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );

    assert!(output.status.success(), "risk report should succeed");

    let payload = parse_stdout_json(&output);
    assert_report_envelope(&payload, "risk");

    let open_positions = payload["data"]["open_positions"]
        .as_array()
        .expect("open_positions must be an array");
    let computed_sum = open_positions.iter().fold(dec!(0), |acc, position| {
        acc + decimal_from_json(&position["capital_amount"])
    });

    let total = decimal_from_json(&payload["data"]["total_capital_at_risk"]);
    assert_eq!(
        computed_sum, total,
        "sum(position.capital_amount) must equal total"
    );
    assert_eq!(payload["consistency"]["position_sum_matches_total"], true);
}

#[test]
fn test_concentration_report_json_math_consistency() {
    let database_url = format!(
        "file:test_concentration_report_json_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_open_and_closed_trade(&database_url);

    let output = run_report(
        &database_url,
        &[
            "report",
            "concentration",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );

    assert!(
        output.status.success(),
        "concentration report should succeed"
    );

    let payload = parse_stdout_json(&output);
    assert_report_envelope(&payload, "concentration");

    let sector_groups = payload["data"]["sector"]["groups"]
        .as_array()
        .expect("sector groups should be present");
    let sector_sum = sector_groups.iter().fold(dec!(0), |acc, group| {
        acc + decimal_from_json(&group["current_open_risk"])
    });
    let sector_total = decimal_from_json(&payload["data"]["sector"]["total_risk"]);

    assert_eq!(
        sector_sum, sector_total,
        "sum of sector risks must equal total"
    );
    assert_eq!(
        payload["consistency"]["sector_group_sum_matches_total"],
        true
    );
}

#[test]
fn test_summary_report_json_contains_advanced_metrics_and_balanced_counts() {
    let database_url = format!(
        "file:test_summary_report_json_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_open_and_closed_trade(&database_url);

    let output = run_report(
        &database_url,
        &[
            "report",
            "summary",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );

    assert!(output.status.success(), "summary report should succeed");

    let payload = parse_stdout_json(&output);
    assert_report_envelope(&payload, "summary");
    assert_eq!(payload["scope"]["account_id"], account_id.to_string());
    assert_eq!(payload["consistency"]["trade_count_balanced"], true);
    assert!(payload["data"]["advanced_metrics"]["expectancy"].is_string());
    assert!(payload["data"]["advanced_metrics"]["gross_profit"].is_string());
    assert!(payload["data"]["advanced_metrics"]["gross_loss"].is_string());
    assert!(payload["data"]["advanced_metrics"]["net_profit"].is_string());
    assert!(payload["data"]["advanced_metrics"]
        .get("sharpe_ratio")
        .is_some());
    assert!(payload["data"]["advanced_metrics"]
        .get("expected_shortfall_95")
        .is_some());
    assert!(payload["data"]["rolling_metrics"].is_array());
    assert!(payload["data"]["execution_quality"].is_object());
    assert!(payload["data"]["exposure"].is_object());
    assert!(payload["data"]["costs"].is_object());
    assert!(payload["data"]["costs"]["fees_total"].is_string());
    assert!(payload["data"]["confidence_intervals"].is_object());
    assert!(payload["data"]["agent_signals"].is_object());
}

#[test]
fn test_summary_report_all_accounts_scope_is_explicit_and_non_random() {
    let database_url = format!(
        "file:test_summary_report_all_accounts_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let _ = seed_account_with_open_and_closed_trade(&database_url);

    let output = run_report(&database_url, &["report", "summary", "--format", "json"]);

    assert!(output.status.success(), "summary report should succeed");

    let payload = parse_stdout_json(&output);
    assert_report_envelope(&payload, "summary");
    assert!(payload["scope"]["account_id"].is_null());

    let equity = decimal_from_json(&payload["data"]["equity"]);
    assert!(equity > dec!(0));
}

#[test]
fn test_report_invalid_account_in_json_mode_is_structured_error_and_non_zero() {
    let database_url = format!(
        "file:test_report_invalid_account_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let output = run_report(
        &database_url,
        &[
            "report",
            "performance",
            "--format",
            "json",
            "--account",
            "invalid-uuid",
        ],
    );

    assert!(
        !output.status.success(),
        "invalid account should fail with non-zero code"
    );

    let payload: Value = serde_json::from_slice(&output.stderr).expect("valid error JSON");
    assert_eq!(payload["error"]["code"], "invalid_account_id");
}

#[test]
fn test_drawdown_report_json_has_consistency_flags() {
    let database_url = format!(
        "file:test_drawdown_report_json_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_open_and_closed_trade(&database_url);

    let output = run_report(
        &database_url,
        &[
            "report",
            "drawdown",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );

    assert!(output.status.success(), "drawdown report should succeed");

    let payload = parse_stdout_json(&output);
    assert_report_envelope(&payload, "drawdown");
    assert!(payload["consistency"]["drawdown_non_negative"].is_boolean());
    assert!(payload["consistency"]["max_ge_current"].is_boolean());
}

#[test]
fn test_performance_report_json_has_balanced_counts() {
    let database_url = format!(
        "file:test_performance_report_json_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_open_and_closed_trade(&database_url);

    let output = run_report(
        &database_url,
        &[
            "report",
            "performance",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );

    assert!(output.status.success(), "performance report should succeed");

    let payload = parse_stdout_json(&output);
    assert_report_envelope(&payload, "performance");
    assert_eq!(payload["consistency"]["trade_count_balanced"], true);
    assert!(payload["data"]["total_trades"].is_number());
}

#[test]
fn test_metrics_report_json_schema_contract() {
    let database_url = format!(
        "file:test_metrics_report_json_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_open_and_closed_trade(&database_url);

    let output = run_report(
        &database_url,
        &[
            "report",
            "metrics",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );

    assert!(output.status.success(), "metrics report should succeed");

    let payload = parse_stdout_json(&output);
    assert_report_envelope(&payload, "metrics");
    assert!(payload["data"]["pnl"].is_object());
    assert!(payload["data"]["pnl"]["net_profit"].is_string());
    assert!(payload["data"]["costs"].is_object());
    assert!(payload["data"]["costs"]["fees_total"].is_string());
    assert!(payload["data"]["trade_quality"].is_object());
    assert!(payload["data"]["risk_adjusted_performance"].is_object());
    assert!(payload["data"]["tail_and_position_sizing"].is_object());
    assert!(payload["data"]["streaks"].is_object());
    assert!(payload["data"]["rolling_metrics"].is_array());
    assert!(payload["data"]["execution_quality"].is_object());
    assert!(payload["data"]["exposure"].is_object());
    assert!(payload["data"]["confidence_intervals"].is_object());
    assert!(payload["data"]["agent_signals"].is_object());
    assert!(payload["data"]["tail_and_position_sizing"]
        .get("expected_shortfall_95")
        .is_some());
    assert!(payload["data"]["streaks"]["max_consecutive_wins"].is_number());
    assert!(payload["data"]["streaks"]["max_consecutive_losses"].is_number());
}

#[test]
fn test_concentration_open_only_for_empty_account_returns_empty_groups() {
    let database_url = format!(
        "file:test_concentration_open_only_empty_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_empty_account(&database_url);

    let output = run_report(
        &database_url,
        &[
            "report",
            "concentration",
            "--format",
            "json",
            "--open-only",
            "--account",
            &account_id.to_string(),
        ],
    );

    assert!(
        output.status.success(),
        "concentration open-only on empty account should succeed; stdout={}; stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let payload = parse_stdout_json(&output);
    assert_report_envelope(&payload, "concentration");

    let sector_groups = payload["data"]["sector"]["groups"]
        .as_array()
        .expect("data.sector.groups should be an array");
    let asset_groups = payload["data"]["asset_class"]["groups"]
        .as_array()
        .expect("data.asset_class.groups should be an array");
    assert!(sector_groups.is_empty());
    assert!(asset_groups.is_empty());
}

#[test]
fn test_performance_report_json_for_empty_account_has_zero_trade_counts() {
    let database_url = format!(
        "file:test_performance_report_empty_json_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_empty_account(&database_url);

    let output = run_report(
        &database_url,
        &[
            "report",
            "performance",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );

    assert!(output.status.success(), "performance report should succeed");

    let payload = parse_stdout_json(&output);
    assert_report_envelope(&payload, "performance");
    assert_eq!(payload["data"]["total_trades"], 0);
    assert_eq!(payload["data"]["winning_trades"], 0);
    assert_eq!(payload["data"]["losing_trades"], 0);
}

#[test]
fn test_text_mode_invalid_account_exits_non_zero_with_plain_error() {
    let database_url = format!(
        "file:test_report_invalid_account_text_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let output = run_report(
        &database_url,
        &["report", "risk", "--account", "invalid-uuid"],
    );

    assert!(
        !output.status.success(),
        "invalid account should fail with non-zero code in text mode"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid_account_id"));
    assert!(
        !stderr.contains("\"error\""),
        "text mode should not output JSON error envelope"
    );
}

#[test]
fn test_summary_and_risk_reports_have_matching_total_capital_at_risk() {
    let database_url = format!(
        "file:test_summary_risk_consistency_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_open_and_closed_trade(&database_url);

    let risk_output = run_report(
        &database_url,
        &[
            "report",
            "risk",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );
    let summary_output = run_report(
        &database_url,
        &[
            "report",
            "summary",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );

    assert!(risk_output.status.success());
    assert!(summary_output.status.success());

    let risk_payload = parse_stdout_json(&risk_output);
    let summary_payload = parse_stdout_json(&summary_output);

    let risk_total = decimal_from_json(&risk_payload["data"]["total_capital_at_risk"]);
    let summary_total =
        decimal_from_json(&summary_payload["data"]["risk"]["total_capital_at_risk"]);
    assert_eq!(risk_total, summary_total);
}

#[test]
fn test_concentration_group_shares_are_within_bounds() {
    let database_url = format!(
        "file:test_concentration_share_bounds_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_open_and_closed_trade(&database_url);

    let output = run_report(
        &database_url,
        &[
            "report",
            "concentration",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );

    assert!(output.status.success());
    let payload = parse_stdout_json(&output);
    let groups = payload["data"]["sector"]["groups"]
        .as_array()
        .expect("sector groups should be an array");

    for group in groups {
        let share = decimal_from_json(&group["open_risk_share_percentage"]);
        assert!(share >= dec!(0), "share must be >= 0");
        assert!(share <= dec!(100), "share must be <= 100");
    }
}
