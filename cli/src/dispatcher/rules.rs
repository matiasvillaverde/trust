//! Legacy rules command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_rules_subcommand;
use crate::command_routing::RulesSubcommand;
use crate::dialogs::RuleDialogBuilder;
use crate::dialogs::RuleRemoveDialogBuilder;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use clap::ArgMatches;
use core::TrustFacade;

pub(in crate::dispatcher) fn dispatch_rules(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = output::parse_report_format(sub_matches);
    match parse_rules_subcommand(sub_matches) {
        RulesSubcommand::Create(sub_sub_matches) => create_rule(trust, sub_sub_matches, format)?,
        RulesSubcommand::Remove(sub_sub_matches) => remove_rule(trust, sub_sub_matches, format)?,
    }
    Ok(())
}

pub(in crate::dispatcher) fn create_rule(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    context::ensure_protected_keyword(trust, sub_matches, format, "rule creation")?;
    RuleDialogBuilder::new()
        .account(trust)
        .name()
        .risk()
        .description()
        .level()
        .build(trust)
        .display();
    Ok(())
}

pub(in crate::dispatcher) fn remove_rule(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    context::ensure_protected_keyword(trust, sub_matches, format, "rule removal")?;
    RuleRemoveDialogBuilder::new()
        .account(trust)
        .select_rule(trust)
        .build(trust)
        .display();
    Ok(())
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct RuleCommand;
impl super::Command for RuleCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_rules(trust, args)
    }
}
