//! Legacy output command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::error::CliError;
use clap::ArgMatches;
use core::TrustFacade;
use rust_decimal::Decimal;
use serde_json::json;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::dispatcher) enum ReportOutputFormat {
    Text,
    Json,
}

pub(in crate::dispatcher) fn parse_report_format(matches: &ArgMatches) -> ReportOutputFormat {
    match matches.get_one::<String>("format").map(String::as_str) {
        Some("json") => ReportOutputFormat::Json,
        _ => ReportOutputFormat::Text,
    }
}

pub(in crate::dispatcher) fn decimal_string(value: Decimal) -> String {
    value.round_dp(8).normalize().to_string()
}

pub(in crate::dispatcher) fn print_json(payload: &Value) -> Result<(), CliError> {
    println!("{payload:#}");
    Ok(())
}

pub(in crate::dispatcher) fn report_error(
    format: ReportOutputFormat,
    code: &'static str,
    message: impl Into<String>,
) -> CliError {
    let error = CliError::new_printed(code, message);
    if format == ReportOutputFormat::Json {
        let payload = json!({
            "error": {
                "code": error.code,
                "message": &error.message
            }
        });
        if let Ok(json_error) = serde_json::to_string_pretty(&payload) {
            eprintln!("{json_error}");
        }
    } else {
        eprintln!("{error}");
    }
    error
}

pub(in crate::dispatcher) type FormattedCommand =
    fn(&mut TrustFacade, &ArgMatches, ReportOutputFormat) -> Result<(), CliError>;
