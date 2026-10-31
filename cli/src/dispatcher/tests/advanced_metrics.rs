#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::broker::*;
use super::fixtures::*;
use crate::commands::MetricsCommandBuilder;
use clap::Arg;
use clap::Command;
use model::Environment;
use model::Status;
use model::Trade;
use rust_decimal_macros::dec;
use uuid::Uuid;

#[test]
fn test_metrics_compare_executes_text_json_validation_and_exports() {
    let mut dispatcher = test_dispatcher();
    let account = dispatcher
        .trust
        .create_account(
            "metrics-compare-account",
            "metrics compare",
            Environment::Paper,
            dec!(10),
            dec!(5),
        )
        .expect("seed account");
    let text = MetricsCommandBuilder::new()
        .compare()
        .build()
        .get_matches_from([
            "metrics",
            "compare",
            "--period1",
            "all",
            "--period2",
            "last-7-days",
        ]);
    crate::dispatcher::advanced_metrics::metrics_compare(
        &mut dispatcher.trust,
        text.subcommand_matches("compare").unwrap(),
    );

    let valid_account = MetricsCommandBuilder::new()
        .compare()
        .build()
        .get_matches_from([
            "metrics",
            "compare",
            "--period1",
            "all",
            "--period2",
            "last-7-days",
            "--account",
            &account.id.to_string(),
        ]);
    crate::dispatcher::advanced_metrics::metrics_compare(
        &mut dispatcher.trust,
        valid_account.subcommand_matches("compare").unwrap(),
    );

    let json = MetricsCommandBuilder::new()
        .compare()
        .build()
        .get_matches_from([
            "metrics",
            "compare",
            "--period1",
            "previous-7-days",
            "--period2",
            "last-7-days",
            "--format",
            "json",
        ]);
    crate::dispatcher::advanced_metrics::metrics_compare(
        &mut dispatcher.trust,
        json.subcommand_matches("compare").unwrap(),
    );

    let invalid_account = MetricsCommandBuilder::new()
        .compare()
        .build()
        .get_matches_from([
            "metrics",
            "compare",
            "--period1",
            "all",
            "--period2",
            "last-7-days",
            "--account",
            "not-a-uuid",
        ]);
    crate::dispatcher::advanced_metrics::metrics_compare(
        &mut dispatcher.trust,
        invalid_account.subcommand_matches("compare").unwrap(),
    );

    let invalid_second_period = MetricsCommandBuilder::new()
        .compare()
        .build()
        .get_matches_from([
            "metrics",
            "compare",
            "--period1",
            "all",
            "--period2",
            "not-a-period",
        ]);
    crate::dispatcher::advanced_metrics::metrics_compare(
        &mut dispatcher.trust,
        invalid_second_period.subcommand_matches("compare").unwrap(),
    );

    let csv_output =
        std::env::temp_dir().join(format!("trust-metrics-compare-{}.csv", Uuid::new_v4()));
    let csv_output_arg = csv_output.to_string_lossy().to_string();
    let csv_export = MetricsCommandBuilder::new()
        .compare()
        .build()
        .get_matches_from([
            "metrics",
            "compare",
            "--period1",
            "all",
            "--period2",
            "last-7-days",
            "--export",
            "csv",
            "--output",
            &csv_output_arg,
        ]);
    crate::dispatcher::advanced_metrics::metrics_compare(
        &mut dispatcher.trust,
        csv_export.subcommand_matches("compare").unwrap(),
    );
    assert!(csv_output.exists());
    let _ = std::fs::remove_file(&csv_output);

    let json_output =
        std::env::temp_dir().join(format!("trust-metrics-compare-{}.json", Uuid::new_v4()));
    let json_output_arg = json_output.to_string_lossy().to_string();
    let json_export = MetricsCommandBuilder::new()
        .compare()
        .build()
        .get_matches_from([
            "metrics",
            "compare",
            "--period1",
            "all",
            "--period2",
            "last-7-days",
            "--format",
            "json",
            "--export",
            "json",
            "--output",
            &json_output_arg,
        ]);
    crate::dispatcher::advanced_metrics::metrics_compare(
        &mut dispatcher.trust,
        json_export.subcommand_matches("compare").unwrap(),
    );
    assert!(json_output.exists());
    let _ = std::fs::remove_file(&json_output);

    let failed_export = MetricsCommandBuilder::new()
        .compare()
        .build()
        .get_matches_from([
            "metrics",
            "compare",
            "--period1",
            "all",
            "--period2",
            "last-7-days",
            "--export",
            "csv",
            "--output",
            "/tmp",
        ]);
    crate::dispatcher::advanced_metrics::metrics_compare(
        &mut dispatcher.trust,
        failed_export.subcommand_matches("compare").unwrap(),
    );

    let unsupported_export = Command::new("compare")
        .arg(Arg::new("period1").long("period1").required(true))
        .arg(Arg::new("period2").long("period2").required(true))
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("format").long("format"))
        .arg(Arg::new("export").long("export"))
        .arg(Arg::new("output").long("output"))
        .get_matches_from([
            "compare",
            "--period1",
            "all",
            "--period2",
            "last-7-days",
            "--export",
            "xml",
            "--output",
            "/tmp",
        ]);
    crate::dispatcher::advanced_metrics::metrics_compare(
        &mut dispatcher.trust,
        &unsupported_export,
    );
}

#[test]
fn test_export_metrics_writes_json_csv_and_rejects_unknown_format() {
    let mut dispatcher = test_dispatcher();
    let mut trade = Trade {
        status: Status::ClosedTarget,
        ..Trade::default()
    };
    trade.balance.total_performance = dec!(125);
    let trades = vec![trade];

    let json_output =
        std::env::temp_dir().join(format!("trust-advanced-metrics-{}.json", Uuid::new_v4()));
    let json_output_arg = json_output.to_string_lossy().to_string();
    crate::dispatcher::advanced_metrics::export_metrics(
        &mut dispatcher.trust,
        &trades,
        "json",
        &json_output_arg,
        dec!(0.05),
    )
    .expect("json export should succeed");
    assert!(json_output.exists());
    let _ = std::fs::remove_file(&json_output);

    let csv_output =
        std::env::temp_dir().join(format!("trust-advanced-metrics-{}.csv", Uuid::new_v4()));
    let csv_output_arg = csv_output.to_string_lossy().to_string();
    crate::dispatcher::advanced_metrics::export_metrics(
        &mut dispatcher.trust,
        &trades,
        "csv",
        &csv_output_arg,
        dec!(0.05),
    )
    .expect("csv export should succeed");
    assert!(csv_output.exists());
    let _ = std::fs::remove_file(&csv_output);

    let unsupported = crate::dispatcher::advanced_metrics::export_metrics(
        &mut dispatcher.trust,
        &trades,
        "xml",
        "/tmp/unused-trust-metrics.xml",
        dec!(0.05),
    )
    .expect_err("unsupported export formats should fail");
    assert!(unsupported
        .to_string()
        .contains("Unsupported export format"));
}

#[test]
fn test_metrics_advanced_executes_non_empty_display_and_export_paths() {
    let mut empty_dispatcher = test_dispatcher();
    let empty_display = MetricsCommandBuilder::new()
        .advanced()
        .build()
        .get_matches_from(["metrics", "advanced"]);
    crate::dispatcher::advanced_metrics::metrics_advanced(
        &mut empty_dispatcher.trust,
        empty_display.subcommand_matches("advanced").unwrap(),
    );
    let invalid_account = MetricsCommandBuilder::new()
        .advanced()
        .build()
        .get_matches_from(["metrics", "advanced", "--account", "not-a-uuid"]);
    crate::dispatcher::advanced_metrics::metrics_advanced(
        &mut empty_dispatcher.trust,
        invalid_account.subcommand_matches("advanced").unwrap(),
    );

    let mut dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (account, _closed) = seed_closed_target_trade(&mut dispatcher);

    let display = MetricsCommandBuilder::new()
        .advanced()
        .build()
        .get_matches_from([
            "metrics",
            "advanced",
            "--account",
            &account.id.to_string(),
            "--days",
            "0",
            "--risk-free-rate",
            "0.03",
        ]);
    crate::dispatcher::advanced_metrics::metrics_advanced(
        &mut dispatcher.trust,
        display.subcommand_matches("advanced").unwrap(),
    );

    let json_output =
        std::env::temp_dir().join(format!("trust-metrics-advanced-{}.json", Uuid::new_v4()));
    let json_output_arg = json_output.to_string_lossy().to_string();
    let json_export = MetricsCommandBuilder::new()
        .advanced()
        .build()
        .get_matches_from([
            "metrics",
            "advanced",
            "--account",
            &account.id.to_string(),
            "--days",
            "0",
            "--export",
            "json",
            "--output",
            &json_output_arg,
        ]);
    crate::dispatcher::advanced_metrics::metrics_advanced(
        &mut dispatcher.trust,
        json_export.subcommand_matches("advanced").unwrap(),
    );
    assert!(json_output.exists());
    let _ = std::fs::remove_file(&json_output);

    let csv_output =
        std::env::temp_dir().join(format!("trust-metrics-advanced-{}.csv", Uuid::new_v4()));
    let csv_output_arg = csv_output.to_string_lossy().to_string();
    let csv_export = MetricsCommandBuilder::new()
        .advanced()
        .build()
        .get_matches_from([
            "metrics",
            "advanced",
            "--account",
            &account.id.to_string(),
            "--days",
            "0",
            "--export",
            "csv",
            "--output",
            &csv_output_arg,
        ]);
    crate::dispatcher::advanced_metrics::metrics_advanced(
        &mut dispatcher.trust,
        csv_export.subcommand_matches("advanced").unwrap(),
    );
    assert!(csv_output.exists());
    let _ = std::fs::remove_file(&csv_output);

    let failed_export = MetricsCommandBuilder::new()
        .advanced()
        .build()
        .get_matches_from([
            "metrics",
            "advanced",
            "--account",
            &account.id.to_string(),
            "--days",
            "0",
            "--export",
            "json",
            "--output",
            "/tmp",
        ]);
    crate::dispatcher::advanced_metrics::metrics_advanced(
        &mut dispatcher.trust,
        failed_export.subcommand_matches("advanced").unwrap(),
    );
}
