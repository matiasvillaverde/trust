#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::TransactionCommandBuilder;
use model::Currency;
use model::Environment;
use rust_decimal_macros::dec;

#[test]
fn test_transaction_non_interactive_arg_detection_helper() {
    let matches = TransactionCommandBuilder::new()
        .deposit()
        .build()
        .get_matches_from(["transaction", "deposit", "--account", "main"]);
    let sub_matches = matches
        .subcommand_matches("deposit")
        .expect("deposit subcommand");
    assert!(crate::dispatcher::capital::has_non_interactive_transaction_args(sub_matches));

    let empty = TransactionCommandBuilder::new()
        .deposit()
        .build()
        .get_matches_from(["transaction", "deposit"]);
    let empty_sub = empty
        .subcommand_matches("deposit")
        .expect("deposit subcommand");
    assert!(!crate::dispatcher::capital::has_non_interactive_transaction_args(empty_sub));
}

#[test]
fn test_dispatch_transactions_non_interactive_success_and_validation_errors() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();

    let account = dispatcher
        .trust
        .create_account("tx-account", "tx", Environment::Paper, dec!(20), dec!(10))
        .expect("create account");

    let deposit = TransactionCommandBuilder::new()
        .deposit()
        .build()
        .get_matches_from([
            "transaction",
            "deposit",
            "--account",
            &account.name,
            "--currency",
            "usd",
            "--amount",
            "100.50",
            "--confirm-protected",
            "secret",
        ]);
    assert!(
        crate::dispatcher::capital::dispatch_transactions(&mut dispatcher.trust, &deposit).is_ok(),
        "non-interactive deposit should succeed"
    );

    let after_deposit = dispatcher
        .trust
        .search_balance(account.id, &Currency::USD)
        .expect("balance after deposit");
    assert_eq!(after_deposit.total_balance, dec!(100.50));
    assert_eq!(after_deposit.total_available, dec!(100.50));

    let withdraw = TransactionCommandBuilder::new()
        .withdraw()
        .build()
        .get_matches_from([
            "transaction",
            "withdraw",
            "--account",
            &account.name,
            "--currency",
            "USD",
            "--amount",
            "40.25",
            "--confirm-protected",
            "secret",
        ]);
    assert!(
        crate::dispatcher::capital::dispatch_transactions(&mut dispatcher.trust, &withdraw).is_ok(),
        "non-interactive withdrawal should succeed"
    );

    let after_withdraw = dispatcher
        .trust
        .search_balance(account.id, &Currency::USD)
        .expect("balance after withdrawal");
    assert_eq!(after_withdraw.total_balance, dec!(60.25));
    assert_eq!(after_withdraw.total_available, dec!(60.25));

    let invalid_currency = TransactionCommandBuilder::new()
        .deposit()
        .build()
        .get_matches_from([
            "transaction",
            "deposit",
            "--account",
            &account.name,
            "--currency",
            "JPY",
            "--amount",
            "1",
            "--confirm-protected",
            "secret",
        ]);
    let currency_error =
        crate::dispatcher::capital::dispatch_transactions(&mut dispatcher.trust, &invalid_currency)
            .expect_err("unsupported currency should fail");
    assert!(currency_error.to_string().contains("invalid_currency"));

    let invalid_amount = TransactionCommandBuilder::new()
        .deposit()
        .build()
        .get_matches_from([
            "transaction",
            "deposit",
            "--account",
            &account.name,
            "--currency",
            "USD",
            "--amount",
            "abc",
            "--confirm-protected",
            "secret",
        ]);
    let amount_error =
        crate::dispatcher::capital::dispatch_transactions(&mut dispatcher.trust, &invalid_amount)
            .expect_err("invalid decimal should fail");
    assert!(amount_error.to_string().contains("invalid_decimal"));

    let missing_account = TransactionCommandBuilder::new()
        .deposit()
        .build()
        .get_matches_from([
            "transaction",
            "deposit",
            "--currency",
            "USD",
            "--amount",
            "1",
            "--confirm-protected",
            "secret",
        ]);
    let missing_account_error =
        crate::dispatcher::capital::dispatch_transactions(&mut dispatcher.trust, &missing_account)
            .expect_err("missing account should fail");
    assert!(missing_account_error
        .to_string()
        .contains("missing_argument"));

    let missing_currency = TransactionCommandBuilder::new()
        .deposit()
        .build()
        .get_matches_from([
            "transaction",
            "deposit",
            "--account",
            &account.name,
            "--amount",
            "1",
            "--confirm-protected",
            "secret",
        ]);
    let missing_currency_error =
        crate::dispatcher::capital::dispatch_transactions(&mut dispatcher.trust, &missing_currency)
            .expect_err("missing currency should fail");
    assert!(missing_currency_error
        .to_string()
        .contains("missing_argument"));

    let missing_amount = TransactionCommandBuilder::new()
        .deposit()
        .build()
        .get_matches_from([
            "transaction",
            "deposit",
            "--account",
            &account.name,
            "--currency",
            "USD",
            "--confirm-protected",
            "secret",
        ]);
    let missing_amount_error =
        crate::dispatcher::capital::dispatch_transactions(&mut dispatcher.trust, &missing_amount)
            .expect_err("missing amount should fail");
    assert!(missing_amount_error
        .to_string()
        .contains("missing_argument"));

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_dispatch_transactions_interactive_deposit_and_withdraw_use_scripted_dialog_io() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    crate::dialogs::scripted_reset();
    let mut dispatcher = test_dispatcher();

    let account = dispatcher
        .trust
        .create_account(
            "interactive-tx",
            "tx",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");

    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_input(Ok("125".to_string()));
    let deposit = TransactionCommandBuilder::new()
        .deposit()
        .build()
        .get_matches_from(["transaction", "deposit", "--confirm-protected", "secret"]);
    crate::dispatcher::capital::dispatch_transactions(&mut dispatcher.trust, &deposit)
        .expect("scripted interactive deposit should dispatch");

    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_input(Ok("25".to_string()));
    let withdraw = TransactionCommandBuilder::new()
        .withdraw()
        .build()
        .get_matches_from(["transaction", "withdraw", "--confirm-protected", "secret"]);
    crate::dispatcher::capital::dispatch_transactions(&mut dispatcher.trust, &withdraw)
        .expect("scripted interactive withdrawal should dispatch");

    let balance = dispatcher
        .trust
        .search_balance(account.id, &Currency::USD)
        .expect("balance after scripted transactions");
    assert_eq!(balance.total_balance, dec!(100));
    assert_eq!(balance.total_available, dec!(100));

    crate::dialogs::scripted_reset();
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}
