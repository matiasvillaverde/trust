#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::broker::*;
use super::fixtures::*;
use crate::dispatcher::ArgDispatcher;
use model::Broker;
use model::Trade;
use rust_decimal_macros::dec;
use uuid::Uuid;

#[test]
fn test_stub_broker_trade_operations_return_explicit_errors() {
    let broker = StubBroker::quote_trade_fixture();
    let trade = Trade::default();
    let account = model::Account::default();

    assert!(broker.submit_trade(&trade, &account).is_err());
    assert!(broker.sync_trade(&trade, &account).is_err());
    assert!(broker.close_trade(&trade, &account).is_err());
    assert!(broker.cancel_trade(&trade, &account).is_err());
    assert!(broker.modify_stop(&trade, &account, dec!(95)).is_err());
    assert!(broker.modify_target(&trade, &account, dec!(125)).is_err());
}

#[test]
fn test_public_dispatch_routes_top_level_subcommands() {
    let _guard = env_guard();
    std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");

    let dispatch = |argv: &[&str]| {
        let matches = root_command().get_matches_from(argv);
        test_dispatcher().dispatch(matches)
    };

    assert!(dispatch(&[
        "trust",
        "db",
        "import",
        "--input",
        "/tmp/missing-trust.json"
    ])
    .is_err());
    assert!(dispatch(&["trust", "keys", "protected-show"]).is_ok());
    assert!(dispatch(&["trust", "accounts", "list"]).is_ok());
    assert!(dispatch(&["trust", "transaction", "deposit"]).is_err());
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = dispatch(&["trust", "rule", "create"]);
    }))
    .is_err());
    assert!(dispatch(&[
        "trust",
        "trading-vehicle",
        "create",
        "--from-alpaca",
        "--symbol",
        "AAPL",
    ])
    .is_err());
    assert!(dispatch(&["trust", "trade", "fund", "--trade-id", "not-a-uuid"]).is_err());
    assert!(dispatch(&[
        "trust",
        "distribution",
        "history",
        "--account-id",
        "not-a-uuid"
    ])
    .is_err());
    assert!(dispatch(&[
        "trust",
        "report",
        "--format",
        "json",
        "performance",
        "--account",
        "bad",
    ])
    .is_err());
    assert!(dispatch(&[
        "trust",
        "market-data",
        "quote",
        "--account",
        "bad",
        "--symbol",
        "AAPL",
    ])
    .is_err());
    assert!(dispatch(&["trust", "grade", "show", "not-a-uuid"]).is_err());
    assert!(dispatch(&["trust", "level", "status", "--account", "bad"]).is_err());
    assert!(dispatch(&["trust", "onboarding", "status", "--format", "json"]).is_ok());
    assert!(dispatch(&["trust", "policy", "--format", "json"]).is_ok());
    assert!(dispatch(&[
        "trust",
        "metrics",
        "bond",
        "--face-value",
        "1000",
        "--market-price",
        "950",
        "--coupon-rate",
        "4",
        "--quantity",
        "3",
        "--years-to-maturity",
        "5",
    ])
    .is_ok());
    assert!(dispatch(&["trust", "advisor", "status", "--account", "bad"]).is_err());
    assert!(dispatch(&["trust", "session", "list", "--account", "bad"]).is_err());
    assert!(dispatch(&["trust", "external-plugin", "arg1", "arg2"]).is_ok());

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
}

#[test]
fn test_database_url_prefers_env_override() {
    let _guard = env_guard();
    std::env::set_var("TRUST_DB_URL", "/tmp/trust-custom.db");
    let value = ArgDispatcher::database_url();
    assert_eq!(value, "/tmp/trust-custom.db");
    std::env::remove_var("TRUST_DB_URL");
}

#[test]
fn test_database_url_uses_default_when_env_missing() {
    let _guard = env_guard();
    std::env::remove_var("TRUST_DB_URL");
    let value = ArgDispatcher::database_url();
    assert!(
        value.ends_with("/.trust/debug.db") || value.ends_with("/.trust/production.db"),
        "unexpected database path: {value}"
    );
}

#[test]
fn test_create_dir_if_necessary_creates_trust_directory() {
    let _guard = env_guard();
    let original_home = std::env::var_os("HOME");
    let home = std::env::temp_dir().join(format!("trust-home-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&home).expect("temp home should be created");
    std::env::set_var("HOME", &home);

    let trust_dir = home.join(".trust");
    assert!(!trust_dir.exists());
    crate::dispatcher::create_dir_if_necessary();
    assert!(trust_dir.exists());
    crate::dispatcher::create_dir_if_necessary();

    std::env::remove_var("HOME");
    let _ = std::fs::remove_dir_all(home);

    let home = std::env::temp_dir().join(format!("trust-home-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&home).expect("temp home should be created");
    std::env::set_var("HOME", &home);

    crate::dispatcher::create_dir_if_necessary();

    std::env::remove_var("HOME");
    let _ = std::fs::remove_dir_all(home);

    if let Some(original_home) = original_home {
        std::env::set_var("HOME", original_home);
    }
}

#[test]
fn test_new_sqlite_dispatcher_initializes_with_custom_db_url() {
    let _guard = env_guard();
    let db_path = format!("/tmp/trust-cli-{}.db", Uuid::new_v4());
    std::env::set_var("TRUST_DB_URL", &db_path);

    let _dispatcher = ArgDispatcher::new_sqlite();

    assert!(
        std::path::Path::new(&db_path).exists(),
        "sqlite database file should be created"
    );
    std::env::remove_var("TRUST_DB_URL");
    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn test_top_level_dispatch_routes_all_command_families_and_external() {
    let _guard = env_guard();
    std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");

    let cases: Vec<(Vec<&str>, bool)> = vec![
        (
            vec![
                "trust",
                "db",
                "import",
                "--input",
                "/tmp/does-not-exist.json",
            ],
            true,
        ),
        (vec!["trust", "keys", "protected-show"], false),
        (vec!["trust", "accounts", "list"], false),
        (
            vec![
                "trust",
                "transaction",
                "deposit",
                "--account",
                "missing",
                "--currency",
                "usd",
                "--amount",
                "10",
                "--confirm-protected",
                "secret",
            ],
            true,
        ),
        (vec!["trust", "rule", "remove"], true),
        (
            vec![
                "trust",
                "trading-vehicle",
                "create",
                "--from-alpaca",
                "--symbol",
                "AAPL",
            ],
            true,
        ),
        (
            vec!["trust", "trade", "fund", "--trade-id", "not-a-uuid"],
            true,
        ),
        (
            vec![
                "trust",
                "distribution",
                "history",
                "--account-id",
                "not-a-uuid",
            ],
            true,
        ),
        (
            vec![
                "trust",
                "report",
                "--format",
                "json",
                "summary",
                "--account",
                "bad",
            ],
            true,
        ),
        (
            vec![
                "trust",
                "market-data",
                "--format",
                "json",
                "snapshot",
                "--account",
                "bad",
                "--symbol",
                "AAPL",
            ],
            true,
        ),
        (vec!["trust", "grade", "show", "not-a-uuid"], true),
        (vec!["trust", "level", "status", "--account", "bad"], true),
        (
            vec!["trust", "onboarding", "--format", "json", "status"],
            false,
        ),
        (vec!["trust", "policy", "--format", "json"], false),
        (
            vec![
                "trust",
                "metrics",
                "compare",
                "--period1",
                "invalid",
                "--period2",
                "last-7-days",
                "--format",
                "json",
            ],
            false,
        ),
        (
            vec![
                "trust",
                "advisor",
                "configure",
                "--account",
                "bad",
                "--sector-limit",
                "30",
                "--asset-class-limit",
                "40",
                "--single-position-limit",
                "20",
                "--confirm-protected",
                "secret",
            ],
            true,
        ),
        (vec!["trust", "trade", "fund", "--unknown"], true),
        (vec!["trust", "external-cmd", "--foo", "bar"], false),
    ];

    for (argv, should_err) in cases {
        let observed_error = match root_command().try_get_matches_from(argv) {
            Ok(matches) => {
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    test_dispatcher().dispatch(matches)
                }));
                match outcome {
                    Ok(result) => result.is_err(),
                    Err(_) => true,
                }
            }
            Err(_) => true,
        };
        assert_eq!(observed_error, should_err);
    }

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
}
