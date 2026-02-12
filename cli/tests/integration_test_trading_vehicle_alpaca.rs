use std::fs;
use std::path::Path;
use std::process::{Command, Output};
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

fn run_command(database_url: &str, args: &[&str]) -> Output {
    Command::new(cli_bin_path())
        .env("TRUST_DB_URL", database_url)
        .args(args)
        .output()
        .expect("run cli command")
}

#[test]
fn test_create_from_alpaca_requires_account() {
    let database_url = format!(
        "file:test_tv_alpaca_no_account_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let output = run_command(
        &database_url,
        &[
            "trading-vehicle",
            "create",
            "--from-alpaca",
            "--symbol",
            "AAPL",
        ],
    );

    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("alpaca_import_invalid_args"));
    assert!(stderr.contains("--account is required"));
}

#[test]
fn test_create_from_alpaca_requires_symbol() {
    let database_url = format!(
        "file:test_tv_alpaca_no_symbol_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let output = run_command(
        &database_url,
        &[
            "trading-vehicle",
            "create",
            "--from-alpaca",
            "--account",
            "paper-account",
        ],
    );

    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("alpaca_import_invalid_args"));
    assert!(stderr.contains("--symbol is required"));
}

#[test]
fn test_create_from_alpaca_unknown_account_fails() {
    let database_url = format!(
        "file:test_tv_alpaca_missing_account_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);

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
        ],
    );

    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("alpaca_import_account_not_found"));
}
