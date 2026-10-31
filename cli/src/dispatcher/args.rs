//! Legacy args command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::str::FromStr;
use uuid::Uuid;

pub(in crate::dispatcher) fn parse_decimal_arg(
    sub_matches: &ArgMatches,
    key: &'static str,
    format: ReportOutputFormat,
) -> Result<Decimal, CliError> {
    let raw = sub_matches.get_one::<String>(key).ok_or_else(|| {
        output::report_error(
            format,
            "missing_argument",
            format!("Missing argument --{key}"),
        )
    })?;
    Decimal::from_str(raw).map_err(|_| {
        output::report_error(
            format,
            "invalid_decimal",
            format!("Invalid decimal value for --{key}: {raw}"),
        )
    })
}

pub(in crate::dispatcher) fn parse_optional_decimal_arg(
    sub_matches: &ArgMatches,
    key: &'static str,
    format: ReportOutputFormat,
) -> Result<Option<Decimal>, CliError> {
    sub_matches
        .get_one::<String>(key)
        .map(|raw| {
            Decimal::from_str(raw).map_err(|_| {
                output::report_error(
                    format,
                    "invalid_decimal",
                    format!("Invalid decimal value for --{key}: {raw}"),
                )
            })
        })
        .transpose()
}

pub(in crate::dispatcher) fn parse_optional_percent_arg(
    sub_matches: &ArgMatches,
    key: &'static str,
    format: ReportOutputFormat,
) -> Result<Option<Decimal>, CliError> {
    parse_optional_decimal_arg(sub_matches, key, format)?
        .map(|value| {
            if value < Decimal::ZERO {
                return Err(output::report_error(
                    format,
                    "invalid_percentage",
                    format!("Invalid percentage for --{key}: value cannot be negative"),
                ));
            }
            let rate = value.checked_div(dec!(100)).ok_or_else(|| {
                output::report_error(
                    format,
                    "invalid_percentage",
                    format!("Invalid percentage for --{key}: {value}"),
                )
            })?;
            if rate > Decimal::ONE {
                return Err(output::report_error(
                    format,
                    "invalid_percentage",
                    format!("Invalid percentage for --{key}: value cannot exceed 100"),
                ));
            }
            Ok(rate)
        })
        .transpose()
}

pub(in crate::dispatcher) fn parse_uuid_arg(
    sub_matches: &ArgMatches,
    key: &'static str,
    format: ReportOutputFormat,
) -> Result<Uuid, CliError> {
    let raw = sub_matches.get_one::<String>(key).ok_or_else(|| {
        output::report_error(
            format,
            "missing_argument",
            format!("Missing argument --{key}"),
        )
    })?;
    Uuid::from_str(raw).map_err(|_| {
        output::report_error(
            format,
            "invalid_uuid",
            format!("Invalid UUID for --{key}: {raw}"),
        )
    })
}

pub(in crate::dispatcher) fn string_arg_present(matches: &ArgMatches, key: &str) -> bool {
    matches.try_get_one::<String>(key).ok().flatten().is_some()
}

pub(in crate::dispatcher) fn bool_arg_present(matches: &ArgMatches, key: &str) -> bool {
    matches
        .try_get_one::<bool>(key)
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false)
}

/// Required values fail with an actionable CLI error, even for programmatic callers.
pub(in crate::dispatcher) fn required_string_arg<'a>(
    matches: &'a ArgMatches,
    key: &str,
) -> Result<&'a str, CliError> {
    matches
        .try_get_one::<String>(key)
        .ok()
        .flatten()
        .map(String::as_str)
        .ok_or_else(|| CliError::new("missing_argument", format!("Provide --{key}.")))
}
