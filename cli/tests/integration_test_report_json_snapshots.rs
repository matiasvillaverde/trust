use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{Currency, DraftTrade, Environment, TradeCategory, TradingVehicleCategory};
use rust_decimal_macros::dec;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

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

fn run_report(database_url: &str, args: &[&str]) -> std::process::Output {
    Command::new(cli_bin_path())
        .env("TRUST_DB_URL", database_url)
        .args(args)
        .output()
        .expect("run cli report command")
}

fn seed_snapshot_dataset(database_url: &str) -> Uuid {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));

    let account = trust
        .create_account(
            "Snapshot Account",
            "snapshot contract dataset",
            Environment::Paper,
            dec!(25.0),
            dec!(10.0),
        )
        .expect("create account");

    let _ = trust
        .create_transaction(
            &account,
            &model::TransactionCategory::Deposit,
            dec!(75000),
            &Currency::USD,
        )
        .expect("deposit funds");

    let stock = trust
        .create_trading_vehicle(
            "AAPL",
            Some("US0378331005"),
            &TradingVehicleCategory::Stock,
            "alpaca",
        )
        .expect("create stock vehicle");

    let etf = trust
        .create_trading_vehicle(
            "XLV",
            Some("US81369Y2090"),
            &TradingVehicleCategory::Stock,
            "alpaca",
        )
        .expect("create etf vehicle");

    let t1 = trust
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: stock,
                quantity: 7,
                category: TradeCategory::Long,
                currency: Currency::USD,
                sector: Some("Technology".to_string()),
                asset_class: Some("Stocks".to_string()),
                thesis: Some("snapshot open tech".to_string()),
                context: None,
            },
            dec!(95),
            dec!(100),
            dec!(120),
        )
        .expect("create trade 1");
    let _ = trust.fund_trade(&t1).expect("fund trade 1");

    let t2 = trust
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: etf,
                quantity: 4,
                category: TradeCategory::Long,
                currency: Currency::USD,
                sector: Some("Healthcare".to_string()),
                asset_class: Some("ETF".to_string()),
                thesis: Some("snapshot open health".to_string()),
                context: None,
            },
            dec!(125),
            dec!(130),
            dec!(150),
        )
        .expect("create trade 2");
    let _ = trust.fund_trade(&t2).expect("fund trade 2");

    account.id
}

fn normalize_json(mut payload: Value) -> Value {
    scrub_volatile_fields(&mut payload);
    normalize_arrays(&mut payload);
    payload
}

fn scrub_volatile_fields(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for key in ["generated_at", "funded_date", "max_drawdown_date"] {
                if map.contains_key(key) {
                    map.insert(key.to_string(), Value::String("<redacted>".to_string()));
                }
            }
            if map.contains_key("trade_id") {
                map.insert(
                    "trade_id".to_string(),
                    Value::String("<trade-id>".to_string()),
                );
            }
            if map.contains_key("account_id") && map.get("account_id").is_some_and(Value::is_string)
            {
                map.insert(
                    "account_id".to_string(),
                    Value::String("<account-id>".to_string()),
                );
            }
            if map.contains_key("days_since_peak") {
                map.insert("days_since_peak".to_string(), Value::from(0));
            }
            if map.contains_key("days_in_drawdown") {
                map.insert("days_in_drawdown".to_string(), Value::from(0));
            }
            for value in map.values_mut() {
                scrub_volatile_fields(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                scrub_volatile_fields(item);
            }
        }
        _ => {}
    }
}

fn normalize_arrays(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for value in map.values_mut() {
                normalize_arrays(value);
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                normalize_arrays(item);
            }
            if items
                .iter()
                .all(|item| item.get("name").is_some_and(Value::is_string))
            {
                items.sort_by(|left, right| {
                    let left_name = left
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let right_name = right
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    left_name.cmp(&right_name)
                });
            }
            if items
                .iter()
                .all(|item| item.get("symbol").is_some_and(Value::is_string))
            {
                items.sort_by(|left, right| {
                    let left_symbol = left
                        .get("symbol")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let right_symbol = right
                        .get("symbol")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    left_symbol.cmp(&right_symbol)
                });
            }
        }
        _ => {}
    }
}

fn snapshot_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
        .join(format!("{name}.json"))
}

fn assert_or_update_snapshot(name: &str, payload: &Value) {
    let path = snapshot_path(name);
    let pretty = serde_json::to_string_pretty(payload).expect("serialize payload");

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create snapshot directory");
    }

    let should_update = std::env::var("UPDATE_SNAPSHOTS")
        .map(|value| value == "1")
        .unwrap_or(false);

    if should_update || !path.exists() {
        fs::write(&path, &pretty).expect("write snapshot");
        return;
    }

    let expected = fs::read_to_string(&path).expect("read snapshot");
    assert_eq!(expected, pretty, "snapshot mismatch for {}", name);
}

#[test]
fn test_report_json_snapshots() {
    let database_url = format!(
        "file:test_report_json_snapshots_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_snapshot_dataset(&database_url);
    let account_arg = account_id.to_string();

    let cases: Vec<(&str, Vec<String>)> = vec![
        (
            "report_performance",
            vec![
                "report".into(),
                "performance".into(),
                "--format".into(),
                "json".into(),
                "--account".into(),
                account_arg.clone(),
            ],
        ),
        (
            "report_drawdown",
            vec![
                "report".into(),
                "drawdown".into(),
                "--format".into(),
                "json".into(),
                "--account".into(),
                account_arg.clone(),
            ],
        ),
        (
            "report_risk",
            vec![
                "report".into(),
                "risk".into(),
                "--format".into(),
                "json".into(),
                "--account".into(),
                account_arg.clone(),
            ],
        ),
        (
            "report_concentration",
            vec![
                "report".into(),
                "concentration".into(),
                "--format".into(),
                "json".into(),
                "--account".into(),
                account_arg.clone(),
            ],
        ),
        (
            "report_concentration_open_only",
            vec![
                "report".into(),
                "concentration".into(),
                "--open-only".into(),
                "--format".into(),
                "json".into(),
                "--account".into(),
                account_arg.clone(),
            ],
        ),
        (
            "report_summary",
            vec![
                "report".into(),
                "summary".into(),
                "--format".into(),
                "json".into(),
                "--account".into(),
                account_arg.clone(),
            ],
        ),
        (
            "report_metrics",
            vec![
                "report".into(),
                "metrics".into(),
                "--format".into(),
                "json".into(),
                "--account".into(),
                account_arg.clone(),
            ],
        ),
    ];

    for (snapshot_name, args) in cases {
        let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = run_report(&database_url, &args_ref);
        assert!(
            output.status.success(),
            "snapshot case failed: {}",
            snapshot_name
        );
        let payload: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
        let normalized = normalize_json(payload);
        assert_or_update_snapshot(snapshot_name, &normalized);
    }
}
