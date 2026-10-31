#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use crate::dispatcher::output::ReportOutputFormat;
use clap::Arg;
use clap::Command;
use uuid::Uuid;

#[test]
fn test_parse_decimal_arg_valid_invalid_missing() {
    let ok_matches = Command::new("test")
        .arg(Arg::new("value").long("value").required(true))
        .get_matches_from(["test", "--value", "123.456"]);
    let parsed =
        crate::dispatcher::args::parse_decimal_arg(&ok_matches, "value", ReportOutputFormat::Json)
            .unwrap();
    assert_eq!(parsed.to_string(), "123.456");

    let bad_matches = Command::new("test")
        .arg(Arg::new("value").long("value").required(true))
        .get_matches_from(["test", "--value", "abc"]);
    let error =
        crate::dispatcher::args::parse_decimal_arg(&bad_matches, "value", ReportOutputFormat::Json)
            .unwrap_err();
    assert!(error.to_string().contains("invalid_decimal"));
    assert!(error.already_printed());

    let missing_matches = Command::new("test")
        .arg(Arg::new("value").long("value").required(false))
        .get_matches_from(["test"]);
    let missing_error = crate::dispatcher::args::parse_decimal_arg(
        &missing_matches,
        "value",
        ReportOutputFormat::Text,
    )
    .unwrap_err();
    assert_eq!(
        missing_error.to_string(),
        "missing_argument: Missing argument --value"
    );
    assert!(missing_error.already_printed());
}

#[test]
fn test_parse_uuid_arg_valid_invalid_missing() {
    let expected = Uuid::new_v4();
    let ok_matches = Command::new("test")
        .arg(Arg::new("account").long("account").required(true))
        .get_matches_from(["test", "--account", &expected.to_string()]);
    let parsed =
        crate::dispatcher::args::parse_uuid_arg(&ok_matches, "account", ReportOutputFormat::Json)
            .unwrap();
    assert_eq!(parsed, expected);

    let bad_matches = Command::new("test")
        .arg(Arg::new("account").long("account").required(true))
        .get_matches_from(["test", "--account", "not-a-uuid"]);
    let bad_error =
        crate::dispatcher::args::parse_uuid_arg(&bad_matches, "account", ReportOutputFormat::Json)
            .unwrap_err();
    assert!(bad_error.to_string().contains("invalid_uuid"));
    assert!(bad_error.already_printed());

    let missing_matches = Command::new("test")
        .arg(Arg::new("account").long("account").required(false))
        .get_matches_from(["test"]);
    let missing_error = crate::dispatcher::args::parse_uuid_arg(
        &missing_matches,
        "account",
        ReportOutputFormat::Text,
    )
    .unwrap_err();
    assert_eq!(
        missing_error.to_string(),
        "missing_argument: Missing argument --account"
    );
    assert!(missing_error.already_printed());
}
