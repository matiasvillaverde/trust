#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::RuleCommandBuilder;
use clap::Arg;
use model::Environment;
use rust_decimal_macros::dec;

#[test]
fn test_rule_remove_dispatch_validates_protected_keyword_before_dialogs() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();

    let remove_rule = RuleCommandBuilder::new()
        .remove_rule()
        .build()
        .arg(Arg::new("format").long("format"))
        .get_matches_from(["rule", "remove", "--confirm-protected", "wrong"]);
    let error = crate::dispatcher::rules::dispatch_rules(&mut dispatcher.trust, &remove_rule)
        .expect_err("wrong protected keyword should fail before rule removal dialog");

    assert!(error.to_string().contains("protected_keyword_invalid"));
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_rule_dispatch_interactive_create_and_remove_use_scripted_dialog_io() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    crate::dialogs::scripted_reset();
    let mut dispatcher = test_dispatcher();
    let account = dispatcher
        .trust
        .create_account(
            "interactive-rule",
            "rules",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");

    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_input(Ok("2.5".to_string()));
    crate::dialogs::scripted_push_input(Ok("scripted risk rule".to_string()));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    let create_rule = RuleCommandBuilder::new()
        .create_rule()
        .build()
        .arg(Arg::new("format").long("format"))
        .get_matches_from(["rule", "create", "--confirm-protected", "secret"]);
    crate::dispatcher::rules::dispatch_rules(&mut dispatcher.trust, &create_rule)
        .expect("scripted rule creation should dispatch");

    let rules = dispatcher
        .trust
        .search_all_rules(account.id)
        .expect("created rule should be readable");
    assert_eq!(rules.len(), 1);
    assert!(rules[0].active);

    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    let remove_rule = RuleCommandBuilder::new()
        .remove_rule()
        .build()
        .arg(Arg::new("format").long("format"))
        .get_matches_from(["rule", "remove", "--confirm-protected", "secret"]);
    crate::dispatcher::rules::dispatch_rules(&mut dispatcher.trust, &remove_rule)
        .expect("scripted rule removal should dispatch");

    let active_rules = dispatcher
        .trust
        .search_all_rules(account.id)
        .expect("rules should remain readable");
    assert!(active_rules.is_empty());

    crate::dialogs::scripted_reset();
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}
