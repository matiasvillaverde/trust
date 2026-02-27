use std::fs;
use std::path::Path;
use std::process::{Command, Output};
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

fn run_command(database_url: &str, args: &[&str]) -> Output {
    Command::new(cli_bin_path())
        .env("TRUST_DB_URL", database_url)
        .env("TRUST_PROTECTED_KEYWORD_EXPECTED", PROTECTED_KEYWORD)
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
            "--confirm-protected",
            PROTECTED_KEYWORD,
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
            "--confirm-protected",
            PROTECTED_KEYWORD,
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
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );

    assert!(!output.status.success(), "command should fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("alpaca_import_account_not_found"));
}
