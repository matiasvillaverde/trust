use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::Environment;
use rust_decimal_macros::dec;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

const PROTECTED_KEYWORD: &str = "I_UNDERSTAND_RISK";

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
        .env("TRUST_PROTECTED_KEYWORD_EXPECTED", PROTECTED_KEYWORD)
        .args(args)
        .output()
        .expect("run cli")
}

fn parse_stdout_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON")
}

fn parse_stderr_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stderr).expect("stderr must be valid JSON")
}

fn stdout_text(output: &std::process::Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout must be UTF-8")
}

fn seed_account(database_url: &str, name: &str) -> Uuid {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));
    let account = trust
        .create_account(
            name,
            "level cli test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");
    account.id
}

#[test]
fn test_level_status_json_contract() {
    let database_url = format!("file:test_level_status_cli_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account(&database_url, "level-status");

    let output = run_cli(
        &database_url,
        &[
            "level",
            "status",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );

    assert!(output.status.success(), "level status should succeed");
    let payload = parse_stdout_json(&output);

    assert_eq!(payload["report"], "level_status");
    assert_eq!(payload["format_version"], 1);
    assert_eq!(payload["scope"]["account_id"], account_id.to_string());
    assert_eq!(payload["data"]["current_level"], 3);
    assert_eq!(payload["data"]["status"], "normal");
    assert_eq!(payload["data"]["risk_multiplier"], "1");
}

#[test]
fn test_level_change_then_history_json_contains_event() {
    let database_url = format!("file:test_level_change_cli_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account(&database_url, "level-change");

    let change = run_cli(
        &database_url,
        &[
            "level",
            "change",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
            "--to",
            "2",
            "--reason",
            "Risk review downgrade",
            "--trigger",
            "manual_review",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );
    assert!(change.status.success(), "level change should succeed");

    let change_payload = parse_stdout_json(&change);
    assert_eq!(change_payload["report"], "level_change");
    assert_eq!(change_payload["data"]["level"]["current_level"], 2);
    assert_eq!(change_payload["data"]["event"]["old_level"], 3);
    assert_eq!(change_payload["data"]["event"]["new_level"], 2);

    let history = run_cli(
        &database_url,
        &[
            "level",
            "history",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
            "--days",
            "30",
        ],
    );
    assert!(history.status.success(), "level history should succeed");

    let history_payload = parse_stdout_json(&history);
    assert_eq!(history_payload["report"], "level_history");
    let events = history_payload["data"].as_array().expect("history array");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["old_level"], 3);
    assert_eq!(events[0]["new_level"], 2);
    assert_eq!(events[0]["trigger_type"], "manual_review");
}

#[test]
fn test_level_evaluate_apply_json_changes_level() {
    let database_url = format!("file:test_level_eval_cli_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account(&database_url, "level-evaluate");

    let output = run_cli(
        &database_url,
        &[
            "level",
            "evaluate",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
            "--profitable-trades",
            "0",
            "--win-rate",
            "20",
            "--monthly-loss=-6",
            "--largest-loss=-2.5",
            "--consecutive-wins",
            "0",
            "--apply",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );

    assert!(output.status.success(), "level evaluate should succeed");
    let payload = parse_stdout_json(&output);
    assert_eq!(payload["report"], "level_evaluate");
    assert_eq!(payload["apply"], true);
    assert_eq!(payload["data"]["decision"]["target_level"], 2);
    assert_eq!(payload["data"]["applied_level"], 2);
}

#[test]
fn test_level_status_without_account_errors_when_multiple_accounts_exist() {
    let database_url = format!("file:test_level_scope_cli_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let _ = seed_account(&database_url, "level-a");
    let _ = seed_account(&database_url, "level-b");

    let output = run_cli(&database_url, &["level", "status", "--format", "json"]);
    assert!(!output.status.success(), "level status should fail");

    let payload = parse_stderr_json(&output);
    assert_eq!(payload["error"]["code"], "account_selection_required");
}

#[test]
fn test_level_change_requires_protected_keyword() {
    let database_url = format!(
        "file:test_level_protected_cli_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account(&database_url, "level-protected");

    let output = run_cli(
        &database_url,
        &[
            "level",
            "change",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
            "--to",
            "2",
            "--reason",
            "Risk review downgrade",
            "--trigger",
            "manual_review",
        ],
    );
    assert!(
        !output.status.success(),
        "change without keyword should fail"
    );

    let payload = parse_stderr_json(&output);
    assert_eq!(payload["error"]["code"], "protected_keyword_required");
}

#[test]
fn test_level_triggers_json_contract() {
    let database_url = format!(
        "file:test_level_triggers_cli_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);

    let output = run_cli(&database_url, &["level", "triggers", "--format", "json"]);
    assert!(output.status.success(), "level triggers should succeed");

    let payload = parse_stdout_json(&output);
    assert_eq!(payload["report"], "level_triggers");
    assert_eq!(payload["format_version"], 1);
    assert_eq!(payload["data"]["custom_allowed"], true);
    let supported = payload["data"]["supported"]
        .as_array()
        .expect("supported triggers");
    assert!(supported.iter().any(|v| v == "manual_override"));
    assert!(supported.iter().any(|v| v == "performance_upgrade"));
}

#[test]
fn test_level_status_text_contract() {
    let database_url = format!("file:test_level_status_text_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account(&database_url, "level-status-text");

    let output = run_cli(
        &database_url,
        &["level", "status", "--account", &account_id.to_string()],
    );
    assert!(output.status.success(), "status text should succeed");
    let text = stdout_text(&output);
    assert!(text.contains("Mario Bros Level Status"));
    assert!(text.contains("Current Level: 3 (Full Size Trading)"));
    assert!(text.contains("Status: normal"));
}

#[test]
fn test_level_history_text_contract_after_change() {
    let database_url = format!(
        "file:test_level_history_text_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account(&database_url, "level-history-text");

    let change = run_cli(
        &database_url,
        &[
            "level",
            "change",
            "--account",
            &account_id.to_string(),
            "--to",
            "2",
            "--reason",
            "Risk review downgrade",
            "--trigger",
            "manual_review",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );
    assert!(change.status.success(), "change should succeed");

    let history = run_cli(
        &database_url,
        &["level", "history", "--account", &account_id.to_string()],
    );
    assert!(history.status.success(), "history text should succeed");
    let text = stdout_text(&history);
    assert!(text.contains("Level Change History"));
    assert!(text.contains("Level 3->2"));
    assert!(text.contains("Risk review downgrade"));
    assert!(text.contains("[manual_review]"));
}
