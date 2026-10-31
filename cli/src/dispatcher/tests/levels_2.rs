#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::AdvisorCommandBuilder;
use crate::commands::KeysCommandBuilder;
use crate::commands::LevelCommandBuilder;
use model::Environment;
use rust_decimal_macros::dec;

#[test]
fn test_dispatch_level_advisor_and_keys_remaining_subcommands_are_reachable() {
    let _guard = env_guard();
    std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    crate::protected_keyword::delete().expect("reset protected keyword");

    let mut dispatcher = test_dispatcher();

    let level_history = LevelCommandBuilder::new()
        .history()
        .build()
        .get_matches_from(["level", "history", "--account", "not-a-uuid"]);
    let level_history_error =
        crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &level_history)
            .expect_err("history with invalid account should fail");
    assert!(level_history_error
        .to_string()
        .contains("invalid_account_id"));

    let level_change = LevelCommandBuilder::new()
        .change()
        .build()
        .get_matches_from([
            "level",
            "change",
            "--account",
            "not-a-uuid",
            "--to",
            "1",
            "--reason",
            "manual adjust",
            "--confirm-protected",
            "secret",
        ]);
    let level_change_error =
        crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &level_change)
            .expect_err("change with invalid account should fail");
    assert!(level_change_error
        .to_string()
        .contains("invalid_account_id"));

    let level_evaluate = LevelCommandBuilder::new()
        .evaluate()
        .build()
        .get_matches_from([
            "level",
            "evaluate",
            "--account",
            "not-a-uuid",
            "--profitable-trades",
            "2",
            "--win-rate",
            "60",
            "--monthly-loss",
            "3",
            "--largest-loss",
            "1",
        ]);
    let level_evaluate_error =
        crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &level_evaluate)
            .expect_err("evaluate with invalid account should fail");
    assert!(level_evaluate_error
        .to_string()
        .contains("invalid_account_id"));

    let level_progress = LevelCommandBuilder::new()
        .progress()
        .build()
        .get_matches_from([
            "level",
            "progress",
            "--account",
            "not-a-uuid",
            "--profitable-trades",
            "2",
            "--win-rate",
            "60",
            "--monthly-loss",
            "3",
            "--largest-loss",
            "1",
        ]);
    let level_progress_error =
        crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &level_progress)
            .expect_err("progress with invalid account should fail");
    assert!(level_progress_error
        .to_string()
        .contains("invalid_account_id"));

    let level_rules_show = LevelCommandBuilder::new()
        .rules()
        .build()
        .get_matches_from(["level", "rules", "show", "--account", "not-a-uuid"]);
    let level_rules_show_error =
        crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &level_rules_show)
            .expect_err("rules show with invalid account should fail");
    assert!(level_rules_show_error
        .to_string()
        .contains("invalid_account_id"));

    let level_rules_set = LevelCommandBuilder::new()
        .rules()
        .build()
        .get_matches_from([
            "level",
            "rules",
            "set",
            "--account",
            "not-a-uuid",
            "--rule",
            "monthly_loss_downgrade_pct",
            "--value",
            "8",
            "--confirm-protected",
            "secret",
        ]);
    let level_rules_set_error =
        crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &level_rules_set)
            .expect_err("rules set with invalid account should fail");
    assert!(level_rules_set_error
        .to_string()
        .contains("invalid_account_id"));

    let advisor_status = AdvisorCommandBuilder::new()
        .status()
        .build()
        .get_matches_from(["advisor", "status", "--account", "not-a-uuid"]);
    let advisor_status_error =
        crate::dispatcher::advisor_config::dispatch_advisor(&mut dispatcher.trust, &advisor_status)
            .expect_err("advisor status with invalid account should fail");
    assert!(advisor_status_error.to_string().contains("invalid_uuid"));

    let advisor_history = AdvisorCommandBuilder::new()
        .history()
        .build()
        .get_matches_from([
            "advisor",
            "history",
            "--account",
            "not-a-uuid",
            "--days",
            "14",
        ]);
    let advisor_history_error = crate::dispatcher::advisor_config::dispatch_advisor(
        &mut dispatcher.trust,
        &advisor_history,
    )
    .expect_err("advisor history with invalid account should fail");
    assert!(advisor_history_error.to_string().contains("invalid_uuid"));

    let advisor_account = dispatcher
        .trust
        .create_account(
            "advisor-warning-account",
            "advisory warnings",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("advisor account should be created");
    let advisor_check = AdvisorCommandBuilder::new()
        .check()
        .build()
        .get_matches_from([
            "advisor",
            "check",
            "--account",
            &advisor_account.id.to_string(),
            "--symbol",
            "AAPL",
            "--entry",
            "100",
            "--quantity",
            "10",
            "--sector",
            "technology",
            "--asset-class",
            "equity",
        ]);
    crate::dispatcher::advisor_config::dispatch_advisor(&mut dispatcher.trust, &advisor_check)
        .expect("advisor check should print warnings and recommendations");

    let keys_show = KeysCommandBuilder::new()
        .read_environment()
        .build()
        .get_matches_from(["keys", "show"]);
    let keys_show_outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = crate::dispatcher::keys::dispatch_keys(&mut dispatcher.trust, &keys_show);
    }));
    assert!(
        keys_show_outcome.is_err(),
        "keys show should reach dialog branch even when selection panics"
    );

    let keys_create = KeysCommandBuilder::new()
        .create_keys()
        .build()
        .get_matches_from(["keys", "create", "--confirm-protected", "secret"]);
    let keys_create_outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = crate::dispatcher::keys::dispatch_keys(&mut dispatcher.trust, &keys_create);
    }));
    assert!(
        keys_create_outcome.is_err(),
        "keys create should reach dialog branch even when account selection panics"
    );

    let keys_delete = KeysCommandBuilder::new()
        .delete_environment()
        .build()
        .get_matches_from(["keys", "delete", "--confirm-protected", "secret"]);
    let keys_delete_outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = crate::dispatcher::keys::dispatch_keys(&mut dispatcher.trust, &keys_delete);
    }));
    assert!(
        keys_delete_outcome.is_err(),
        "keys delete should reach dialog branch even when environment selection panics"
    );

    crate::protected_keyword::delete().expect("cleanup protected keyword");
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
}
