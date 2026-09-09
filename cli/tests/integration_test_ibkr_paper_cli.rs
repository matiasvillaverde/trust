//! Opt-in, read-only broker checks through the real CLI, using an isolated local ledger.
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{AccountType, BrokerKind, Environment};
use rust_decimal_macros::dec;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;
use uuid::Uuid;

struct Sandbox(PathBuf);

fn valid_bar_data(value: &Value) -> bool {
    value
        .pointer("/data/bars")
        .and_then(Value::as_array)
        .is_some_and(|bars| {
            !bars.is_empty()
                && bars.iter().all(|bar| {
                    ["open", "high", "low", "close"].iter().all(|field| {
                        bar.get(field)
                            .and_then(Value::as_str)
                            .and_then(|text| text.parse::<rust_decimal::Decimal>().ok())
                            .is_some_and(|price| price > dec!(0))
                    }) && bar.get("volume").and_then(Value::as_u64).is_some()
                })
                && bars.iter().any(|bar| {
                    bar.get("volume")
                        .and_then(Value::as_u64)
                        .is_some_and(|volume| volume > 0)
                })
        })
}

#[test]
fn bar_acceptance_rejects_empty_or_silently_zeroed_volume() {
    assert!(!valid_bar_data(&serde_json::json!({"data": {"bars": []}})));
    assert!(!valid_bar_data(
        &serde_json::json!({"data": {"bars": [{"open": "10", "high": "11", "low": "9", "close": "10", "volume": 0}]}})
    ));
    assert!(valid_bar_data(
        &serde_json::json!({"data": {"bars": [{"open": "10", "high": "11", "low": "9", "close": "10", "volume": 1234}]}})
    ));
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "requires explicitly enabled authenticated IBKR paper gateway; no order mutations"]
fn paper_market_data_and_benchmark_through_cli() {
    assert_eq!(
        std::env::var("TRUST_IBKR_LIVE_E2E").as_deref(),
        Ok("1"),
        "explicit paper-test opt-in required"
    );
    let broker_account_id = std::env::var("TRUST_IBKR_ACCOUNT_ID").expect("paper account required");
    assert!(
        broker_account_id.starts_with("DU"),
        "paper account required"
    );
    let gateway = std::env::var("TRUST_IBKR_URL").expect("explicit gateway URL required");
    let root = std::env::temp_dir().join(format!("trust-paper-cli-{}", Uuid::new_v4()));
    std::fs::create_dir(&root).expect("create isolated test directory");
    let sandbox = Sandbox(root);
    let database_path = sandbox.0.join("fixture.db");
    let database_url = database_path.to_str().expect("UTF-8 database path");
    let account = {
        let mut facade = TrustFacade::new(
            Box::new(SqliteDatabase::new(database_url)),
            Box::new(ibkr_broker::IbkrBroker),
        );
        facade
            .create_account_with_profile(
                "paper-cli-fixture",
                "read-only broker test",
                Environment::Paper,
                dec!(0),
                dec!(0),
                AccountType::Primary,
                None,
                BrokerKind::Ibkr,
                Some(&broker_account_id),
            )
            .expect("create isolated paper profile")
    };
    ibkr_broker::IbkrBroker::preflight_gateway(&account).expect("authenticated paper gateway");
    let account_id = account.id.to_string();
    let mut scenarios = Vec::new();
    for operation in ["snapshot", "quote", "trade", "session"] {
        scenarios.push(vec![
            "market-data",
            operation,
            "--account",
            &account_id,
            "--symbol",
            "AAPL",
            "--format",
            "json",
        ]);
    }
    // A completed historical window makes the contract independent of today's session hours.
    for timeframe in ["1m", "1h", "1d"] {
        scenarios.push(vec![
            "market-data",
            "bars",
            "--account",
            &account_id,
            "--symbol",
            "AAPL",
            "--timeframe",
            timeframe,
            "--start",
            "2026-09-01T00:00:00Z",
            "--end",
            "2026-09-03T00:00:00Z",
            "--format",
            "json",
        ]);
    }
    scenarios.push(vec![
        "report",
        "benchmark",
        "--account",
        &account_id,
        "--benchmark",
        "SPY",
        "--from",
        "2026-09-01",
        "--to",
        "2026-09-03",
        "--format",
        "json",
    ]);
    let mut failures = Vec::new();
    for args in scenarios {
        let mut command = Command::new(env!("CARGO_BIN_EXE_trust"));
        command
            .env_clear()
            .env("TRUST_DB_URL", database_url)
            .env("TRUST_DISABLE_KEYCHAIN", "1")
            .env("TRUST_IBKR_URL", &gateway)
            .env("TRUST_IBKR_ACCOUNT_ID", &broker_account_id)
            .current_dir(&sandbox.0)
            .args(&args);
        if let Ok(value) = std::env::var("TRUST_IBKR_ALLOW_INSECURE_TLS") {
            command.env("TRUST_IBKR_ALLOW_INSECURE_TLS", value);
        }
        let output = command.output().expect("execute built trust binary");
        let payload = serde_json::from_slice::<Value>(&output.stdout);
        let valid = output.status.success()
            && payload.as_ref().is_ok_and(|value| {
                value.pointer("/source/broker").and_then(Value::as_str) == Some("ibkr")
                    && value.pointer("/scope/account_id").and_then(Value::as_str)
                        == Some(account_id.as_str())
                    && (args.get(1) != Some(&"bars") || valid_bar_data(value))
            });
        // Do not dump raw gateway output or account identifiers in failure reports.
        let mut label = args.iter().take(2).copied().collect::<Vec<_>>().join(" ");
        if let Some(timeframe) = args
            .windows(2)
            .find(|pair| pair.first() == Some(&"--timeframe"))
            .and_then(|pair| pair.get(1))
        {
            label.push_str(&format!(" {timeframe}"));
        }
        eprintln!("{label}: {}", if valid { "PASS" } else { "FAIL" });
        if !valid {
            failures.push(label);
        }
    }
    assert!(
        failures.is_empty(),
        "paper CLI scenarios failed: {failures:?}"
    );
}
