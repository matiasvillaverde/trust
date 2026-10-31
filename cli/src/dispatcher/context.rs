//! Legacy context command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use crate::protected_keyword;
use clap::ArgMatches;
use core::TrustFacade;
use model::Account;
use model::Trade;
use std::str::FromStr;
use uuid::Uuid;

pub(in crate::dispatcher) fn is_valid_protected_keyword(expected: &str, provided: &str) -> bool {
    if expected.is_empty() || provided.is_empty() {
        return false;
    }

    let expected_bytes = expected.as_bytes();
    let provided_bytes = provided.as_bytes();
    let max_len = expected_bytes.len().max(provided_bytes.len());
    let mut diff = expected_bytes.len() ^ provided_bytes.len();

    for index in 0..max_len {
        let expected_byte = expected_bytes.get(index).copied().unwrap_or_default();
        let provided_byte = provided_bytes.get(index).copied().unwrap_or_default();
        diff |= usize::from(expected_byte ^ provided_byte);
    }

    diff == 0
}

pub(in crate::dispatcher) fn resolve_account_arg(
    trust: &mut TrustFacade,
    raw: &str,
    format: ReportOutputFormat,
) -> Result<Account, CliError> {
    if let Ok(id) = Uuid::from_str(raw) {
        let accounts = trust.search_all_accounts().map_err(|error| {
            output::report_error(
                format,
                "accounts_unavailable",
                format!("Failed to load accounts: {error}"),
            )
        })?;
        return accounts
            .into_iter()
            .find(|account| account.id == id)
            .ok_or_else(|| {
                output::report_error(
                    format,
                    "account_not_found",
                    format!("No account found for id: {id}"),
                )
            });
    }

    trust.search_account(raw).map_err(|error| {
        output::report_error(
            format,
            "account_not_found",
            format!("No account found for '{raw}': {error}"),
        )
    })
}

pub(in crate::dispatcher) fn account_by_id(
    trust: &mut TrustFacade,
    account_id: Uuid,
    format: ReportOutputFormat,
) -> Result<Account, CliError> {
    let accounts = trust.search_all_accounts().map_err(|error| {
        output::report_error(
            format,
            "accounts_unavailable",
            format!("Failed to load accounts: {error}"),
        )
    })?;

    accounts
        .into_iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| {
            output::report_error(
                format,
                "account_not_found",
                format!("No account found for id: {account_id}"),
            )
        })
}

pub(in crate::dispatcher) fn find_trade_by_id(
    trust: &mut TrustFacade,
    trade_id: Uuid,
    statuses: &[model::Status],
    format: ReportOutputFormat,
) -> Result<Trade, CliError> {
    let accounts = trust.search_all_accounts().map_err(|error| {
        output::report_error(
            format,
            "accounts_unavailable",
            format!("Failed to load accounts: {error}"),
        )
    })?;

    for account in accounts {
        for status in statuses {
            if let Ok(trades) = trust.search_trades(account.id, *status) {
                if let Some(trade) = trades.into_iter().find(|trade| trade.id == trade_id) {
                    return Ok(trade);
                }
            }
        }
    }

    let statuses_str = statuses
        .iter()
        .map(std::string::ToString::to_string)
        .collect::<Vec<String>>()
        .join(", ");
    Err(output::report_error(
        format,
        "trade_not_found",
        format!("Trade {trade_id} not found in statuses [{statuses_str}] across all accounts"),
    ))
}

pub(in crate::dispatcher) fn ensure_protected_keyword(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
    operation: &'static str,
) -> Result<(), CliError> {
    let expected = protected_keyword::read_expected().map_err(|_| {
        output::report_error(
            format,
            "protected_keyword_not_configured",
            "Protected keyword is not configured. Run: trust keys protected-set --value <KEYWORD>",
        )
    })?;

    let provided = sub_matches
        .get_one::<String>("confirm-protected")
        .map(String::as_str)
        .ok_or_else(|| {
            output::report_error(
                format,
                "protected_keyword_required",
                format!("{operation} is protected. Provide --confirm-protected <KEYWORD>"),
            )
        })?;

    if !is_valid_protected_keyword(&expected, provided) {
        return Err(output::report_error(
            format,
            "protected_keyword_invalid",
            format!("Invalid protected keyword for {operation}."),
        ));
    }

    trust
        .authorize_protected_mutation(provided, &expected)
        .map_err(|_| {
            output::report_error(
                format,
                "protected_keyword_invalid",
                format!("Invalid protected keyword for {operation}."),
            )
        })?;
    Ok(())
}
