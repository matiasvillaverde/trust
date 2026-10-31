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
use crate::commands::ReportCommandBuilder;
use crate::dispatcher::output::ReportOutputFormat;
use model::MistakeErrorType;
use model::MungerTendency;

#[test]
fn test_report_handler_factory_routes_to_expected_handler_behavior() {
    let mut dispatcher = test_dispatcher();
    let invalid_account = "not-a-uuid";
    let commands: Vec<Vec<&str>> = vec![
        vec!["report", "performance", "--account", invalid_account],
        vec!["report", "drawdown", "--account", invalid_account],
        vec!["report", "risk", "--account", invalid_account],
        vec!["report", "concentration", "--account", invalid_account],
        vec!["report", "summary", "--account", invalid_account],
        vec!["report", "metrics", "--account", invalid_account],
        vec![
            "report",
            "attribution",
            "--account",
            invalid_account,
            "--by",
            "symbol",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ],
        vec![
            "report",
            "benchmark",
            "--account",
            invalid_account,
            "--benchmark",
            "SPY",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ],
        vec![
            "report",
            "timeline",
            "--account",
            invalid_account,
            "--granularity",
            "day",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ],
        vec![
            "report",
            "bias-summary",
            "--account",
            invalid_account,
            "--days",
            "7",
        ],
        vec!["report", "tax", "--account", invalid_account],
    ];

    for argv in commands {
        let matches = ReportCommandBuilder::new()
            .performance()
            .drawdown()
            .risk()
            .concentration()
            .summary()
            .metrics()
            .attribution()
            .benchmark()
            .timeline()
            .bias_summary()
            .tax()
            .build()
            .get_matches_from(argv);

        let (handler, sub) = crate::dispatcher::reports::report_handler_for(
            crate::command_routing::parse_report_subcommand(&matches),
        );
        let error = handler(&mut dispatcher.trust, sub, ReportOutputFormat::Json)
            .expect_err("invalid account should fail");
        assert!(
            error.to_string().contains("invalid_account_id")
                || error.to_string().contains("account_not_found")
        );
    }
}

#[test]
fn test_bias_summary_report_aggregates_mistakes_and_renders_formats() {
    let mut dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (account, closed) = seed_closed_target_trade(&mut dispatcher);
    seed_mistake(
        &mut dispatcher,
        closed.id,
        vec![
            MungerTendency::DeprivalSuperreaction,
            MungerTendency::SocialProof,
        ],
        false,
        MistakeErrorType::Commission,
    );
    seed_mistake(
        &mut dispatcher,
        closed.id,
        vec![MungerTendency::DeprivalSuperreaction],
        false,
        MistakeErrorType::Omission,
    );
    seed_mistake(
        &mut dispatcher,
        closed.id,
        vec![
            MungerTendency::DeprivalSuperreaction,
            MungerTendency::Lollapalooza,
        ],
        true,
        MistakeErrorType::Commission,
    );

    let account_id = account.id.to_string();
    let json_matches = ReportCommandBuilder::new()
        .bias_summary()
        .build()
        .get_matches_from([
            "report",
            "bias-summary",
            "--format",
            "json",
            "--account",
            &account_id,
            "--days",
            "7",
        ]);
    crate::dispatcher::reports::dispatch_report(&mut dispatcher.trust, &json_matches)
        .expect("bias summary json should dispatch");

    let human_matches = ReportCommandBuilder::new()
        .bias_summary()
        .build()
        .get_matches_from([
            "report",
            "bias-summary",
            "--format",
            "human",
            "--account",
            &account_id,
            "--days",
            "7",
        ]);
    crate::dispatcher::reports::dispatch_report(&mut dispatcher.trust, &human_matches)
        .expect("bias summary human should dispatch");

    let aggregation = dispatcher
        .trust
        .bias_aggregation_for_account(account.id, 7)
        .expect("bias aggregation should calculate");
    let payload =
        crate::dispatcher::bias_report::bias_aggregation_payload(account.id, &aggregation);
    assert_eq!(payload["report"], "bias_summary");
    assert_eq!(payload["window_days"], 7);
    assert_eq!(payload["total_mistakes"], 3);
    assert_eq!(payload["most_frequent_tendency"]["tendency_number"], 14);
    assert_eq!(payload["most_frequent_tendency"]["count"], 3);
    assert_eq!(payload["lollapalooza_count"], 1);
    assert_eq!(payload["omission_count"], 1);
    assert_eq!(payload["commission_count"], 2);
    assert_eq!(payload["mechanical_antidote_needed"], true);
    assert_eq!(payload["standdown_recommended"], false);
}

#[test]
fn test_dispatch_report_and_grade_remaining_subcommand_arms() {
    let mut dispatcher = test_dispatcher();
    let invalid_account = "not-a-uuid";
    let report_commands: Vec<Vec<&str>> = vec![
        vec!["report", "risk", "--account", invalid_account],
        vec!["report", "concentration", "--account", invalid_account],
        vec!["report", "summary", "--account", invalid_account],
        vec!["report", "metrics", "--account", invalid_account],
        vec![
            "report",
            "attribution",
            "--account",
            invalid_account,
            "--by",
            "symbol",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ],
        vec![
            "report",
            "benchmark",
            "--account",
            invalid_account,
            "--benchmark",
            "SPY",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ],
        vec![
            "report",
            "timeline",
            "--account",
            invalid_account,
            "--granularity",
            "day",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ],
        vec![
            "report",
            "bias-summary",
            "--account",
            invalid_account,
            "--days",
            "7",
        ],
    ];

    for argv in report_commands {
        let matches = ReportCommandBuilder::new()
            .risk()
            .concentration()
            .summary()
            .metrics()
            .attribution()
            .benchmark()
            .timeline()
            .bias_summary()
            .build()
            .get_matches_from(argv);
        let error = crate::dispatcher::reports::dispatch_report(&mut dispatcher.trust, &matches)
            .expect_err("invalid account should fail");
        assert!(
            error.to_string().contains("invalid_account_id")
                || error.to_string().contains("account_not_found")
        );
    }

    let grade_summary = GradeCommandBuilder::new()
        .summary()
        .build()
        .get_matches_from(["grade", "summary", "--account", invalid_account]);
    let grade_error =
        crate::dispatcher::grades::dispatch_grade(&mut dispatcher.trust, &grade_summary)
            .expect_err("invalid grade summary account should fail");
    assert!(grade_error.to_string().contains("invalid_account_id"));
}

#[test]
fn test_dispatch_report_new_subcommands_cover_success_and_failure_routes() {
    let mut dispatcher = test_dispatcher();
    let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);
    let account_id = account.id.to_string();

    let attribution_json = ReportCommandBuilder::new()
        .attribution()
        .build()
        .get_matches_from([
            "report",
            "--format",
            "json",
            "attribution",
            "--account",
            &account_id,
            "--by",
            "symbol",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ]);
    crate::dispatcher::reports::dispatch_report(&mut dispatcher.trust, &attribution_json)
        .expect("attribution json should dispatch");

    let timeline_text = ReportCommandBuilder::new()
        .timeline()
        .build()
        .get_matches_from([
            "report",
            "timeline",
            "--account",
            &account_id,
            "--granularity",
            "day",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ]);
    crate::dispatcher::reports::dispatch_report(&mut dispatcher.trust, &timeline_text)
        .expect("timeline text should dispatch");

    let tax_json = ReportCommandBuilder::new().tax().build().get_matches_from([
        "report",
        "--format",
        "json",
        "tax",
        "--account",
        &account_id,
    ]);
    crate::dispatcher::reports::dispatch_report(&mut dispatcher.trust, &tax_json)
        .expect("tax json should dispatch");

    let benchmark = ReportCommandBuilder::new()
        .benchmark()
        .build()
        .get_matches_from([
            "report",
            "--format",
            "json",
            "benchmark",
            "--account",
            &account_id,
            "--benchmark",
            "SPY",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ]);
    let err = crate::dispatcher::reports::dispatch_report(&mut dispatcher.trust, &benchmark)
        .expect_err("benchmark should fail with default test broker");
    assert!(err.to_string().contains("benchmark_data_failed"));
}
