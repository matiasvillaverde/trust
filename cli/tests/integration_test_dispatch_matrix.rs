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

fn seed_account_with_open_trade(database_url: &str, broker_kind: model::BrokerKind) -> Uuid {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    let account = trust
        .create_account_with_profile(
            "dispatch-matrix",
            "integration",
            Environment::Paper,
            dec!(20),
            dec!(10),
            model::AccountType::Primary,
            None,
            broker_kind,
            Some("DU_FIXTURE_ONLY"),
        )
        .expect("create account");
    trust
        .create_account_with_profile(
            "dispatch-insurance",
            "fixture reserve",
            Environment::Paper,
            dec!(0),
            dec!(0),
            model::AccountType::Insurance,
            Some(account.id),
            broker_kind,
            None,
        )
        .expect("create insurance fixture");
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
                quantity: 5.into(),
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
fn test_dispatch_matrix_reports_levels_advisor_and_keys_alpaca() {
    reports_levels_advisor_and_keys(model::BrokerKind::Alpaca);
}

#[test]
fn test_dispatch_matrix_reports_levels_advisor_and_keys_ibkr() {
    reports_levels_advisor_and_keys(model::BrokerKind::Ibkr);
}

fn reports_levels_advisor_and_keys(broker_kind: model::BrokerKind) {
    let database_url = format!("file:test_dispatch_matrix_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_open_trade(&database_url, broker_kind);
    let account = account_id.to_string();

    for operation in ["configure", "execute"] {
        let args = if operation == "configure" {
            vec![
                "distribution",
                "configure",
                "--account-id",
                &account,
                "--earnings",
                "40",
                "--tax",
                "30",
                "--reinvestment",
                "30",
                "--threshold",
                "1",
                "--password",
                "fixture-distribution-only",
            ]
        } else {
            vec![
                "distribution",
                "execute",
                "--account-id",
                &account,
                "--amount",
                "1",
            ]
        };
        let output = run_cli(&database_url, &args);
        assert!(
            !output.status.success(),
            "distribution mutation must require authorization"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("protected_keyword_required"));
        let mut wrong_keyword = args.clone();
        wrong_keyword.extend(["--confirm-protected", "incorrect-fixture-keyword"]);
        let output = run_cli(&database_url, &wrong_keyword);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("protected_keyword_invalid"));
    }

    let report_commands: Vec<Vec<&str>> = vec![
        vec![
            "distribution",
            "configure",
            "--account-id",
            &account,
            "--earnings",
            "40",
            "--tax",
            "30",
            "--reinvestment",
            "30",
            "--threshold",
            "1",
            "--password",
            "fixture-distribution-only",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
        vec![
            "report",
            "attribution",
            "--account",
            &account,
            "--by",
            "symbol",
            "--from",
            "2020-01-01",
            "--to",
            "2099-12-31",
            "--format",
            "json",
        ],
        vec![
            "report",
            "timeline",
            "--account",
            &account,
            "--granularity",
            "month",
            "--from",
            "2020-01-01",
            "--to",
            "2099-12-31",
            "--format",
            "json",
        ],
        vec![
            "report",
            "bias-summary",
            "--account",
            &account,
            "--format",
            "json",
        ],
        vec![
            "report",
            "wash-sales",
            "--account",
            &account,
            "--format",
            "json",
        ],
        vec!["report", "tax", "--account", &account, "--format", "json"],
        vec!["accounts", "list"],
        vec!["accounts", "balance"],
        vec![
            "trade",
            "list-open",
            "--account",
            &account,
            "--format",
            "json",
        ],
        vec![
            "trade",
            "search",
            "--account",
            &account,
            "--status",
            "funded",
            "--format",
            "json",
        ],
        vec![
            "grade",
            "summary",
            "--account",
            &account,
            "--format",
            "json",
        ],
        vec!["session", "list", "--account", &account, "--format", "json"],
        vec!["onboarding", "status", "--format", "json"],
        vec!["policy", "--format", "json"],
        vec!["metrics", "advanced", "--account", &account],
        vec![
            "metrics",
            "compare",
            "--account",
            &account,
            "--period1",
            "last-30-days",
            "--period2",
            "previous-30-days",
        ],
        vec!["distribution", "history", "--account-id", &account],
        vec!["distribution", "insurance", "--account-id", &account],
        vec!["distribution", "rules", "show", "--account-id", &account],
        vec![
            "level",
            "rules",
            "show",
            "--account",
            &account,
            "--format",
            "json",
        ],
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
        if args.windows(2).any(|pair| pair == ["--format", "json"]) {
            let payload: serde_json::Value = serde_json::from_slice(&output.stdout)
                .expect("successful JSON commands must emit machine-clean JSON");
            assert!(payload.is_object(), "expected JSON object for {args:?}");
            if matches!(args.get(1), Some(&"attribution" | &"timeline")) {
                assert_eq!(
                    payload
                        .pointer("/source/broker")
                        .and_then(serde_json::Value::as_str),
                    Some(broker_kind.as_str()),
                    "broker provenance for {args:?}"
                );
                assert_eq!(
                    payload
                        .pointer("/scope/account_id")
                        .and_then(serde_json::Value::as_str),
                    Some(account.as_str())
                );
            }
        }
    }
}

#[test]
fn test_dispatch_matrix_market_data_and_trade_preview_paths() {
    let database_url = format!(
        "file:test_dispatch_matrix_md_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_open_trade(&database_url, model::BrokerKind::Alpaca);
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

    let hypothesis = run_cli(
        &database_url,
        &[
            "trade",
            "hypothesis",
            "--account",
            account.as_str(),
            "--entry",
            "200",
            "--stop",
            "190",
            "--target",
            "230",
            "--quantity",
            "10",
            "--currency",
            "usd",
            "--format",
            "json",
        ],
    );
    assert!(
        hypothesis.status.success(),
        "trade hypothesis should succeed: {}",
        String::from_utf8_lossy(&hypothesis.stderr)
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
