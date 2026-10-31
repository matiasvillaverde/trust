//! Legacy reports command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_report_subcommand;
use crate::command_routing::ReportSubcommand;
use crate::dispatcher::attribution_report;
use crate::dispatcher::benchmark_report;
use crate::dispatcher::bias_report;
use crate::dispatcher::concentration_report;
use crate::dispatcher::error::CliError;
use crate::dispatcher::metrics_report;
use crate::dispatcher::output;
use crate::dispatcher::performance_reports;
use crate::dispatcher::risk_report;
use crate::dispatcher::summary_report;
use crate::dispatcher::tax_report;
use crate::dispatcher::timeline_report;
use crate::dispatcher::wash_sales_report;
use clap::ArgMatches;
use core::TrustFacade;

pub(in crate::dispatcher) fn dispatch_report(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = output::parse_report_format(sub_matches);
    let (handler, target_matches) = report_handler_for(parse_report_subcommand(sub_matches));
    handler(trust, target_matches, format)?;
    Ok(())
}

pub(in crate::dispatcher) fn report_handler_for(
    subcommand: ReportSubcommand<'_>,
) -> (output::FormattedCommand, &ArgMatches) {
    match subcommand {
        ReportSubcommand::Performance(args) => (performance_reports::performance_report, args),
        ReportSubcommand::Drawdown(args) => (performance_reports::drawdown_report, args),
        ReportSubcommand::Risk(args) => (risk_report::risk_report, args),
        ReportSubcommand::Concentration(args) => (concentration_report::concentration_report, args),
        ReportSubcommand::Summary(args) => (summary_report::summary_report, args),
        ReportSubcommand::Metrics(args) => (metrics_report::metrics_report, args),
        ReportSubcommand::Attribution(args) => (attribution_report::attribution_report, args),
        ReportSubcommand::Benchmark(args) => (benchmark_report::benchmark_report, args),
        ReportSubcommand::Timeline(args) => (timeline_report::timeline_report, args),
        ReportSubcommand::BiasSummary(args) => (bias_report::bias_summary_report, args),
        ReportSubcommand::WashSales(args) => (wash_sales_report::wash_sales_report, args),
        ReportSubcommand::Tax(args) => (tax_report::tax_report, args),
    }
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct ReportCommand;
impl super::Command for ReportCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_report(trust, args)
    }
}
