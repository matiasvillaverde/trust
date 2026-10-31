#![cfg(feature = "legacy-cli")]
mod support;
use serde_json::Value;
use support::TestDatabase;

fn run_cli(database_url: &str, args: &[&str]) -> std::process::Output {
    support::legacy_command(database_url, "test_keyword")
        .args(args)
        .output()
        .expect("run cli")
}

fn parse_stdout_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON")
}

#[test]
fn test_policy_json_contract() {
    let fixture = TestDatabase::new();
    let database_url = fixture.url();
    let output = run_cli(&database_url, &["policy", "--format", "json"]);
    assert!(output.status.success(), "policy command should succeed");

    let payload = parse_stdout_json(&output);
    assert_eq!(payload["report"], "policy");
    assert_eq!(payload["format_version"], 1);
    let protected = payload["data"]["protected"]
        .as_array()
        .expect("protected array");
    assert!(protected.iter().any(|v| v == "level change"));
    assert!(protected.iter().any(|v| v == "transaction withdraw"));
}

#[test]
fn test_onboarding_status_json_contract() {
    let fixture = TestDatabase::new();
    let database_url = fixture.url();
    let output = run_cli(&database_url, &["onboarding", "status", "--format", "json"]);
    assert!(
        output.status.success(),
        "onboarding status command should succeed"
    );

    let payload = parse_stdout_json(&output);
    assert_eq!(payload["report"], "onboarding_status");
    assert_eq!(payload["format_version"], 1);
    assert_eq!(payload["data"]["protected_keyword"], "configured");
}
