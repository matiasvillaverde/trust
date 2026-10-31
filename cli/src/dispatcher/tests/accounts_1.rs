#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use model::AccountType;
use model::BrokerKind;
use model::Currency;
use model::Environment;
use model::TradingVehicleCategory;
use model::TransactionCategory;
use rust_decimal_macros::dec;

#[test]
fn test_parse_account_type_accepts_aliases() {
    assert_eq!(
        crate::dispatcher::accounts::parse_account_type("primary").unwrap(),
        AccountType::Primary
    );
    assert_eq!(
        crate::dispatcher::accounts::parse_account_type("earnings").unwrap(),
        AccountType::Earnings
    );
    assert_eq!(
        crate::dispatcher::accounts::parse_account_type("tax-reserve").unwrap(),
        AccountType::TaxReserve
    );
    assert_eq!(
        crate::dispatcher::accounts::parse_account_type("tax_reserve").unwrap(),
        AccountType::TaxReserve
    );
    assert_eq!(
        crate::dispatcher::accounts::parse_account_type("reinvestment").unwrap(),
        AccountType::Reinvestment
    );
    assert_eq!(
        crate::dispatcher::accounts::parse_account_type("insurance").unwrap(),
        AccountType::Insurance
    );
}

#[test]
fn test_parse_environment_accepts_aliases() {
    assert_eq!(
        crate::dispatcher::accounts::parse_environment("paper").unwrap(),
        Environment::Paper
    );
    assert_eq!(
        crate::dispatcher::accounts::parse_environment("sandbox").unwrap(),
        Environment::Paper
    );
    assert_eq!(
        crate::dispatcher::accounts::parse_environment("live").unwrap(),
        Environment::Live
    );
    assert_eq!(
        crate::dispatcher::accounts::parse_environment("production").unwrap(),
        Environment::Live
    );
}

#[test]
fn test_parse_environment_rejects_invalid_value() {
    let error = crate::dispatcher::accounts::parse_environment("demo").unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid_environment: Invalid --environment value (expected paper|live)"
    );
}

#[test]
fn test_parse_account_type_rejects_invalid_value() {
    let error = crate::dispatcher::accounts::parse_account_type("savings").unwrap_err();
    assert_eq!(
            error.to_string(),
            "invalid_account_type: Invalid --type value (expected primary|earnings|tax-reserve|reinvestment|insurance)"
        );
}

#[test]
fn test_parse_broker_kind_accepts_supported_values() {
    assert_eq!(
        crate::dispatcher::accounts::parse_broker_kind("alpaca").unwrap(),
        BrokerKind::Alpaca
    );
    assert_eq!(
        crate::dispatcher::accounts::parse_broker_kind("IBKR").unwrap(),
        BrokerKind::Ibkr
    );
}

#[test]
fn test_parse_broker_kind_rejects_invalid_value() {
    let error = crate::dispatcher::accounts::parse_broker_kind("binance").unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid_broker_kind: Invalid --broker value (expected alpaca|ibkr)"
    );
}

#[test]
fn test_parse_trading_vehicle_category_rejects_unknown_values() {
    assert_eq!(
        crate::dispatcher::accounts::parse_trading_vehicle_category("bond").unwrap(),
        TradingVehicleCategory::Bond
    );
    let error = crate::dispatcher::accounts::parse_trading_vehicle_category("option").unwrap_err();
    assert!(error
        .to_string()
        .contains("invalid_trading_vehicle_category"));
}

#[test]
fn test_non_interactive_arg_detection_helpers() {
    let empty_accounts = account_matches(&[]);
    assert!(!crate::dispatcher::accounts::has_non_interactive_account_args(&empty_accounts));
    let named_accounts = account_matches(&["--name", "demo"]);
    assert!(crate::dispatcher::accounts::has_non_interactive_account_args(&named_accounts));

    let empty_trade = trade_matches(&[]);
    assert!(!crate::dispatcher::trade_commands::has_non_interactive_trade_args(&empty_trade));
    let trade_with_symbol = trade_matches(&["--symbol", "AAPL"]);
    assert!(crate::dispatcher::trade_commands::has_non_interactive_trade_args(&trade_with_symbol));
}

#[test]
fn test_create_account_non_interactive_missing_required_fields() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();

    let matches = account_matches(&["--confirm-protected", "secret", "--description", "x"]);
    let error = crate::dispatcher::accounts::create_account(&mut dispatcher.trust, &matches)
        .expect_err("missing name should fail");
    assert!(error.to_string().contains("missing_name"));

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_create_account_non_interactive_validates_remaining_required_and_enum_fields() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");

    let assert_error = |args: &[&str], expected_code: &str| {
        let mut dispatcher = test_dispatcher();
        let matches = account_matches(args);
        let error = crate::dispatcher::accounts::create_account(&mut dispatcher.trust, &matches)
            .expect_err("invalid account args should fail");
        assert!(
            error.to_string().contains(expected_code),
            "expected `{expected_code}` in `{error}`"
        );
    };

    assert_error(
        &["--confirm-protected", "secret", "--name", "missing-desc"],
        "missing_description",
    );
    assert_error(
        &[
            "--confirm-protected",
            "secret",
            "--name",
            "missing-env",
            "--description",
            "missing env",
        ],
        "missing_environment",
    );
    assert_error(
        &[
            "--confirm-protected",
            "secret",
            "--name",
            "missing-taxes",
            "--description",
            "missing taxes",
            "--environment",
            "paper",
        ],
        "missing_taxes",
    );
    assert_error(
        &[
            "--confirm-protected",
            "secret",
            "--name",
            "missing-earnings",
            "--description",
            "missing earnings",
            "--environment",
            "paper",
            "--taxes",
            "20",
        ],
        "missing_earnings",
    );
    assert_error(
        &[
            "--confirm-protected",
            "secret",
            "--name",
            "bad-type",
            "--description",
            "bad type",
            "--environment",
            "paper",
            "--taxes",
            "20",
            "--earnings",
            "10",
            "--type",
            "margin",
        ],
        "invalid_account_type",
    );
    assert_error(
        &[
            "--confirm-protected",
            "secret",
            "--name",
            "bad-broker",
            "--description",
            "bad broker",
            "--environment",
            "paper",
            "--taxes",
            "20",
            "--earnings",
            "10",
            "--broker",
            "manual",
        ],
        "invalid_broker",
    );

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_create_account_non_interactive_rejects_invalid_parent_uuid() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();

    let matches = account_matches(&[
        "--confirm-protected",
        "secret",
        "--name",
        "child",
        "--description",
        "child",
        "--environment",
        "paper",
        "--taxes",
        "20",
        "--earnings",
        "10",
        "--parent",
        "not-a-uuid",
    ]);
    let error = crate::dispatcher::accounts::create_account(&mut dispatcher.trust, &matches)
        .expect_err("invalid parent uuid should fail");
    assert!(error.to_string().contains("invalid_uuid"));

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_create_account_non_interactive_requires_ibkr_primary_account_id() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();

    let matches = account_matches(&[
        "--confirm-protected",
        "secret",
        "--name",
        "ibkr-main",
        "--description",
        "ibkr trading",
        "--environment",
        "paper",
        "--taxes",
        "20",
        "--earnings",
        "10",
        "--broker",
        "ibkr",
    ]);
    let error = crate::dispatcher::accounts::create_account(&mut dispatcher.trust, &matches)
        .expect_err("missing ibkr broker account id should fail");
    assert!(error.to_string().contains("missing_broker_account_id"));

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_create_account_non_interactive_validates_environment_and_decimals() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");

    let invalid_environment = account_matches(&[
        "--confirm-protected",
        "secret",
        "--name",
        "bad-env",
        "--description",
        "bad env",
        "--environment",
        "demo",
        "--taxes",
        "20",
        "--earnings",
        "10",
    ]);
    let mut dispatcher = test_dispatcher();
    let env_error =
        crate::dispatcher::accounts::create_account(&mut dispatcher.trust, &invalid_environment)
            .expect_err("invalid environment should fail");
    assert!(env_error.to_string().contains("invalid_environment"));

    let invalid_taxes = account_matches(&[
        "--confirm-protected",
        "secret",
        "--name",
        "bad-tax",
        "--description",
        "bad tax",
        "--environment",
        "paper",
        "--taxes",
        "abc",
        "--earnings",
        "10",
    ]);
    let mut dispatcher = test_dispatcher();
    let tax_error =
        crate::dispatcher::accounts::create_account(&mut dispatcher.trust, &invalid_taxes)
            .expect_err("invalid taxes should fail");
    assert!(tax_error.to_string().contains("invalid_decimal"));

    let invalid_earnings = account_matches(&[
        "--confirm-protected",
        "secret",
        "--name",
        "bad-earnings",
        "--description",
        "bad earnings",
        "--environment",
        "paper",
        "--taxes",
        "20",
        "--earnings",
        "abc",
    ]);
    let mut dispatcher = test_dispatcher();
    let earnings_error =
        crate::dispatcher::accounts::create_account(&mut dispatcher.trust, &invalid_earnings)
            .expect_err("invalid earnings should fail");
    assert!(earnings_error.to_string().contains("invalid_decimal"));

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_create_account_non_interactive_success_with_broker_profiles() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();

    let ibkr_primary = account_matches(&[
        "--confirm-protected",
        "secret",
        "--name",
        "ibkr-profile",
        "--description",
        "ibkr primary",
        "--environment",
        "paper",
        "--taxes",
        "20",
        "--earnings",
        "10",
        "--broker",
        "ibkr",
        "--broker-account-id",
        "U1234567",
    ]);
    crate::dispatcher::accounts::create_account(&mut dispatcher.trust, &ibkr_primary)
        .expect("ibkr primary account should be created");
    let created = dispatcher
        .trust
        .search_account("ibkr-profile")
        .expect("created ibkr account should be searchable");
    assert_eq!(created.account_type, AccountType::Primary);
    assert_eq!(created.broker_kind, BrokerKind::Ibkr);
    assert_eq!(created.broker_account_id.as_deref(), Some("U1234567"));

    let parent_id = created.id.to_string();
    let earnings = account_matches(&[
        "--confirm-protected",
        "secret",
        "--name",
        "ibkr-earnings",
        "--description",
        "ibkr earnings",
        "--environment",
        "paper",
        "--taxes",
        "20",
        "--earnings",
        "10",
        "--type",
        "earnings",
        "--broker",
        "ibkr",
        "--parent",
        &parent_id,
    ]);
    crate::dispatcher::accounts::create_account(&mut dispatcher.trust, &earnings)
        .expect("non-primary ibkr account with parent does not require broker account id");
    let earnings = dispatcher
        .trust
        .search_account("ibkr-earnings")
        .expect("created earnings account should be searchable");
    assert_eq!(earnings.account_type, AccountType::Earnings);
    assert_eq!(earnings.broker_kind, BrokerKind::Ibkr);
    assert!(earnings.broker_account_id.is_none());
    assert_eq!(earnings.parent_account_id, Some(created.id));

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_create_account_non_interactive_success_with_hierarchy() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();

    let parent = dispatcher
        .trust
        .create_account("parent", "root", Environment::Paper, dec!(15), dec!(5))
        .expect("create parent account");

    let matches = account_matches(&[
        "--confirm-protected",
        "secret",
        "--name",
        "child",
        "--description",
        "child account",
        "--environment",
        "paper",
        "--taxes",
        "20",
        "--earnings",
        "10",
        "--type",
        "reinvestment",
        "--parent",
        &parent.id.to_string(),
    ]);

    assert!(
        crate::dispatcher::accounts::create_account(&mut dispatcher.trust, &matches).is_ok(),
        "non-interactive create should succeed"
    );

    let created = dispatcher
        .trust
        .search_account("child")
        .expect("find created child account");
    assert_eq!(created.account_type, AccountType::Reinvestment);
    assert_eq!(created.parent_account_id, Some(parent.id));
    assert_eq!(created.environment, Environment::Paper);

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_create_account_non_interactive_surfaces_core_create_errors() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();

    let args = [
        "--confirm-protected",
        "secret",
        "--name",
        "duplicate-account",
        "--description",
        "original",
        "--environment",
        "paper",
        "--taxes",
        "20",
        "--earnings",
        "10",
    ];

    let first = account_matches(&args);
    crate::dispatcher::accounts::create_account(&mut dispatcher.trust, &first)
        .expect("first account creation should succeed");

    let duplicate = account_matches(&args);
    let error = crate::dispatcher::accounts::create_account(&mut dispatcher.trust, &duplicate)
        .expect_err("duplicate account creation should fail");
    assert!(error.to_string().contains("create_account_failed"));

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_delete_account_requires_protected_keyword_and_deletes_zero_balance_account() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();
    let account = dispatcher
        .trust
        .create_account(
            "delete-cli",
            "delete",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account should be created");
    let account_id = account.id.to_string();

    let missing = account_matches(&["--account", &account_id]);
    let error = crate::dispatcher::accounts::delete_account(&mut dispatcher.trust, &missing)
        .expect_err("protected keyword should be required");
    assert!(error.to_string().contains("protected_keyword_required"));

    let invalid = account_matches(&[
        "--account",
        &account_id,
        "--confirm-protected",
        "wrong-secret",
    ]);
    let error = crate::dispatcher::accounts::delete_account(&mut dispatcher.trust, &invalid)
        .expect_err("invalid protected keyword should fail");
    assert!(error.to_string().contains("protected_keyword_invalid"));

    let valid = account_matches(&["--account", &account_id, "--confirm-protected", "secret"]);
    crate::dispatcher::accounts::delete_account(&mut dispatcher.trust, &valid)
        .expect("account deletion should succeed");
    assert!(dispatcher.trust.search_account("delete-cli").is_err());
    assert!(dispatcher
        .trust
        .search_all_accounts()
        .expect("accounts should load")
        .is_empty());

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_delete_account_rejects_non_zero_balance_unless_forced() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();
    let account = dispatcher
        .trust
        .create_account(
            "delete-balance-cli",
            "delete",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account should be created");
    dispatcher
        .trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(50),
            &Currency::USD,
        )
        .expect("deposit should create non-zero balance");
    let account_id = account.id.to_string();

    let safe_delete = account_matches(&["--account", &account_id, "--confirm-protected", "secret"]);
    let error = crate::dispatcher::accounts::delete_account(&mut dispatcher.trust, &safe_delete)
        .expect_err("non-zero account should require force");
    assert!(error.to_string().contains("non-zero balance"));

    let forced = account_matches(&[
        "--account",
        &account_id,
        "--force",
        "--confirm-protected",
        "secret",
    ]);
    crate::dispatcher::accounts::delete_account(&mut dispatcher.trust, &forced)
        .expect("force should bypass zero-balance check");
    assert!(dispatcher
        .trust
        .search_account("delete-balance-cli")
        .is_err());

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_delete_account_rejects_open_trades() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();
    let (account, _trade) = seed_new_trade(&mut dispatcher);
    let account_id = account.id.to_string();
    let matches = account_matches(&[
        "--account",
        &account_id,
        "--force",
        "--confirm-protected",
        "secret",
    ]);

    let error = crate::dispatcher::accounts::delete_account(&mut dispatcher.trust, &matches)
        .expect_err("account with open trades should not delete");
    assert!(error.to_string().contains("open trade"));

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}
