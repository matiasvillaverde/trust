#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::broker::*;
use super::fixtures::*;
use crate::commands::ReportCommandBuilder;
use crate::dispatcher::output::ReportOutputFormat;
use clap::Arg;
use clap::Command;

#[test]
fn test_new_report_handlers_cover_json_and_validation_paths() {
    let mut dispatcher = test_dispatcher();
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);
    let account_id = account.id.to_string();

    let attribution = Command::new("test")
        .arg(Arg::new("account").long("account").required(true))
        .arg(Arg::new("by").long("by").required(true))
        .arg(Arg::new("from").long("from").required(true))
        .arg(Arg::new("to").long("to").required(true))
        .get_matches_from([
            "test",
            "--account",
            &account_id,
            "--by",
            "symbol",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ]);
    crate::dispatcher::attribution_report::attribution_report(
        &mut dispatcher.trust,
        &attribution,
        ReportOutputFormat::Json,
    )
    .expect("attribution report should succeed");

    let attribution_bad_by = Command::new("test")
        .arg(Arg::new("account").long("account").required(true))
        .arg(Arg::new("by").long("by").required(true))
        .arg(Arg::new("from").long("from").required(true))
        .arg(Arg::new("to").long("to").required(true))
        .get_matches_from([
            "test",
            "--account",
            &account_id,
            "--by",
            "bad",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ]);
    let err = crate::dispatcher::attribution_report::attribution_report(
        &mut dispatcher.trust,
        &attribution_bad_by,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid attribution dimension should fail");
    assert!(err.to_string().contains("invalid_argument"));

    let timeline = Command::new("test")
        .arg(Arg::new("account").long("account").required(true))
        .arg(Arg::new("granularity").long("granularity").required(true))
        .arg(Arg::new("from").long("from").required(true))
        .arg(Arg::new("to").long("to").required(true))
        .get_matches_from([
            "test",
            "--account",
            &account_id,
            "--granularity",
            "day",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ]);
    crate::dispatcher::timeline_report::timeline_report(
        &mut dispatcher.trust,
        &timeline,
        ReportOutputFormat::Json,
    )
    .expect("timeline report should succeed");

    let timeline_bad = Command::new("test")
        .arg(Arg::new("account").long("account").required(true))
        .arg(Arg::new("granularity").long("granularity").required(true))
        .arg(Arg::new("from").long("from").required(true))
        .arg(Arg::new("to").long("to").required(true))
        .get_matches_from([
            "test",
            "--account",
            &account_id,
            "--granularity",
            "bad",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ]);
    let err = crate::dispatcher::timeline_report::timeline_report(
        &mut dispatcher.trust,
        &timeline_bad,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid timeline granularity should fail");
    assert!(err.to_string().contains("invalid_argument"));

    let benchmark = Command::new("test")
        .arg(Arg::new("account").long("account").required(true))
        .arg(Arg::new("benchmark").long("benchmark").required(true))
        .arg(Arg::new("from").long("from").required(true))
        .arg(Arg::new("to").long("to").required(true))
        .get_matches_from([
            "test",
            "--account",
            &account_id,
            "--benchmark",
            "SPY",
            "--from",
            "2026-01-01",
            "--to",
            "2026-01-31",
        ]);
    let err = crate::dispatcher::benchmark_report::benchmark_report(
        &mut dispatcher.trust,
        &benchmark,
        ReportOutputFormat::Json,
    )
    .expect_err("benchmark fetch should fail with default test broker");
    assert!(err.to_string().contains("benchmark_data_failed"));
}

#[test]
fn test_attribution_benchmark_and_timeline_reports_execute_text_and_json_paths() {
    let mut dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (account, _closed) = seed_closed_target_trade(&mut dispatcher);
    let account_id = account.id.to_string();

    let attribution = ReportCommandBuilder::new()
        .attribution()
        .build()
        .get_matches_from([
            "report",
            "attribution",
            "--account",
            &account_id,
            "--by",
            "symbol",
            "--from",
            "2020-01-01",
            "--to",
            "2030-01-01",
        ]);
    let attribution_matches = attribution.subcommand_matches("attribution").unwrap();
    crate::dispatcher::attribution_report::attribution_report(
        &mut dispatcher.trust,
        attribution_matches,
        ReportOutputFormat::Text,
    )
    .expect("attribution text report should succeed");
    crate::dispatcher::attribution_report::attribution_report(
        &mut dispatcher.trust,
        attribution_matches,
        ReportOutputFormat::Json,
    )
    .expect("attribution json report should succeed");
    for (grouping, format) in [
        ("sector", ReportOutputFormat::Text),
        ("asset-class", ReportOutputFormat::Json),
    ] {
        let grouped = ReportCommandBuilder::new()
            .attribution()
            .build()
            .get_matches_from([
                "report",
                "attribution",
                "--account",
                &account_id,
                "--by",
                grouping,
                "--from",
                "2020-01-01",
                "--to",
                "2030-01-01",
            ]);
        crate::dispatcher::attribution_report::attribution_report(
            &mut dispatcher.trust,
            grouped.subcommand_matches("attribution").unwrap(),
            format,
        )
        .expect("grouped attribution report should succeed");
    }

    let benchmark = ReportCommandBuilder::new()
        .benchmark()
        .build()
        .get_matches_from([
            "report",
            "benchmark",
            "--account",
            &account_id,
            "--benchmark",
            "SPY",
            "--from",
            "2020-01-01",
            "--to",
            "2030-01-01",
        ]);
    let benchmark_matches = benchmark.subcommand_matches("benchmark").unwrap();
    crate::dispatcher::benchmark_report::benchmark_report(
        &mut dispatcher.trust,
        benchmark_matches,
        ReportOutputFormat::Text,
    )
    .expect("benchmark text report should succeed");
    crate::dispatcher::benchmark_report::benchmark_report(
        &mut dispatcher.trust,
        benchmark_matches,
        ReportOutputFormat::Json,
    )
    .expect("benchmark json report should succeed");

    for (granularity, format) in [
        ("day", ReportOutputFormat::Text),
        ("week", ReportOutputFormat::Json),
        ("month", ReportOutputFormat::Text),
    ] {
        let timeline = ReportCommandBuilder::new()
            .timeline()
            .build()
            .get_matches_from([
                "report",
                "timeline",
                "--account",
                &account_id,
                "--granularity",
                granularity,
                "--from",
                "2020-01-01",
                "--to",
                "2030-01-01",
            ]);
        crate::dispatcher::timeline_report::timeline_report(
            &mut dispatcher.trust,
            timeline.subcommand_matches("timeline").unwrap(),
            format,
        )
        .expect("timeline report should succeed");
    }
}
