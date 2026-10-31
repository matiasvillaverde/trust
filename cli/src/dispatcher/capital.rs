//! Legacy capital command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_transactions_subcommand;
use crate::command_routing::TransactionsSubcommand;
use crate::dialogs::TransactionDialogBuilder;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::TrustFacade;
use model::Currency;
use model::TransactionCategory;
use rust_decimal::Decimal;
use std::str::FromStr;

pub(in crate::dispatcher) fn dispatch_transactions(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    match parse_transactions_subcommand(sub_matches) {
        TransactionsSubcommand::Deposit(sub_sub_matches) => deposit(trust, sub_sub_matches)?,
        TransactionsSubcommand::Withdraw(sub_sub_matches) => withdraw(trust, sub_sub_matches)?,
    }
    Ok(())
}

pub(in crate::dispatcher) fn has_non_interactive_transaction_args(
    sub_matches: &ArgMatches,
) -> bool {
    sub_matches.get_one::<String>("account").is_some()
        || sub_matches.get_one::<String>("currency").is_some()
        || sub_matches.get_one::<String>("amount").is_some()
}

pub(in crate::dispatcher) fn create_transaction_non_interactive(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    category: TransactionCategory,
) -> Result<(), CliError> {
    let format = ReportOutputFormat::Text;
    let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --account")
    })?;
    let currency_raw = sub_matches.get_one::<String>("currency").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --currency")
    })?;
    let amount_raw = sub_matches.get_one::<String>("amount").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --amount")
    })?;

    let account = context::resolve_account_arg(trust, account_raw, format)?;
    let currency =
        Currency::from_str(currency_raw.trim().to_ascii_uppercase().as_str()).map_err(|_| {
            output::report_error(
                format,
                "invalid_currency",
                format!(
                    "Invalid --currency value: {}",
                    currency_raw.to_ascii_lowercase()
                ),
            )
        })?;
    let amount = Decimal::from_str_exact(amount_raw.trim()).map_err(|_| {
        output::report_error(
            format,
            "invalid_decimal",
            format!("Invalid --amount decimal: {amount_raw}"),
        )
    })?;

    let (transaction, account_balance) = trust
        .create_transaction(&account, &category, amount, &currency)
        .map_err(|e| output::report_error(format, "transaction_create_failed", e.to_string()))?;

    crate::views::TransactionView::display_with_basis(
        &transaction,
        &account.name,
        account_balance.total_balance,
    );
    crate::views::AccountBalanceView::display(account_balance, &account.name);
    Ok(())
}

pub(in crate::dispatcher) fn deposit(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    context::ensure_protected_keyword(trust, sub_matches, ReportOutputFormat::Text, "deposit")?;
    if has_non_interactive_transaction_args(sub_matches) {
        create_transaction_non_interactive(trust, sub_matches, TransactionCategory::Deposit)?;
    } else {
        TransactionDialogBuilder::new(TransactionCategory::Deposit)
            .account(trust)
            .currency(trust)
            .amount(trust)
            .build(trust)
            .display();
    }
    Ok(())
}

pub(in crate::dispatcher) fn withdraw(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    context::ensure_protected_keyword(trust, sub_matches, ReportOutputFormat::Text, "withdrawal")?;
    if has_non_interactive_transaction_args(sub_matches) {
        create_transaction_non_interactive(trust, sub_matches, TransactionCategory::Withdrawal)?;
    } else {
        TransactionDialogBuilder::new(TransactionCategory::Withdrawal)
            .account(trust)
            .currency(trust)
            .amount(trust)
            .build(trust)
            .display();
    }
    Ok(())
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct TransactionCommand;
impl super::Command for TransactionCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_transactions(trust, args)
    }
}
