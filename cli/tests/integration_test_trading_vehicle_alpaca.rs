#![cfg(feature = "legacy-cli")]
mod support;
use std::process::Output;
use support::TestDatabase;

const PROTECTED_KEYWORD: &str = "I_UNDERSTAND_RISK";

fn run_command(database_url: &str, args: &[&str]) -> Output {
    support::legacy_command(database_url, PROTECTED_KEYWORD)
        .args(args)
        .output()
        .expect("run cli command")
}

#[test]
fn test_create_from_alpaca_requires_account() {
    let fixture = TestDatabase::new();
    let database_url = fixture.url();

    let output = run_command(
        &database_url,
        &[
            "trading-vehicle",
            "create",
            "--from-alpaca",
            "--symbol",
            "AAPL",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );

    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("broker_import_invalid_args"));
    assert!(stderr.contains("--account is required"));
}

#[test]
fn test_create_from_alpaca_requires_symbol() {
    let fixture = TestDatabase::new();
    let database_url = fixture.url();

    let output = run_command(
        &database_url,
        &[
            "trading-vehicle",
            "create",
            "--from-alpaca",
            "--account",
            "paper-account",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );

    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("broker_import_invalid_args"));
    assert!(stderr.contains("--symbol is required"));
}

#[test]
fn test_create_from_alpaca_unknown_account_fails() {
    let fixture = TestDatabase::new();
    let database_url = fixture.url();

    let output = run_command(
        &database_url,
        &[
            "trading-vehicle",
            "create",
            "--from-alpaca",
            "--account",
            "does-not-exist",
            "--symbol",
            "AAPL",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );

    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("broker_import_account_not_found"));
}
