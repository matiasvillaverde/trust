//! Legacy keys command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_keys_subcommand;
use crate::command_routing::KeysSubcommand;
use crate::dialogs::KeysDeleteDialogBuilder;
use crate::dialogs::KeysReadDialogBuilder;
use crate::dialogs::KeysWriteDialogBuilder;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output::ReportOutputFormat;
use crate::protected_keyword;
use clap::ArgMatches;
use core::TrustFacade;

pub(in crate::dispatcher) fn dispatch_keys(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    match parse_keys_subcommand(sub_matches) {
        KeysSubcommand::Create(sub_sub_matches) => create_keys(trust, sub_sub_matches)?,
        KeysSubcommand::Show => show_keys(trust),
        KeysSubcommand::Delete(sub_sub_matches) => delete_keys(trust, sub_sub_matches)?,
        KeysSubcommand::ProtectedSet(sub_sub_matches) => {
            set_protected_keyword(trust, sub_sub_matches)?
        }
        KeysSubcommand::ProtectedShow => show_protected_keyword(trust),
        KeysSubcommand::ProtectedDelete(sub_sub_matches) => {
            delete_protected_keyword(trust, sub_sub_matches)?
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn create_keys(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    context::ensure_protected_keyword(
        trust,
        sub_matches,
        ReportOutputFormat::Text,
        "broker key creation",
    )?;
    KeysWriteDialogBuilder::new()
        .account(trust)
        .environment()
        .url()
        .key_id()
        .key_secret()
        .build()
        .display();
    Ok(())
}

pub(in crate::dispatcher) fn show_keys(trust: &mut TrustFacade) {
    KeysReadDialogBuilder::new()
        .account(trust)
        .environment()
        .build()
        .display();
}

pub(in crate::dispatcher) fn delete_keys(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    context::ensure_protected_keyword(
        trust,
        sub_matches,
        ReportOutputFormat::Text,
        "broker key deletion",
    )?;
    KeysDeleteDialogBuilder::new()
        .account(trust)
        .environment()
        .build()
        .display();
    Ok(())
}

pub(in crate::dispatcher) fn set_protected_keyword(
    _trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let value = sub_matches.get_one::<String>("value").ok_or_else(|| {
        CliError::new(
            "missing_value",
            "Missing --value for protected keyword configuration",
        )
    })?;
    let current = sub_matches
        .get_one::<String>("confirm-protected")
        .map(String::as_str);
    match protected_keyword::read_expected() {
        Ok(expected) => {
            let provided = current.ok_or_else(|| {
                    CliError::new(
                        "protected_keyword_required",
                        "Protected keyword already configured. Provide --confirm-protected <CURRENT_KEYWORD> to rotate it.",
                    )
                })?;
            if provided != expected {
                return Err(CliError::new(
                    "protected_keyword_invalid",
                    "Invalid protected keyword. Rotation denied.",
                ));
            }
            protected_keyword::store(value).map_err(|error| {
                CliError::new(
                    "protected_keyword_store_failed",
                    format!("Unable to rotate protected keyword: {error}"),
                )
            })?;
            println!("Protected mutation keyword rotated.");
        }
        Err(_) => {
            protected_keyword::store(value).map_err(|error| {
                CliError::new(
                    "protected_keyword_store_failed",
                    format!("Unable to store protected keyword: {error}"),
                )
            })?;
            println!("Protected mutation keyword stored in keychain.");
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn show_protected_keyword(_trust: &mut TrustFacade) {
    match protected_keyword::read_expected() {
        Ok(_) => println!("Protected mutation keyword is configured."),
        Err(_) => println!("Protected mutation keyword is not configured."),
    }
}

pub(in crate::dispatcher) fn delete_protected_keyword(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    context::ensure_protected_keyword(
        trust,
        sub_matches,
        ReportOutputFormat::Text,
        "protected keyword deletion",
    )?;
    protected_keyword::delete().map_err(|error| {
        CliError::new(
            "protected_keyword_delete_failed",
            format!("Unable to delete protected keyword: {error}"),
        )
    })?;
    println!("Protected mutation keyword deleted.");
    Ok(())
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct KeysCommand;
impl super::Command for KeysCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_keys(trust, args)
    }
}
