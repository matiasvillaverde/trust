//! Legacy CLI composition root and GoF Command invoker.
use crate::command_routing::{parse_top_level_command, TopLevelCommand};
use alpaca_broker::AlpacaBroker;
use clap::ArgMatches;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
pub use error::CliError;
use ibkr_broker::IbkrBroker;
use shellexpand::tilde;
use std::{ffi::OsString, fs::create_dir_all, path::Path};
mod accounts;
mod advanced_metrics;
mod advisor_config;
mod args;
mod attribution_report;
mod autopsy;
mod benchmark_report;
mod bias_report;
mod bond_metrics;
mod capital;
mod concentration_report;
mod context;
mod database;
mod distribution;
mod error;
mod grades;
mod instrument_search;
mod instrument_stats;
mod instruments;
mod keys;
mod level_output;
mod level_policy;
mod levels;
mod market_data;
mod market_output;
mod metrics_report;
mod onboarding;
mod output;
mod performance_reports;
mod policy;
mod report_calculations;
mod report_signals;
mod reports;
mod risk_report;
mod rules;
mod sessions;
mod sizing;
mod summary_report;
mod tax_report;
mod timeline_report;
mod trade_advisor;
mod trade_commands;
mod trade_queries;
mod wash_sales_report;

trait Command {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError>;
}

pub struct ArgDispatcher {
    trust: TrustFacade,
}
impl ArgDispatcher {
    pub fn new_sqlite() -> Self {
        create_dir_if_necessary();
        let database = SqliteDatabase::new(ArgDispatcher::database_url().as_str());
        let mut trust = TrustFacade::new_with_brokers(
            Box::new(database),
            vec![Box::<AlpacaBroker>::default(), Box::<IbkrBroker>::default()],
        );
        trust.enable_protected_mode();
        ArgDispatcher { trust }
    }
    fn database_url() -> String {
        if let Ok(database_url) = std::env::var("TRUST_DB_URL") {
            return database_url;
        }

        #[cfg(debug_assertions)]
        {
            tilde("~/.trust/debug.db").to_string()
        }

        #[cfg(not(debug_assertions))]
        {
            tilde("~/.trust/production.db").to_string()
        }
    }
    pub fn dispatch(mut self, matches: ArgMatches) -> Result<(), CliError> {
        crate::zen::set_enabled(
            matches
                .try_get_one::<bool>("zen")
                .ok()
                .flatten()
                .copied()
                .unwrap_or(false),
        );
        match parse_top_level_command(&matches) {
            TopLevelCommand::Db(args) => database::DbCommand.execute(&mut self.trust, args)?,
            TopLevelCommand::Keys(args) => keys::KeysCommand.execute(&mut self.trust, args)?,
            TopLevelCommand::Accounts(args) => {
                accounts::AccountsCommand.execute(&mut self.trust, args)?
            }
            TopLevelCommand::Transaction(args) => {
                capital::TransactionCommand.execute(&mut self.trust, args)?
            }
            TopLevelCommand::Rule(args) => rules::RuleCommand.execute(&mut self.trust, args)?,
            TopLevelCommand::TradingVehicle(args) => {
                instruments::TradingVehicleCommand.execute(&mut self.trust, args)?
            }
            TopLevelCommand::Trade(args) => {
                trade_commands::TradeCommand.execute(&mut self.trust, args)?
            }
            TopLevelCommand::Distribution(args) => {
                distribution::DistributionCommand.execute(&mut self.trust, args)?
            }
            TopLevelCommand::Report(args) => {
                reports::ReportCommand.execute(&mut self.trust, args)?
            }
            TopLevelCommand::MarketData(args) => {
                market_data::MarketDataCommand.execute(&mut self.trust, args)?
            }
            TopLevelCommand::Grade(args) => grades::GradeCommand.execute(&mut self.trust, args)?,
            TopLevelCommand::Level(args) => levels::LevelCommand.execute(&mut self.trust, args)?,
            TopLevelCommand::Onboarding(args) => {
                onboarding::OnboardingCommand.execute(&mut self.trust, args)?
            }
            TopLevelCommand::Policy(args) => {
                policy::PolicyCommand.execute(&mut self.trust, args)?
            }
            TopLevelCommand::Metrics(args) => {
                advanced_metrics::MetricsCommand.execute(&mut self.trust, args)?
            }
            TopLevelCommand::Advisor(args) => {
                advisor_config::AdvisorCommand.execute(&mut self.trust, args)?
            }
            TopLevelCommand::Session(args) => {
                sessions::SessionCommand.execute(&mut self.trust, args)?
            }
            TopLevelCommand::External { name, args } => {
                let args = args
                    .get_many::<OsString>("")
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                println!("Calling out to {name:?} with {args:?}");
            }
        }
        Ok(())
    }
}
// Utils

fn create_dir_if_necessary() {
    let directory_path = tilde("~/.trust").to_string();

    // Check if directory already exists or not
    if Path::new(&directory_path).exists() {
        return;
    }

    // Create directory
    let _result = create_dir_all(&directory_path);
}
#[cfg(test)]
mod tests;
