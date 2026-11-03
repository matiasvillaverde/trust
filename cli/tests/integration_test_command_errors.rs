//! The shipped command tree must reject incomplete input before financial work.
#![cfg(feature = "legacy-cli")]
mod support;
use std::process::Stdio;

#[test]
fn incomplete_commands_fail_before_opening_a_financial_database() {
    for arguments in [
        vec![],
        vec!["grade"],
        vec!["level"],
        vec!["level", "rules"],
        vec!["report"],
        vec!["market-data"],
        vec!["session"],
        vec!["onboarding"],
        vec!["rule"],
        vec!["db"],
        vec!["keys"],
        vec!["accounts"],
        vec!["transaction"],
        vec!["trading-vehicle"],
        vec!["trade"],
        vec!["distribution"],
        vec!["metrics"],
        vec!["advisor"],
    ] {
        let db = support::TestDatabase::new();
        let output = support::legacy_command(&db.url(), "command-contract")
            .args(&arguments)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{arguments:?}: {output:?}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("Usage:"), "{arguments:?}: {error}");
        assert!(!error.contains("panicked"), "{arguments:?}: {error}");
        assert!(
            output.stdout.is_empty(),
            "help/errors must not be confused with successful data"
        );
        assert!(
            !std::path::Path::new(&db.url()).exists(),
            "{arguments:?}: parser failure opened a database"
        );
    }
}

#[test]
fn invalid_mutation_requests_fail_without_changing_persisted_cash() {
    use model::{Currency, Environment, TransactionCategory};
    use rust_decimal_macros::dec;
    let db = support::TestDatabase::new();
    let mut trust = core::TrustFacade::new_with_brokers(
        Box::new(db_sqlite::SqliteDatabase::new(&db.url())),
        vec![],
    );
    let account = trust
        .create_account("protected", "cash", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1234.56),
            &Currency::USD,
        )
        .unwrap();
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    let receipts = trust.get_account_transactions(account.id).unwrap();
    for (arguments, cause) in [
        (vec!["rule", "create"], "protected_keyword_required"),
        (vec!["rule", "remove"], "protected_keyword_required"),
        (vec!["trade", "fund", "--trade-id", "not-a-uuid"], "UUID"),
        (
            vec!["distribution", "history", "--account-id", "not-a-uuid"],
            "UUID",
        ),
        (
            vec![
                "transaction",
                "deposit",
                "--account",
                "missing",
                "--currency",
                "usd",
                "--amount",
                "10",
                "--confirm-protected",
                "command-contract",
            ],
            "account",
        ),
        (vec!["trade", "fund", "--unknown"], "unexpected argument"),
    ] {
        let output = support::legacy_command(&db.url(), "command-contract")
            .args(&arguments)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!output.status.success(), "{arguments:?}: {output:?}");
        let diagnostic = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            diagnostic.to_lowercase().contains(&cause.to_lowercase()),
            "{arguments:?}: {diagnostic}"
        );
        assert!(!diagnostic.contains("panicked"), "{diagnostic}");
        assert_eq!(
            trust.search_balance(account.id, &Currency::USD).unwrap(),
            before
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            receipts
        );
    }
}
