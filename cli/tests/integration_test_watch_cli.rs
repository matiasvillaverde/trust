use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::Environment;
use rust_decimal_macros::dec;
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

fn seed_account(database_url: &str, name: &str) {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));
    trust
        .create_account(
            name,
            "watch cli test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");
}

#[test]
fn test_trade_watch_non_interactive_invalid_trade_id_returns_watch_error() {
    let database_url = format!("file:test_trade_watch_cli_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_name = "watch-cli-account";
    seed_account(&database_url, account_name);

    let output = run_cli(
        &database_url,
        &[
            "trade",
            "watch",
            "--account",
            account_name,
            "--trade-id",
            "not-a-uuid",
            "--json",
            "--timeout-secs",
            "1",
        ],
    );

    assert!(!output.status.success(), "watch command should fail");
    let stderr = String::from_utf8(output.stderr).expect("stderr utf8");
    assert!(
        stderr.contains("watch_invalid_trade_id"),
        "stderr should include structured watch error code; got: {stderr}"
    );
}
