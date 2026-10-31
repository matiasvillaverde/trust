#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::broker::*;
use super::fixtures::*;
use crate::commands::GradeCommandBuilder;
use crate::dispatcher::output::ReportOutputFormat;
use clap::Arg;
use clap::Command;
use uuid::Uuid;

#[test]
fn test_grade_show_and_summary_cover_text_json_regrade_and_empty_paths() {
    let mut dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (account, closed) = seed_closed_target_trade(&mut dispatcher);
    let trade_id = closed.id.to_string();

    let show_matches = GradeCommandBuilder::new()
        .show()
        .build()
        .get_matches_from(["grade", "show", &trade_id]);
    let show = show_matches
        .subcommand_matches("show")
        .expect("show subcommand");
    crate::dispatcher::grades::grade_show(&mut dispatcher.trust, show, ReportOutputFormat::Text)
        .expect("grade show text should compute a grade");
    crate::dispatcher::grades::grade_show(&mut dispatcher.trust, show, ReportOutputFormat::Json)
        .expect("grade show json should reuse the existing grade");

    let mismatched_weights = GradeCommandBuilder::new().show().build().get_matches_from([
        "grade",
        "show",
        &trade_id,
        "--weights",
        "25,25,25,25",
    ]);
    let mismatch_error = crate::dispatcher::grades::grade_show(
        &mut dispatcher.trust,
        mismatched_weights
            .subcommand_matches("show")
            .expect("show subcommand"),
        ReportOutputFormat::Json,
    )
    .expect_err("mismatched weights should require --regrade");
    assert!(mismatch_error.to_string().contains("weights_mismatch"));

    let regrade_matches = GradeCommandBuilder::new().show().build().get_matches_from([
        "grade",
        "show",
        &trade_id,
        "--weights",
        "25,25,25,25",
        "--regrade",
    ]);
    crate::dispatcher::grades::grade_show(
        &mut dispatcher.trust,
        regrade_matches
            .subcommand_matches("show")
            .expect("show subcommand"),
        ReportOutputFormat::Text,
    )
    .expect("regrade with explicit weights should succeed");

    let stored_grade_matches = GradeCommandBuilder::new().show().build().get_matches_from([
        "grade",
        "show",
        &Uuid::new_v4().to_string(),
    ]);
    let mut stored_grade_dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::existing_empty_trade_grade(),
    );
    crate::dispatcher::grades::grade_show(
        &mut stored_grade_dispatcher.trust,
        stored_grade_matches
            .subcommand_matches("show")
            .expect("show subcommand"),
        ReportOutputFormat::Text,
    )
    .expect("stored grade with no recommendations should render");

    let stored_grade_matching_weights =
        GradeCommandBuilder::new().show().build().get_matches_from([
            "grade",
            "show",
            &Uuid::new_v4().to_string(),
            "--weights",
            "40,30,20,10",
        ]);
    crate::dispatcher::grades::grade_show(
        &mut stored_grade_dispatcher.trust,
        stored_grade_matching_weights
            .subcommand_matches("show")
            .expect("show subcommand"),
        ReportOutputFormat::Json,
    )
    .expect("stored grade should accept matching explicit weights");

    let regrade_failure_matches = GradeCommandBuilder::new().show().build().get_matches_from([
        "grade",
        "show",
        &Uuid::new_v4().to_string(),
        "--regrade",
    ]);
    let mut regrade_failure_dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::existing_trade_grade_then_trade_failure(),
    );
    let error = crate::dispatcher::grades::grade_show(
        &mut regrade_failure_dispatcher.trust,
        regrade_failure_matches
            .subcommand_matches("show")
            .expect("show subcommand"),
        ReportOutputFormat::Json,
    )
    .expect_err("regrade should surface grade computation failures");
    assert_eq!(error.code, "grade_compute_failed");

    let summary_matches = GradeCommandBuilder::new()
        .summary()
        .build()
        .get_matches_from([
            "grade",
            "summary",
            "--account",
            &account.id.to_string(),
            "--days",
            "365",
        ]);
    let summary = summary_matches
        .subcommand_matches("summary")
        .expect("summary subcommand");
    crate::dispatcher::grades::grade_summary(
        &mut dispatcher.trust,
        summary,
        ReportOutputFormat::Text,
    )
    .expect("grade summary text should include non-empty distribution");
    crate::dispatcher::grades::grade_summary(
        &mut dispatcher.trust,
        summary,
        ReportOutputFormat::Json,
    )
    .expect("grade summary json should include non-empty distribution");

    let mut ungraded_dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (ungraded_account, _) = seed_closed_target_trade(&mut ungraded_dispatcher);
    let ungraded_summary_matches = GradeCommandBuilder::new()
        .summary()
        .build()
        .get_matches_from([
            "grade",
            "summary",
            "--account",
            &ungraded_account.id.to_string(),
            "--days",
            "365",
        ]);
    crate::dispatcher::grades::grade_summary(
        &mut ungraded_dispatcher.trust,
        ungraded_summary_matches
            .subcommand_matches("summary")
            .expect("summary subcommand"),
        ReportOutputFormat::Json,
    )
    .expect("grade summary should compute missing grades");

    let mut grade_failure_dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::closed_trade_without_grade_then_trade_read_failure(
        ),
    );
    let grade_failure_account = Uuid::new_v4().to_string();
    let grade_failure_summary = GradeCommandBuilder::new()
        .summary()
        .build()
        .get_matches_from([
            "grade",
            "summary",
            "--account",
            &grade_failure_account,
            "--days",
            "365",
        ]);
    let error = crate::dispatcher::grades::grade_summary(
        &mut grade_failure_dispatcher.trust,
        grade_failure_summary
            .subcommand_matches("summary")
            .expect("summary subcommand"),
        ReportOutputFormat::Json,
    )
    .expect_err("grade summary should surface grade computation failures");
    assert_eq!(error.code, "grade_compute_failed");

    let mut empty_dispatcher = test_dispatcher();
    let empty_summary_matches = GradeCommandBuilder::new()
        .summary()
        .build()
        .get_matches_from(["grade", "summary", "--days", "30"]);
    crate::dispatcher::grades::grade_summary(
        &mut empty_dispatcher.trust,
        empty_summary_matches
            .subcommand_matches("summary")
            .expect("summary subcommand"),
        ReportOutputFormat::Text,
    )
    .expect("empty grade summary text should succeed");
}

#[test]
fn test_parse_grade_weights_default_percent_permille_and_errors() {
    let no_weights = Command::new("test")
        .arg(Arg::new("weights").long("weights").required(false))
        .get_matches_from(["test"]);
    let defaults =
        crate::dispatcher::grades::parse_grade_weights(&no_weights, ReportOutputFormat::Text)
            .unwrap();
    assert_eq!(defaults.process, 400);
    assert_eq!(defaults.risk, 300);
    assert_eq!(defaults.execution, 200);
    assert_eq!(defaults.documentation, 100);

    let percent = Command::new("test")
        .arg(Arg::new("weights").long("weights").required(false))
        .get_matches_from(["test", "--weights", "30,20,30,20"]);
    let percent_weights =
        crate::dispatcher::grades::parse_grade_weights(&percent, ReportOutputFormat::Text).unwrap();
    assert_eq!(percent_weights.process, 300);
    assert_eq!(percent_weights.risk, 200);
    assert_eq!(percent_weights.execution, 300);
    assert_eq!(percent_weights.documentation, 200);

    let permille = Command::new("test")
        .arg(Arg::new("weights").long("weights").required(false))
        .get_matches_from(["test", "--weights", "300,200,300,200"]);
    let permille_weights =
        crate::dispatcher::grades::parse_grade_weights(&permille, ReportOutputFormat::Text)
            .unwrap();
    assert_eq!(permille_weights.process, 300);
    assert_eq!(permille_weights.risk, 200);
    assert_eq!(permille_weights.execution, 300);
    assert_eq!(permille_weights.documentation, 200);

    let wrong_len = Command::new("test")
        .arg(Arg::new("weights").long("weights").required(false))
        .get_matches_from(["test", "--weights", "10,20,30"]);
    let wrong_len_error =
        crate::dispatcher::grades::parse_grade_weights(&wrong_len, ReportOutputFormat::Text)
            .unwrap_err();
    assert!(wrong_len_error.to_string().contains("invalid_weights"));

    let invalid_number = Command::new("test")
        .arg(Arg::new("weights").long("weights").required(false))
        .get_matches_from(["test", "--weights", "10,20,x,70"]);
    let invalid_number_error =
        crate::dispatcher::grades::parse_grade_weights(&invalid_number, ReportOutputFormat::Text)
            .unwrap_err();
    assert!(invalid_number_error.to_string().contains("invalid_weights"));

    let invalid_sum = Command::new("test")
        .arg(Arg::new("weights").long("weights").required(false))
        .get_matches_from(["test", "--weights", "10,20,30,10"]);
    let invalid_sum_error =
        crate::dispatcher::grades::parse_grade_weights(&invalid_sum, ReportOutputFormat::Text)
            .unwrap_err();
    assert!(invalid_sum_error
        .to_string()
        .contains("Weights must sum to 100 (percent) or 1000 (permille)"));
}
