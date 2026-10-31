#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use crate::dispatcher::error::CliError;
use crate::dispatcher::output::ReportOutputFormat;
use clap::Arg;
use clap::Command;
use rust_decimal_macros::dec;

#[test]
fn test_parse_report_format_defaults_and_json() {
    let text_matches = Command::new("test")
        .arg(Arg::new("format").long("format").required(false))
        .get_matches_from(["test"]);
    assert_eq!(
        crate::dispatcher::output::parse_report_format(&text_matches),
        ReportOutputFormat::Text
    );

    let json_matches = Command::new("test")
        .arg(Arg::new("format").long("format").required(false))
        .get_matches_from(["test", "--format", "json"]);
    assert_eq!(
        crate::dispatcher::output::parse_report_format(&json_matches),
        ReportOutputFormat::Json
    );
}

#[test]
fn test_report_error_marks_error_as_printed() {
    let error: CliError = crate::dispatcher::output::report_error(
        ReportOutputFormat::Text,
        "test_code",
        "test_message",
    );
    assert!(error.already_printed());
    assert_eq!(error.to_string(), "test_code: test_message");
}

#[test]
fn test_decimal_string_rounds_and_normalizes() {
    assert_eq!(
        crate::dispatcher::output::decimal_string(dec!(1.2300000000)),
        "1.23"
    );
    assert_eq!(
        crate::dispatcher::output::decimal_string(dec!(1.2345678912)),
        "1.23456789"
    );
}
