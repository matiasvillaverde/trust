//! Legacy policy command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::TrustFacade;
use serde_json::json;

pub(in crate::dispatcher) fn dispatch_policy(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    policy_show(trust, output::parse_report_format(sub_matches))
}

pub(in crate::dispatcher) fn policy_show(
    _trust: &mut TrustFacade,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let protected_operations = vec![
        "account create",
        "transaction deposit",
        "transaction withdraw",
        "rule create",
        "rule remove",
        "trading-vehicle create",
        "level change",
        "level evaluate --apply",
        "keys create",
        "keys delete",
        "keys protected-set (rotation)",
        "keys protected-delete",
    ];
    let unrestricted_operations = vec![
        "trade *",
        "report *",
        "metrics *",
        "grade *",
        "level status",
        "level history",
        "level triggers",
        "level evaluate (without --apply)",
        "account search",
        "keys show",
        "keys protected-show",
        "onboarding status",
    ];

    match format {
        ReportOutputFormat::Text => {
            println!("Trust CLI Policy");
            println!("================");
            println!("Protected operations require --confirm-protected <KEYWORD>.");
            println!("Protected:");
            for operation in &protected_operations {
                println!("- {operation}");
            }
            println!("Unrestricted:");
            for operation in &unrestricted_operations {
                println!("- {operation}");
            }
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "policy",
                "format_version": 1,
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "data": {
                    "protected": protected_operations,
                    "unrestricted": unrestricted_operations
                }
            });
            output::print_json(&payload)?;
        }
    }
    Ok(())
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct PolicyCommand;
impl super::Command for PolicyCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_policy(trust, args)
    }
}
