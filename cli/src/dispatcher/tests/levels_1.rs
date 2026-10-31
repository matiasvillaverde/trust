#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::LevelCommandBuilder;
use crate::dispatcher::output::ReportOutputFormat;
use clap::Arg;
use clap::Command;
use core::services::leveling::LevelPerformanceSnapshot;
use model::Environment;
use rust_decimal_macros::dec;
use uuid::Uuid;

#[test]
fn test_level_dispatch_text_success_paths_cover_read_and_mutation_handlers() {
    let _guard = env_guard();
    std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    crate::protected_keyword::delete().expect("reset protected keyword");

    let mut dispatcher = test_dispatcher();
    let account = dispatcher
        .trust
        .create_account(
            "level-text",
            "level text account",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");
    let account_id = account.id.to_string();

    let triggers = LevelCommandBuilder::new()
        .triggers()
        .build()
        .get_matches_from(["level", "triggers"]);
    crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &triggers)
        .expect("triggers text should succeed");

    let status = LevelCommandBuilder::new()
        .status()
        .build()
        .get_matches_from(["level", "status", "--account", &account_id]);
    crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &status)
        .expect("status text should succeed");

    let rules_show = LevelCommandBuilder::new()
        .rules()
        .build()
        .get_matches_from(["level", "rules", "show", "--account", &account_id]);
    crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &rules_show)
        .expect("rules show text should succeed");

    let rules_set = LevelCommandBuilder::new()
        .rules()
        .build()
        .get_matches_from([
            "level",
            "rules",
            "set",
            "--account",
            &account_id,
            "--rule",
            "max_changes_in_30_days",
            "--value",
            "4",
            "--confirm-protected",
            "secret",
        ]);
    crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &rules_set)
        .expect("rules set text should succeed");
    let updated_rules = dispatcher
        .trust
        .level_adjustment_rules_for_account(account.id)
        .expect("updated rules should load");
    assert_eq!(updated_rules.max_changes_in_30_days, 4);

    let change = LevelCommandBuilder::new()
        .change()
        .build()
        .get_matches_from([
            "level",
            "change",
            "--account",
            &account_id,
            "--to",
            "2",
            "--reason",
            "manual test change",
            "--trigger",
            "manual_review",
            "--confirm-protected",
            "secret",
        ]);
    crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &change)
        .expect("level change text should succeed");

    let evaluate = LevelCommandBuilder::new()
        .evaluate()
        .build()
        .get_matches_from([
            "level",
            "evaluate",
            "--account",
            &account_id,
            "--profitable-trades",
            "8",
            "--win-rate",
            "80",
            "--monthly-loss",
            "0",
            "--largest-loss",
            "0",
            "--consecutive-wins",
            "4",
            "--apply",
            "--confirm-protected",
            "secret",
        ]);
    crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &evaluate)
        .expect("level evaluate text should succeed");

    let progress = LevelCommandBuilder::new()
        .progress()
        .build()
        .get_matches_from([
            "level",
            "progress",
            "--account",
            &account_id,
            "--profitable-trades",
            "2",
            "--win-rate",
            "50",
            "--monthly-loss",
            "1",
            "--largest-loss",
            "1",
            "--consecutive-wins",
            "1",
        ]);
    crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &progress)
        .expect("level progress text should succeed");

    crate::protected_keyword::delete().expect("cleanup protected keyword");
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
}

#[test]
fn test_level_handlers_surface_core_errors_and_required_field_validation() {
    let _guard = env_guard();
    std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    crate::protected_keyword::delete().expect("reset protected keyword");

    let mut dispatcher = test_dispatcher();
    let account = dispatcher
        .trust
        .create_account(
            "level-errors",
            "level error account",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");
    let account_id = account.id.to_string();
    let missing_account_id = Uuid::new_v4().to_string();

    let status = LevelCommandBuilder::new()
        .status()
        .build()
        .get_matches_from(["level", "status", "--account", &missing_account_id]);
    let error = crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &status)
        .expect_err("unknown account level status should fail");
    assert!(error.to_string().contains("level_status_failed"));

    let rules_show = LevelCommandBuilder::new()
        .rules()
        .build()
        .get_matches_from(["level", "rules", "show", "--account", &missing_account_id]);
    let error = crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &rules_show)
        .expect_err("unknown account rules show should fail");
    assert!(error.to_string().contains("level_rules_show_failed"));

    let rules_set = LevelCommandBuilder::new()
        .rules()
        .build()
        .get_matches_from([
            "level",
            "rules",
            "set",
            "--account",
            &missing_account_id,
            "--rule",
            "monthly_loss_downgrade_pct",
            "--value",
            "8",
            "--confirm-protected",
            "secret",
        ]);
    let error = crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &rules_set)
        .expect_err("unknown account rules set should fail while reading current rules");
    assert!(error.to_string().contains("level_rules_read_failed"));

    let progress = LevelCommandBuilder::new()
        .progress()
        .build()
        .get_matches_from([
            "level",
            "progress",
            "--account",
            &missing_account_id,
            "--profitable-trades",
            "1",
            "--win-rate",
            "50",
            "--monthly-loss",
            "0",
            "--largest-loss",
            "0",
        ]);
    let error = crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &progress)
        .expect_err("unknown account progress should fail");
    assert!(error.to_string().contains("level_progress_failed"));

    let evaluate = LevelCommandBuilder::new()
        .evaluate()
        .build()
        .get_matches_from([
            "level",
            "evaluate",
            "--account",
            &missing_account_id,
            "--profitable-trades",
            "1",
            "--win-rate",
            "50",
            "--monthly-loss",
            "0",
            "--largest-loss",
            "0",
        ]);
    let error = crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &evaluate)
        .expect_err("unknown account evaluate should fail");
    assert!(error.to_string().contains("level_evaluation_failed"));

    let level_change_matches = |args: &[&str]| {
        let mut argv = vec!["level-change"];
        argv.extend_from_slice(args);
        Command::new("level-change")
            .arg(Arg::new("account").long("account"))
            .arg(
                Arg::new("to")
                    .long("to")
                    .value_parser(clap::value_parser!(u8)),
            )
            .arg(Arg::new("reason").long("reason"))
            .arg(Arg::new("trigger").long("trigger"))
            .arg(Arg::new("confirm-protected").long("confirm-protected"))
            .get_matches_from(argv)
    };

    let missing_target = level_change_matches(&[
        "--account",
        &account_id,
        "--reason",
        "missing target",
        "--trigger",
        "manual_review",
        "--confirm-protected",
        "secret",
    ]);
    let error = crate::dispatcher::levels::level_change(
        &mut dispatcher.trust,
        &missing_target,
        ReportOutputFormat::Json,
    )
    .expect_err("missing target level should fail");
    assert!(error.to_string().contains("missing_level"));

    let invalid_trigger = level_change_matches(&[
        "--account",
        &account_id,
        "--to",
        "2",
        "--reason",
        "blank trigger",
        "--trigger",
        "   ",
        "--confirm-protected",
        "secret",
    ]);
    let error = crate::dispatcher::levels::level_change(
        &mut dispatcher.trust,
        &invalid_trigger,
        ReportOutputFormat::Json,
    )
    .expect_err("blank trigger should fail");
    assert!(error.to_string().contains("invalid_trigger"));

    let invalid_level = level_change_matches(&[
        "--account",
        &account_id,
        "--to",
        "9",
        "--reason",
        "bad target",
        "--trigger",
        "manual_review",
        "--confirm-protected",
        "secret",
    ]);
    let error = crate::dispatcher::levels::level_change(
        &mut dispatcher.trust,
        &invalid_level,
        ReportOutputFormat::Json,
    )
    .expect_err("out of range target should fail in core");
    assert!(error.to_string().contains("level_change_failed"));

    crate::protected_keyword::delete().expect("cleanup protected keyword");
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
}

#[test]
fn test_level_dispatchers_surface_level_store_failures() {
    let _guard = env_guard();
    std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    crate::protected_keyword::delete().expect("reset protected keyword");

    let account_id = Uuid::new_v4().to_string();
    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::levels());

    let status = LevelCommandBuilder::new()
        .status()
        .build()
        .get_matches_from(["level", "status", "--account", &account_id]);
    let status_error = crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &status)
        .expect_err("level status read failure should surface");
    assert_eq!(status_error.code, "level_status_failed");

    let history = LevelCommandBuilder::new()
        .history()
        .build()
        .get_matches_from(["level", "history", "--account", &account_id]);
    let history_error = crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &history)
        .expect_err("level history read failure should surface");
    assert_eq!(history_error.code, "level_history_failed");

    let rules_show = LevelCommandBuilder::new()
        .rules()
        .build()
        .get_matches_from(["level", "rules", "show", "--account", &account_id]);
    let rules_show_error =
        crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &rules_show)
            .expect_err("level rules read failure should surface");
    assert_eq!(rules_show_error.code, "level_rules_show_failed");

    let rules_set = LevelCommandBuilder::new()
        .rules()
        .build()
        .get_matches_from([
            "level",
            "rules",
            "set",
            "--account",
            &account_id,
            "--rule",
            "monthly_loss_downgrade_pct",
            "--value",
            "8",
            "--confirm-protected",
            "secret",
        ]);
    let rules_set_read_error =
        crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &rules_set)
            .expect_err("level rules read failure should surface before update");
    assert_eq!(rules_set_read_error.code, "level_rules_read_failed");

    let mut dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::level_write_after_read(),
    );
    let rules_set_write_error =
        crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &rules_set)
            .expect_err("level rules write failure should surface");
    assert_eq!(rules_set_write_error.code, "level_rules_set_failed");

    crate::protected_keyword::delete().expect("cleanup protected keyword");
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
}

#[test]
fn test_resolve_level_account_id_paths() {
    let mut dispatcher = test_dispatcher();

    let invalid_matches = Command::new("level")
        .arg(Arg::new("account").long("account"))
        .get_matches_from(["level", "--account", "not-a-uuid"]);
    let invalid_error = crate::dispatcher::levels::resolve_level_account_id(
        &mut dispatcher.trust,
        &invalid_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid account id should fail");
    assert!(invalid_error.to_string().contains("invalid_account_id"));

    let account = dispatcher
        .trust
        .create_account(
            "solo",
            "single account",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");
    let no_arg_matches = Command::new("level")
        .arg(Arg::new("account").long("account"))
        .get_matches_from(["level"]);
    let inferred = crate::dispatcher::levels::resolve_level_account_id(
        &mut dispatcher.trust,
        &no_arg_matches,
        ReportOutputFormat::Json,
    )
    .expect("single account should be inferred");
    assert_eq!(inferred, account.id);

    dispatcher
        .trust
        .create_account(
            "second",
            "second account",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create second account");
    let requires_selection = crate::dispatcher::levels::resolve_level_account_id(
        &mut dispatcher.trust,
        &no_arg_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("multiple accounts should require explicit selection");
    assert!(requires_selection
        .to_string()
        .contains("account_selection_required"));
}

#[test]
fn test_level_snapshot_from_args_validates_required_and_decimal_fields() {
    let valid = level_eval_matches(&[
        "--profitable-trades",
        "7",
        "--win-rate",
        "62.5",
        "--monthly-loss",
        "4.2",
        "--largest-loss",
        "1.1",
        "--consecutive-wins",
        "3",
    ]);
    let snapshot =
        crate::dispatcher::levels::level_snapshot_from_args(&valid, ReportOutputFormat::Json)
            .expect("valid snapshot should parse");
    let expected = LevelPerformanceSnapshot {
        profitable_trades: 7,
        win_rate_percentage: dec!(62.5),
        monthly_loss_percentage: dec!(4.2),
        largest_loss_percentage: dec!(1.1),
        consecutive_wins: 3,
    };
    assert_eq!(snapshot.profitable_trades, expected.profitable_trades);
    assert_eq!(snapshot.win_rate_percentage, expected.win_rate_percentage);
    assert_eq!(
        snapshot.monthly_loss_percentage,
        expected.monthly_loss_percentage
    );
    assert_eq!(
        snapshot.largest_loss_percentage,
        expected.largest_loss_percentage
    );
    assert_eq!(snapshot.consecutive_wins, expected.consecutive_wins);

    let missing_profitable = level_eval_matches(&[
        "--win-rate",
        "60",
        "--monthly-loss",
        "2",
        "--largest-loss",
        "1",
        "--consecutive-wins",
        "2",
    ]);
    let missing_profitable_error = crate::dispatcher::levels::level_snapshot_from_args(
        &missing_profitable,
        ReportOutputFormat::Json,
    )
    .expect_err("missing profitable trades should fail");
    assert!(missing_profitable_error
        .to_string()
        .contains("missing_profitable_trades"));

    let missing_consecutive = level_eval_matches(&[
        "--profitable-trades",
        "2",
        "--win-rate",
        "60",
        "--monthly-loss",
        "2",
        "--largest-loss",
        "1",
    ]);
    let missing_consecutive_error = crate::dispatcher::levels::level_snapshot_from_args(
        &missing_consecutive,
        ReportOutputFormat::Json,
    )
    .expect_err("missing consecutive wins should fail");
    assert!(missing_consecutive_error
        .to_string()
        .contains("missing_consecutive_wins"));

    let invalid_decimal = level_eval_matches(&[
        "--profitable-trades",
        "2",
        "--win-rate",
        "bad",
        "--monthly-loss",
        "2",
        "--largest-loss",
        "1",
        "--consecutive-wins",
        "2",
    ]);
    let invalid_decimal_error = crate::dispatcher::levels::level_snapshot_from_args(
        &invalid_decimal,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid decimal should fail");
    assert!(invalid_decimal_error
        .to_string()
        .contains("invalid_decimal"));

    let invalid_monthly_loss = level_eval_matches(&[
        "--profitable-trades",
        "2",
        "--win-rate",
        "60",
        "--monthly-loss",
        "bad",
        "--largest-loss",
        "1",
        "--consecutive-wins",
        "2",
    ]);
    let invalid_monthly_loss_error = crate::dispatcher::levels::level_snapshot_from_args(
        &invalid_monthly_loss,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid monthly loss should fail");
    assert!(invalid_monthly_loss_error
        .to_string()
        .contains("invalid_decimal"));

    let invalid_largest_loss = level_eval_matches(&[
        "--profitable-trades",
        "2",
        "--win-rate",
        "60",
        "--monthly-loss",
        "2",
        "--largest-loss",
        "bad",
        "--consecutive-wins",
        "2",
    ]);
    let invalid_largest_loss_error = crate::dispatcher::levels::level_snapshot_from_args(
        &invalid_largest_loss,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid largest loss should fail");
    assert!(invalid_largest_loss_error
        .to_string()
        .contains("invalid_decimal"));
}
