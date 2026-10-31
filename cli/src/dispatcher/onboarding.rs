//! Legacy onboarding command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_onboarding_subcommand;
use crate::command_routing::OnboardingSubcommand;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use crate::protected_keyword;
use clap::ArgMatches;
use core::TrustFacade;
use serde_json::json;

pub(in crate::dispatcher) fn dispatch_onboarding(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = output::parse_report_format(sub_matches);
    match parse_onboarding_subcommand(sub_matches) {
        OnboardingSubcommand::Init(sub_sub_matches) => {
            onboarding_init(trust, sub_sub_matches, format)?
        }
        OnboardingSubcommand::Status => onboarding_status(trust, format)?,
    }
    Ok(())
}

pub(in crate::dispatcher) fn onboarding_init(
    _trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    if protected_keyword::read_expected().is_ok() {
        return Err(output::report_error(
                format,
                "onboarding_already_initialized",
                "Protected keyword already configured. Use `trust keys protected-set --value <NEW> --confirm-protected <CURRENT>` to rotate.",
            ));
    }
    let value = sub_matches
        .get_one::<String>("protected-keyword")
        .ok_or_else(|| {
            output::report_error(format, "missing_keyword", "Missing --protected-keyword")
        })?;
    protected_keyword::store(value).map_err(|error| {
        output::report_error(
            format,
            "onboarding_store_failed",
            format!("Unable to store onboarding keyword: {error}"),
        )
    })?;
    match format {
        ReportOutputFormat::Text => {
            println!("Onboarding initialized.");
            println!("Protected mutation keyword is now configured.");
            println!("Next steps:");
            println!(
                "1. Configure broker keys with `trust keys create --confirm-protected <KEYWORD>`."
            );
            println!(
                "2. Configure risk rules with `trust rule create --confirm-protected <KEYWORD>`."
            );
            println!(
                "3. Use trading commands normally; protected operations will require the keyword."
            );
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "onboarding_init",
                "format_version": 1,
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "data": {
                    "protected_keyword_configured": true,
                    "next_steps": [
                        "trust keys create --confirm-protected <KEYWORD>",
                        "trust rule create --confirm-protected <KEYWORD>"
                    ]
                }
            });
            output::print_json(&payload)?;
        }
    }
    Ok(())
}

pub(in crate::dispatcher) fn onboarding_status(
    _trust: &mut TrustFacade,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let protected = protected_keyword::read_expected().is_ok();
    match format {
        ReportOutputFormat::Text => {
            println!("Onboarding Status");
            println!("=================");
            println!(
                "Protected keyword: {}",
                if protected { "configured" } else { "missing" }
            );
            if !protected {
                println!("Run: trust onboarding init --protected-keyword <KEYWORD>");
            }
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "onboarding_status",
                "format_version": 1,
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "data": {
                    "protected_keyword": if protected { "configured" } else { "missing" }
                }
            });
            output::print_json(&payload)?;
        }
    }
    Ok(())
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct OnboardingCommand;
impl super::Command for OnboardingCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_onboarding(trust, args)
    }
}
