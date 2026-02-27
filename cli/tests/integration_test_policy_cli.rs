use serde_json::Value;
use std::path::Path;
use std::process::Command;
use uuid::Uuid;

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

fn run_cli(database_url: &str, args: &[&str]) -> std::process::Output {
    Command::new(cli_bin_path())
        .env("TRUST_DB_URL", database_url)
        .env("TRUST_DISABLE_KEYCHAIN", "1")
        .env("TRUST_PROTECTED_KEYWORD_EXPECTED", "test_keyword")
        .args(args)
        .output()
        .expect("run cli")
}

fn parse_stdout_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).expect("stdout must be valid JSON")
}

#[test]
fn test_policy_json_contract() {
    let database_url = format!("file:test_policy_cli_{}.db", Uuid::new_v4().simple());
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
    let database_url = format!(
        "file:test_onboarding_status_cli_{}.db",
        Uuid::new_v4().simple()
    );
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
