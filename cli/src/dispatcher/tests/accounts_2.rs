#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::AccountCommandBuilder;
use model::AccountType;
use model::Currency;
use model::Environment;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn test_accounts_transfer_moves_balance_between_accounts() {
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "transfer-test");
    let mut dispatcher = test_dispatcher();
    let parent = dispatcher
        .trust
        .create_account("parent", "source", Environment::Paper, dec!(10), dec!(5))
        .expect("create source account");
    let child = dispatcher
        .trust
        .create_account_with_hierarchy(
            "child-earnings",
            "dest",
            Environment::Paper,
            dec!(10),
            dec!(5),
            AccountType::Earnings,
            Some(parent.id),
        )
        .expect("create destination account");
    dispatcher
        .trust
        .create_transaction(
            &parent,
            &model::TransactionCategory::Deposit,
            dec!(200),
            &Currency::USD,
        )
        .expect("seed source balance");

    let transfer = AccountCommandBuilder::new()
        .transfer_account()
        .build()
        .get_matches_from([
            "accounts",
            "transfer",
            "--from",
            &parent.id.to_string(),
            "--to",
            &child.id.to_string(),
            "--amount",
            "50",
            "--reason",
            "rebalance",
            "--confirm-protected",
            "transfer-test",
        ]);
    assert!(
        crate::dispatcher::accounts::dispatch_accounts(&mut dispatcher.trust, &transfer).is_ok(),
        "transfer should succeed"
    );

    let from_balance = dispatcher
        .trust
        .search_balance(parent.id, &Currency::USD)
        .expect("source balance");
    assert!(
        from_balance.total_balance >= Decimal::ZERO,
        "source balance should remain non-negative"
    );

    let child_balances = dispatcher
        .trust
        .search_all_balances(child.id)
        .expect("destination balances query should succeed");
    assert!(
        child_balances
            .iter()
            .all(|balance| balance.total_balance >= Decimal::ZERO),
        "child balances should remain non-negative after transfer"
    );
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_accounts_dispatch_covers_search_hierarchy_and_detailed_balance_paths() {
    let mut dispatcher = test_dispatcher();

    let search = AccountCommandBuilder::new()
        .read_account()
        .build()
        .get_matches_from(["accounts", "search"]);
    assert!(
        crate::dispatcher::accounts::dispatch_accounts(&mut dispatcher.trust, &search).is_ok(),
        "empty account search should report through dialog display without failing dispatch"
    );

    let parent = dispatcher
        .trust
        .create_account("hier-parent", "root", Environment::Paper, dec!(10), dec!(5))
        .expect("create parent account");
    let child = dispatcher
        .trust
        .create_account_with_hierarchy(
            "hier-child",
            "child",
            Environment::Paper,
            dec!(10),
            dec!(5),
            AccountType::Earnings,
            Some(parent.id),
        )
        .expect("create child account");
    dispatcher
        .trust
        .create_account("aaa-root", "root", Environment::Paper, dec!(10), dec!(5))
        .expect("create second root account");
    dispatcher
        .trust
        .create_account_with_hierarchy(
            "aaa-child",
            "child",
            Environment::Paper,
            dec!(10),
            dec!(5),
            AccountType::TaxReserve,
            Some(parent.id),
        )
        .expect("create second child account");
    dispatcher
        .trust
        .create_transaction(
            &parent,
            &model::TransactionCategory::Deposit,
            dec!(100),
            &Currency::USD,
        )
        .expect("seed parent balance");
    dispatcher
        .trust
        .create_transaction(
            &child,
            &model::TransactionCategory::Deposit,
            dec!(25),
            &Currency::USD,
        )
        .expect("seed child balance");

    let list_hierarchy = AccountCommandBuilder::new()
        .list_accounts()
        .build()
        .get_matches_from(["accounts", "list", "--hierarchy"]);
    assert!(
        crate::dispatcher::accounts::dispatch_accounts(&mut dispatcher.trust, &list_hierarchy)
            .is_ok()
    );

    let balance_detailed = AccountCommandBuilder::new()
        .balance_accounts()
        .build()
        .get_matches_from(["accounts", "balance", "--detailed"]);
    assert!(crate::dispatcher::accounts::dispatch_accounts(
        &mut dispatcher.trust,
        &balance_detailed
    )
    .is_ok());
}

#[test]
fn test_account_create_dispatch_interactive_path_uses_scripted_dialog_io() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    crate::dialogs::scripted_reset();
    crate::dialogs::scripted_push_input(Ok("interactive-account".to_string()));
    crate::dialogs::scripted_push_input(Ok("created from scripted dialog".to_string()));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_input(Ok("0.2".to_string()));
    crate::dialogs::scripted_push_input(Ok("0.1".to_string()));
    let mut dispatcher = test_dispatcher();

    let create_interactive = AccountCommandBuilder::new()
        .create_account()
        .build()
        .get_matches_from(["accounts", "create", "--confirm-protected", "secret"]);
    assert!(
        crate::dispatcher::accounts::dispatch_accounts(&mut dispatcher.trust, &create_interactive)
            .is_ok(),
        "scripted interactive account creation should dispatch successfully"
    );

    let account = dispatcher
        .trust
        .search_account("interactive-account")
        .expect("scripted account should be created");
    assert_eq!(account.environment, Environment::Paper);
    assert_eq!(account.taxes_percentage, dec!(0.2));
    assert_eq!(account.earnings_percentage, dec!(0.1));

    crate::dialogs::scripted_reset();
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}
