#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::DistributionCommandBuilder;
use model::AccountType;
use model::Currency;
use model::Environment;
use model::TransactionCategory;
use rust_decimal_macros::dec;

#[test]
fn test_distribution_and_transfer_commands_validate_inputs_and_defaults() {
    let mut dispatcher = test_dispatcher();
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);

    let configure_bad_uuid = distribution_config_matches(&[
        "--account-id",
        "bad",
        "--earnings",
        "10",
        "--tax",
        "20",
        "--reinvestment",
        "70",
        "--threshold",
        "100",
        "--password",
        "pw",
    ]);
    let error = crate::dispatcher::distribution::configure_distribution(
        &mut dispatcher.trust,
        &configure_bad_uuid,
    )
    .expect_err("invalid uuid should fail");
    assert!(error.to_string().contains("invalid_uuid"));

    let configure_bad_decimal = distribution_config_matches(&[
        "--account-id",
        &account.id.to_string(),
        "--earnings",
        "bad",
        "--tax",
        "20",
        "--reinvestment",
        "70",
        "--threshold",
        "100",
        "--password",
        "pw",
    ]);
    let error = crate::dispatcher::distribution::configure_distribution(
        &mut dispatcher.trust,
        &configure_bad_decimal,
    )
    .expect_err("invalid decimal should fail");
    assert!(error.to_string().contains("invalid_decimal"));

    let execute_bad_amount =
        distribution_execute_matches(&["--account-id", &account.id.to_string(), "--amount", "bad"]);
    let error = crate::dispatcher::distribution::execute_distribution(
        &mut dispatcher.trust,
        &execute_bad_amount,
    )
    .expect_err("invalid amount should fail");
    assert!(error.to_string().contains("invalid_decimal"));

    let history_default_limit =
        distribution_history_matches(&["--account-id", &account.id.to_string(), "--limit", "bad"]);
    crate::dispatcher::distribution::distribution_history(
        &mut dispatcher.trust,
        &history_default_limit,
    )
    .expect("history should default to 20 for invalid limit");

    let rules_bad_uuid = distribution_rules_matches(&["--account-id", "bad"]);
    let error =
        crate::dispatcher::distribution::distribution_rules(&mut dispatcher.trust, &rules_bad_uuid)
            .expect_err("invalid uuid should fail");
    assert!(error.to_string().contains("invalid_uuid"));

    let transfer_bad_from = transfer_matches(&[
        "--from",
        "bad",
        "--to",
        &account.id.to_string(),
        "--amount",
        "10",
        "--reason",
        "rebalance",
    ]);
    let error =
        crate::dispatcher::accounts::transfer_accounts(&mut dispatcher.trust, &transfer_bad_from)
            .expect_err("invalid from uuid should fail");
    assert!(error.to_string().contains("invalid_uuid"));

    let transfer_bad_amount = transfer_matches(&[
        "--from",
        &account.id.to_string(),
        "--to",
        &account.id.to_string(),
        "--amount",
        "bad",
        "--reason",
        "rebalance",
    ]);
    let error =
        crate::dispatcher::accounts::transfer_accounts(&mut dispatcher.trust, &transfer_bad_amount)
            .expect_err("invalid amount should fail");
    assert!(error.to_string().contains("invalid_decimal"));
}

#[test]
fn test_distribution_dispatch_supports_legacy_rules_route() {
    let mut dispatcher = test_dispatcher();
    let legacy_rules = DistributionCommandBuilder::new()
        .show_rules()
        .build()
        .get_matches_from(["distribution", "rules", "--account-id", "not-a-uuid"]);

    let error = crate::dispatcher::distribution::dispatch_distribution(
        &mut dispatcher.trust,
        &legacy_rules,
    )
    .expect_err("legacy rules route should validate account id");

    assert!(error.to_string().contains("invalid_uuid"));
}

#[test]
fn test_distribution_dispatch_paths_cover_parse_and_service_edges() {
    let _guard = env_guard();
    std::env::set_var(
        "TRUST_PROTECTED_KEYWORD_EXPECTED",
        "distribution-test-keyword",
    );
    let mut dispatcher = test_dispatcher();
    let account = dispatcher
        .trust
        .create_account(
            "dist",
            "distribution",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create distribution account");

    let configure = DistributionCommandBuilder::new()
        .configure_distribution()
        .build()
        .get_matches_from([
            "distribution",
            "configure",
            "--account-id",
            &account.id.to_string(),
            "--earnings",
            "40",
            "--tax",
            "30",
            "--reinvestment",
            "30",
            "--threshold",
            "100",
            "--password",
            "strong-password",
            "--confirm-protected",
            "distribution-test-keyword",
        ]);
    let _ =
        crate::dispatcher::distribution::dispatch_distribution(&mut dispatcher.trust, &configure);

    let execute_invalid_amount = DistributionCommandBuilder::new()
        .execute_distribution()
        .build()
        .get_matches_from([
            "distribution",
            "execute",
            "--account-id",
            &account.id.to_string(),
            "--amount",
            "abc",
            "--confirm-protected",
            "distribution-test-keyword",
        ]);
    let execute_error = crate::dispatcher::distribution::dispatch_distribution(
        &mut dispatcher.trust,
        &execute_invalid_amount,
    )
    .expect_err("invalid execute amount should fail");
    assert!(execute_error.to_string().contains("invalid_decimal"));

    let history = DistributionCommandBuilder::new()
        .history()
        .build()
        .get_matches_from([
            "distribution",
            "history",
            "--account-id",
            &account.id.to_string(),
            "--limit",
            "5",
        ]);
    assert!(
        crate::dispatcher::distribution::dispatch_distribution(&mut dispatcher.trust, &history)
            .is_ok(),
        "history should succeed for empty history"
    );

    let rules = DistributionCommandBuilder::new()
        .show_rules()
        .build()
        .get_matches_from([
            "distribution",
            "rules",
            "show",
            "--account-id",
            &account.id.to_string(),
        ]);
    let _ = crate::dispatcher::distribution::dispatch_distribution(&mut dispatcher.trust, &rules);
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_distribution_success_paths_execute_history_and_rules_output() {
    let mut dispatcher = test_dispatcher();
    let source = dispatcher
        .trust
        .create_account(
            "dist-source",
            "source",
            Environment::Paper,
            dec!(10),
            dec!(5),
        )
        .expect("create source account");
    dispatcher
        .trust
        .create_account_with_hierarchy(
            "dist-earnings",
            "earnings",
            Environment::Paper,
            dec!(10),
            dec!(5),
            AccountType::Earnings,
            Some(source.id),
        )
        .expect("create earnings account");
    dispatcher
        .trust
        .create_account_with_hierarchy(
            "dist-tax",
            "tax",
            Environment::Paper,
            dec!(10),
            dec!(5),
            AccountType::TaxReserve,
            Some(source.id),
        )
        .expect("create tax account");
    dispatcher
        .trust
        .create_account_with_hierarchy(
            "dist-reinvestment",
            "reinvestment",
            Environment::Paper,
            dec!(10),
            dec!(5),
            AccountType::Reinvestment,
            Some(source.id),
        )
        .expect("create reinvestment account");
    dispatcher
        .trust
        .create_account_with_hierarchy(
            "dist-insurance",
            "insurance",
            Environment::Paper,
            dec!(10),
            dec!(5),
            AccountType::Insurance,
            Some(source.id),
        )
        .expect("create insurance account");
    dispatcher
        .trust
        .create_transaction(
            &source,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .expect("seed distributable balance");

    let configure = distribution_config_matches(&[
        "--account-id",
        &source.id.to_string(),
        "--earnings",
        "40",
        "--tax",
        "30",
        "--reinvestment",
        "20",
        "--insurance",
        "10",
        "--threshold",
        "0",
        "--password",
        "strong-password",
    ]);
    crate::dispatcher::distribution::configure_distribution(&mut dispatcher.trust, &configure)
        .expect("distribution configuration should succeed");

    for amount in ["100", "50"] {
        let execute = distribution_execute_matches(&[
            "--account-id",
            &source.id.to_string(),
            "--amount",
            amount,
        ]);
        crate::dispatcher::distribution::execute_distribution(&mut dispatcher.trust, &execute)
            .expect("distribution execution should succeed");
    }

    let history =
        distribution_history_matches(&["--account-id", &source.id.to_string(), "--limit", "1"]);
    crate::dispatcher::distribution::distribution_history(&mut dispatcher.trust, &history)
        .expect("distribution history should render the truncated latest entry");

    let insurance = distribution_rules_matches(&["--account-id", &source.id.to_string()]);
    crate::dispatcher::distribution::distribution_insurance_status(
        &mut dispatcher.trust,
        &insurance,
    )
    .expect("insurance reserve status should render configured account");

    let rules = distribution_rules_matches(&["--account-id", &source.id.to_string()]);
    crate::dispatcher::distribution::distribution_rules(&mut dispatcher.trust, &rules)
        .expect("distribution rules should render configured values");
}
