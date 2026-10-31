//! Shared isolation for legacy subprocess tests. No host database or keychain access.
use std::process::Command;

pub struct TestDatabase(tempfile::TempDir);

impl TestDatabase {
    pub fn new() -> Self {
        Self(tempfile::tempdir().expect("create isolated test directory"))
    }

    pub fn url(&self) -> String {
        self.0
            .path()
            .join("financial.db")
            .to_str()
            .expect("test database path must be UTF-8")
            .to_owned()
    }
}

pub fn legacy_command(database_url: &str, protected_keyword: &str) -> Command {
    // Cargo selects the current profile/target, including instrumented coverage builds.
    let mut command = Command::new(env!("CARGO_BIN_EXE_trust-legacy"));
    command
        .env("TRUST_DB_URL", database_url)
        .env("TRUST_DISABLE_KEYCHAIN", "1")
        .env("TRUST_PROTECTED_KEYWORD_EXPECTED", protected_keyword);
    command
}
