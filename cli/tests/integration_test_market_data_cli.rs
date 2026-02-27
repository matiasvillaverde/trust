use serde_json::Value;
use std::path::Path;
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

fn run_cli(database_url: &str, args: &[&str]) -> std::process::Output {
    Command::new(cli_bin_path())
        .env("TRUST_DB_URL", database_url)
        .env("TRUST_DISABLE_KEYCHAIN", "1")
        .env("TRUST_PROTECTED_KEYWORD_EXPECTED", "test_keyword")
        .args(args)
        .output()
        .expect("run cli")
}

fn parse_stderr_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stderr).expect("stderr must be valid JSON")
}

#[test]
fn test_market_data_bars_invalid_start_timestamp_json_error() {
    let database_url = format!("file:test_market_data_invalid_start_{}.db", Uuid::new_v4());
    let output = run_cli(
        &database_url,
        &[
            "market-data",
            "bars",
            "--format",
            "json",
            "--account",
            "paper-account",
            "--symbol",
            "AAPL",
            "--timeframe",
            "1m",
            "--start",
            "invalid-time",
            "--end",
            "2026-02-24T10:00:00Z",
        ],
    );
    assert!(!output.status.success(), "command must fail");

    let payload = parse_stderr_json(&output);
    assert_eq!(payload["error"]["code"], "invalid_timestamp");
}

#[test]
fn test_market_data_bars_end_before_start_json_error() {
    let database_url = format!("file:test_market_data_invalid_range_{}.db", Uuid::new_v4());
    let output = run_cli(
        &database_url,
        &[
            "market-data",
            "bars",
            "--format",
            "json",
            "--account",
            "paper-account",
            "--symbol",
            "AAPL",
            "--timeframe",
            "1h",
            "--start",
            "2026-02-24T11:00:00Z",
            "--end",
            "2026-02-24T10:00:00Z",
        ],
    );
    assert!(!output.status.success(), "command must fail");

    let payload = parse_stderr_json(&output);
    assert_eq!(payload["error"]["code"], "invalid_time_range");
}

#[test]
fn test_market_data_snapshot_unknown_account_json_error() {
    let database_url = format!(
        "file:test_market_data_unknown_account_{}.db",
        Uuid::new_v4()
    );
    let output = run_cli(
        &database_url,
        &[
            "market-data",
            "snapshot",
            "--format",
            "json",
            "--account",
            "does-not-exist",
            "--symbol",
            "AAPL",
        ],
    );
    assert!(!output.status.success(), "command must fail");

    let payload = parse_stderr_json(&output);
    assert_eq!(payload["error"]["code"], "account_not_found");
}

#[test]
fn test_market_data_help_lists_subcommands() {
    let database_url = format!("file:test_market_data_help_{}.db", Uuid::new_v4());
    let output = run_cli(&database_url, &["market-data", "--help"]);
    assert!(output.status.success(), "help command should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("snapshot"));
    assert!(stdout.contains("bars"));
    assert!(stdout.contains("stream"));
}

#[test]
fn test_market_data_stream_invalid_channel_json_error() {
    let database_url = format!(
        "file:test_market_data_invalid_channel_{}.db",
        Uuid::new_v4()
    );
    let output = run_cli(
        &database_url,
        &[
            "market-data",
            "stream",
            "--format",
            "json",
            "--account",
            "paper-account",
            "--symbols",
            "AAPL",
            "--channels",
            "foo",
            "--max-events",
            "5",
            "--timeout-seconds",
            "2",
        ],
    );
    assert!(!output.status.success(), "command must fail");

    let payload = parse_stderr_json(&output);
    assert_eq!(payload["error"]["code"], "invalid_channel");
}
