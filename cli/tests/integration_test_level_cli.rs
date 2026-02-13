use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{Currency, Environment, RuleLevel, RuleName, TransactionCategory};
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

fn seed_account_with_capital_and_rules(database_url: &str, name: &str) -> Uuid {
    let database = SqliteDatabase::new(database_url);
    let mut trust = TrustFacade::new(Box::new(database), Box::new(alpaca_broker::AlpacaBroker));
    let account = trust
        .create_account(
            name,
            "level cli quantity test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(50_000),
            &Currency::USD,
        )
        .expect("deposit");
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerMonth(20.0),
            "max monthly risk",
            &RuleLevel::Error,
        )
        .expect("risk per month");
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(2.0),
            "max trade risk",
            &RuleLevel::Error,
        )
        .expect("risk per trade");
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
    assert_eq!(payload["data"]["progress"]["current_level"], 3);

    let downgrade_paths = payload["data"]["progress"]["downgrade_paths"]
        .as_array()
        .expect("downgrade paths array");
    assert!(
        downgrade_paths
            .iter()
            .any(|path| path["path"] == "risk_breach_monthly_loss"),
        "expected monthly risk-breach path"
    );
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
fn test_level_progress_json_contract() {
    let database_url = format!(
        "file:test_level_progress_cli_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account(&database_url, "level-progress");

    let output = run_cli(
        &database_url,
        &[
            "level",
            "progress",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
            "--profitable-trades",
            "7",
            "--win-rate",
            "64",
            "--monthly-loss=-4.1",
            "--largest-loss=-1.7",
            "--consecutive-wins",
            "1",
        ],
    );
    assert!(output.status.success(), "level progress should succeed");

    let payload = parse_stdout_json(&output);
    assert_eq!(payload["report"], "level_evaluate");
    assert_eq!(payload["apply"], false);
    assert_eq!(payload["data"]["current_level"], 3);
    let upgrade_paths = payload["data"]["progress"]["upgrade_paths"]
        .as_array()
        .expect("upgrade paths");
    assert!(!upgrade_paths.is_empty(), "expected upgrade path");
}

#[test]
fn test_level_rules_show_json_contract() {
    let database_url = format!("file:test_level_rules_show_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account(&database_url, "level-rules-show");

    let output = run_cli(
        &database_url,
        &[
            "level",
            "rules",
            "show",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );
    assert!(output.status.success(), "rules show should succeed");

    let payload = parse_stdout_json(&output);
    assert_eq!(payload["report"], "level_rules");
    assert_eq!(payload["scope"]["account_id"], account_id.to_string());
    assert_eq!(payload["data"]["upgrade_profitable_trades"], 10);
    assert_eq!(payload["data"]["cooldown_profitable_trades"], 20);
}

#[test]
fn test_level_rules_set_requires_protected_keyword() {
    let database_url = format!(
        "file:test_level_rules_protected_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account(&database_url, "level-rules-protected");

    let output = run_cli(
        &database_url,
        &[
            "level",
            "rules",
            "set",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
            "--rule",
            "upgrade_profitable_trades",
            "--value",
            "12",
        ],
    );
    assert!(
        !output.status.success(),
        "rules set without keyword should fail"
    );
    let payload = parse_stderr_json(&output);
    assert_eq!(payload["error"]["code"], "protected_keyword_required");
}

#[test]
fn test_level_rules_set_updates_value() {
    let database_url = format!("file:test_level_rules_set_{}.db", Uuid::new_v4().simple());
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account(&database_url, "level-rules-set");

    let set_output = run_cli(
        &database_url,
        &[
            "level",
            "rules",
            "set",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
            "--rule",
            "upgrade_profitable_trades",
            "--value",
            "14",
            "--confirm-protected",
            PROTECTED_KEYWORD,
        ],
    );
    assert!(set_output.status.success(), "rules set should succeed");
    let payload = parse_stdout_json(&set_output);
    assert_eq!(payload["report"], "level_rules_set");
    assert_eq!(payload["data"]["updated_key"], "upgrade_profitable_trades");
    assert_eq!(payload["data"]["rules"]["upgrade_profitable_trades"], 14);

    let show_output = run_cli(
        &database_url,
        &[
            "level",
            "rules",
            "show",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
        ],
    );
    assert!(show_output.status.success(), "rules show should succeed");
    let show_payload = parse_stdout_json(&show_output);
    assert_eq!(show_payload["data"]["upgrade_profitable_trades"], 14);
}

#[test]
fn test_trade_size_preview_json_contract() {
    let database_url = format!(
        "file:test_trade_size_preview_{}.db",
        Uuid::new_v4().simple()
    );
    let _cleanup = TestDatabaseCleanup::new(&database_url);
    let account_id = seed_account_with_capital_and_rules(&database_url, "size-preview");

    let output = run_cli(
        &database_url,
        &[
            "trade",
            "size-preview",
            "--format",
            "json",
            "--account",
            &account_id.to_string(),
            "--entry",
            "40",
            "--stop",
            "38",
            "--currency",
            "usd",
        ],
    );
    assert!(output.status.success(), "size preview should succeed");

    let payload = parse_stdout_json(&output);
    assert_eq!(payload["report"], "trade_size_preview");
    assert_eq!(payload["scope"]["account_id"], account_id.to_string());
    assert_eq!(payload["data"]["base_quantity"], 500);
    assert_eq!(payload["data"]["current_level"], 3);
    assert_eq!(payload["data"]["current_multiplier"], "1");
    assert_eq!(payload["data"]["current_quantity"], 500);
    assert_eq!(payload["data"]["risk_per_share"], "2");
    let levels = payload["data"]["levels"].as_array().expect("levels array");
    assert_eq!(levels.len(), 5);
    assert_eq!(levels[2]["level"], 2);
    assert_eq!(levels[2]["quantity"], 250);
    assert_eq!(levels[4]["quantity"], 750);
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
