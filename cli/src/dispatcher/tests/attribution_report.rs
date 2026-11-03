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
fn reports_reconcile_recorded_profit_funding_and_cash_flows() {
    crate::test_support::capture::assert_stdout(
        module_path!(),
        "reports_reconcile_recorded_profit_funding_and_cash_flows",
        || {
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

            println!("BEGIN:attribution");
            crate::dispatcher::attribution_report::attribution_report(
                &mut dispatcher.trust,
                attribution_matches,
                ReportOutputFormat::Json,
            )
            .expect("attribution json report should succeed");
            println!("END:attribution");

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

                println!("BEGIN:{grouping}");
                crate::dispatcher::attribution_report::attribution_report(
                    &mut dispatcher.trust,
                    grouped.subcommand_matches("attribution").unwrap(),
                    format,
                )
                .expect("grouped attribution report should succeed");
                println!("END:{grouping}");
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

            println!("BEGIN:benchmark");
            crate::dispatcher::benchmark_report::benchmark_report(
                &mut dispatcher.trust,
                benchmark_matches,
                ReportOutputFormat::Json,
            )
            .expect("benchmark json report should succeed");
            println!("END:benchmark");

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

                println!("BEGIN:{granularity}");
                crate::dispatcher::timeline_report::timeline_report(
                    &mut dispatcher.trust,
                    timeline.subcommand_matches("timeline").unwrap(),
                    format,
                )
                .expect("timeline report should succeed");
                println!("END:{granularity}");
            }
        },
        |output| {
            assert!(output.contains("Attribution report (symbol)"), "{output}");
            assert!(output.contains("closed_trades: 1"), "{output}");
            assert!(
                output.contains("AAPL: trades=1 pnl=20 funding=200 share_pct=100"),
                "{output}"
            );
            assert!(output.contains("Benchmark report"), "{output}");
            assert!(output.contains("Timeline report"), "{output}");
            use crate::test_support::capture::json_section;
            let attribution = json_section(output, "attribution");
            assert_eq!(attribution["data"]["closed_trade_count"], 1);
            assert_eq!(attribution["data"]["total_pnl"], "20");
            assert_eq!(
                attribution["data"]["groups"],
                serde_json::json!([{"key":"AAPL","trade_count":1,"pnl":"20","funding":"200","share_pct":"100"}])
            );
            assert_eq!(
                json_section(output, "asset-class")["data"]["groups"][0]["key"],
                "equity"
            );
            let benchmark = json_section(output, "benchmark");
            assert_eq!(benchmark["scope"]["benchmark"], "SPY");
            assert_eq!(
                benchmark["data"]["strategy"],
                serde_json::json!({"funding":"200","pnl":"20","return_pct":"10"})
            );
            assert_eq!(benchmark["data"]["benchmark"]["bars"], 1);
            assert_eq!(benchmark["data"]["benchmark"]["return_pct"], "1.51515152");
            assert_eq!(benchmark["data"]["alpha_pct"], "8.48484848");
            let timeline = json_section(output, "week");
            assert_eq!(timeline["data"]["bucket_count"], 1);
            assert_eq!(timeline["data"]["net_cash_flow"], "10020");
            assert_eq!(timeline["data"]["buckets"][0]["net_cash_flow"], "10020");
        },
    );
}
