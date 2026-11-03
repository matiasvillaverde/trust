#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use crate::dispatcher::output::ReportOutputFormat;
use chrono::NaiveDate;
use clap::Arg;
use clap::Command;

#[test]
fn test_parse_required_date_range_validates_format_and_order() {
    let valid = Command::new("test")
        .arg(Arg::new("from").long("from").required(true))
        .arg(Arg::new("to").long("to").required(true))
        .get_matches_from(["test", "--from", "2026-01-01", "--to", "2026-01-31"]);
    let (from, to) = crate::dispatcher::metrics_report::parse_required_date_range(
        &valid,
        ReportOutputFormat::Json,
    )
    .expect("valid range should parse");
    assert_eq!(
        from,
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid literal date")
    );
    assert_eq!(
        to,
        NaiveDate::from_ymd_opt(2026, 1, 31).expect("valid literal date")
    );

    let invalid = Command::new("test")
        .arg(Arg::new("from").long("from").required(true))
        .arg(Arg::new("to").long("to").required(true))
        .get_matches_from(["test", "--from", "bad", "--to", "2026-01-31"]);
    let err = crate::dispatcher::metrics_report::parse_required_date_range(
        &invalid,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid date should fail");
    assert!(err.to_string().contains("invalid_date"));

    let reversed = Command::new("test")
        .arg(Arg::new("from").long("from").required(true))
        .arg(Arg::new("to").long("to").required(true))
        .get_matches_from(["test", "--from", "2026-02-01", "--to", "2026-01-31"]);
    let err = crate::dispatcher::metrics_report::parse_required_date_range(
        &reversed,
        ReportOutputFormat::Json,
    )
    .expect_err("reversed date range should fail");
    assert!(err.to_string().contains("invalid_date_range"));
}
