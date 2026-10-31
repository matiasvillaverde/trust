#![cfg(feature = "legacy-cli")]
mod support;

#[test]
fn subprocess_uses_current_build_and_leaves_no_database_artifacts() {
    let fixture = support::TestDatabase::new();
    let database_url = fixture.url();
    let database_path = std::path::Path::new(&database_url);
    let directory = database_path.parent().expect("fixture parent").to_owned();
    let mut command = support::legacy_command(&database_url, "test_keyword");
    let executable = std::env::current_exe().expect("test executable");
    let profile = executable
        .parent()
        .and_then(std::path::Path::parent)
        .expect("Cargo profile directory");
    assert_eq!(
        std::path::Path::new(command.get_program())
            .parent()
            .expect("binary profile directory")
            .canonicalize()
            .expect("existing binary directory"),
        profile.canonicalize().expect("existing test directory")
    );
    assert!(command
        .get_envs()
        .any(|(name, value)| name == "TRUST_DISABLE_KEYCHAIN"
            && value == Some(std::ffi::OsStr::new("1"))));
    let output = command
        .args(["policy", "--format", "json"])
        .output()
        .expect("run current legacy binary");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).expect("policy JSON")["report"],
        "policy"
    );
    assert!(
        database_path.is_file(),
        "CLI must create only the selected fixture database"
    );
    drop(fixture);
    assert!(
        !directory.exists(),
        "Database and sidecars must not outlive the fixture"
    );
}
