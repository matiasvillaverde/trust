//! CLI dispatcher module - handles command-line interface operations
//!
//! SAFETY ALLOWANCES: This module contains UI code that uses .unwrap()
//! for command-line argument handling where panics are acceptable since
//! they indicate programming errors in command setup, not runtime errors.
#![allow(
    clippy::unwrap_used,
    clippy::uninlined_format_args,
    clippy::expect_used
)]

use crate::command_routing::{
    parse_accounts_subcommand, parse_advisor_subcommand, parse_db_subcommand,
    parse_distribution_subcommand, parse_grade_subcommand, parse_keys_subcommand,
    parse_level_subcommand, parse_market_data_subcommand, parse_metrics_subcommand,
    parse_onboarding_subcommand, parse_report_subcommand, parse_rules_subcommand,
    parse_session_subcommand, parse_top_level_command, parse_trade_subcommand,
    parse_trading_vehicle_subcommand, parse_transactions_subcommand, AccountsSubcommand,
    AdvisorSubcommand, DbSubcommand, DistributionSubcommand, GradeSubcommand, KeysSubcommand,
    LevelSubcommand, MarketDataSubcommand, MetricsSubcommand, OnboardingSubcommand,
    ReportSubcommand, RulesSubcommand, SessionSubcommand, TopLevelCommand, TradeSubcommand,
    TradingVehicleSubcommand, TransactionsSubcommand,
};
use crate::dialogs::{
    AccountDialogBuilder, AccountSearchDialog, CancelDialogBuilder, CloseDialogBuilder,
    ConsoleDialogIo, DialogIo, ExitDialogBuilder, FillTradeDialogBuilder, FundingDialogBuilder,
    KeysDeleteDialogBuilder, KeysReadDialogBuilder, KeysWriteDialogBuilder, ModifyDialogBuilder,
    SubmitDialogBuilder, SyncTradeDialogBuilder, TradeDialogBuilder, TradeSearchDialogBuilder,
    TradeWatchDialogBuilder, TradingVehicleDialogBuilder, TradingVehicleSearchDialogBuilder,
    TransactionDialogBuilder,
};
use crate::dialogs::{RuleDialogBuilder, RuleRemoveDialogBuilder};
use crate::protected_keyword;
use crate::trading_vehicle_import;
use advisor::{
    AdvisorConfig, AdvisorConfigUpdate, CalendarCredentials, CalendarProvider, CatalystScanRequest,
    CorrelationConfig, CorrelationRequest, PositionHeat, RegimeConfig, RegimeRequest,
};
use alpaca_broker::AlpacaBroker;
use chrono::{DateTime, Datelike, Days, NaiveDate, Utc};
use clap::ArgMatches;
use core::calculators_performance::BiasAggregation;
use core::services::leveling::{
    LevelCriterionProgress, LevelEvaluationOutcome, LevelPathProgress, LevelPerformanceSnapshot,
    LevelProgressReport,
};
use core::services::{AdvisoryThresholds, TradeProposal};
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use db_sqlite::{ImportMode, ImportOptions};
use dialoguer::Password;
use ibkr_broker::IbkrBroker;
use model::{
    Account, AccountType, BarTimeframe, BrokerKind, Currency, DraftTrade, Environment,
    FixedIncomeTerms, Level, LevelAdjustmentRules, LevelTrigger, MarketDataChannel,
    MarketSnapshotSource, Mistake, MistakeErrorType, MungerTendency, OrderCategory, SessionPlan,
    SessionPlanClose, SessionRegime, Status, Trade, TradeCategory, TradeEvent, TradeEventSeverity,
    TradeEventType, TradeGrade, TransactionCategory,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use shellexpand::tilde;
use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::fs::create_dir_all;
use std::path::Path;
use std::str::FromStr;
use std::{collections::BTreeMap, collections::HashMap};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReportOutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone)]
struct TradePreviewArgs {
    account_id: Uuid,
    entry_price: Decimal,
    stop_price: Decimal,
    currency: Currency,
}

#[derive(Debug, Clone)]
struct TradeHypothesisArgs {
    preview: TradePreviewArgs,
    target_price: Decimal,
    quantity: i64,
}

#[derive(Debug, Clone)]
struct TradeHypothesisReport {
    available_capital: Decimal,
    capital_required: Decimal,
    capital_required_pct_of_available: Option<Decimal>,
    risk_per_share: Decimal,
    reward_per_share: Decimal,
    max_loss: Decimal,
    max_loss_pct_of_available: Option<Decimal>,
    max_gain: Decimal,
    max_gain_pct_of_available: Option<Decimal>,
    risk_reward_ratio: Option<Decimal>,
}

#[derive(Debug, Clone)]
struct TradeAdvisorCheck {
    status: &'static str,
    payload: Value,
}

#[derive(Debug, Clone)]
struct SessionTradeReviewRow {
    trade_id: Uuid,
    symbol: String,
    setup: Option<String>,
    planned: bool,
}

#[derive(Debug)]
pub struct CliError {
    code: &'static str,
    message: String,
    printed: bool,
}

impl CliError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            printed: false,
        }
    }

    fn new_printed(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            printed: true,
        }
    }

    pub fn already_printed(&self) -> bool {
        self.printed
    }
}

impl Display for CliError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CliError {}

fn advisor_provider_config_requested(matches: &ArgMatches) -> bool {
    bool_arg_present(matches, "show")
        || string_arg_present(matches, "calendar-provider")
        || string_arg_present(matches, "calendar-api-key")
        || string_arg_present(matches, "claude-api-key")
}

fn advisor_threshold_config_requested(matches: &ArgMatches) -> bool {
    string_arg_present(matches, "account")
        || string_arg_present(matches, "sector-limit")
        || string_arg_present(matches, "asset-class-limit")
        || string_arg_present(matches, "single-position-limit")
        || string_arg_present(matches, "confirm-protected")
}

fn string_arg_present(matches: &ArgMatches, key: &str) -> bool {
    matches.try_get_one::<String>(key).ok().flatten().is_some()
}

fn bool_arg_present(matches: &ArgMatches, key: &str) -> bool {
    matches
        .try_get_one::<bool>(key)
        .ok()
        .flatten()
        .copied()
        .unwrap_or(false)
}

fn advisor_config_update_from_matches(
    matches: &ArgMatches,
) -> Result<AdvisorConfigUpdate, CliError> {
    let calendar_provider = matches
        .get_one::<String>("calendar-provider")
        .map(|value| {
            CalendarProvider::from_str(value)
                .map_err(|e| CliError::new("invalid_calendar_provider", e.to_string()))
        })
        .transpose()?;

    Ok(AdvisorConfigUpdate {
        calendar_provider,
        calendar_api_key: matches.get_one::<String>("calendar-api-key").cloned(),
        claude_api_key: matches.get_one::<String>("claude-api-key").cloned(),
    })
}

fn print_advisor_provider_config(config: &AdvisorConfig) {
    println!("Advisor configuration:");
    println!("Calendar provider: {}", config.calendar_provider);
    println!("Calendar API key: {}", config.calendar_api_key_display());
    println!("Claude API key: {}", config.claude_api_key_display());
}

pub struct ArgDispatcher {
    trust: TrustFacade,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TradingVehicleSearchFilters {
    category: Option<model::TradingVehicleCategory>,
    broker: Option<String>,
    symbol: Option<String>,
    isin: Option<String>,
    missing_bond_terms: bool,
    list_all: bool,
    format: ReportOutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TradingVehicleInventoryStats {
    total: usize,
    by_category: BTreeMap<String, usize>,
    by_broker: BTreeMap<String, usize>,
    bonds: BondInventoryStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BondInventoryStats {
    total: usize,
    complete_terms: usize,
    missing_terms: usize,
    coupon_rate_count: usize,
    average_coupon_rate_pct: Option<Decimal>,
    earliest_maturity: Option<NaiveDate>,
    latest_maturity: Option<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MarketDataStreamRequest {
    symbols: Vec<String>,
    channels_raw: String,
    channels: Vec<MarketDataChannel>,
    max_events: usize,
    timeout_seconds: u64,
}

trait MarketDataCommandHandler {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError>;
}

struct MarketDataSnapshotCommand;
struct MarketDataBarsCommand;
struct MarketDataStreamCommand;
struct MarketDataQuoteCommand;
struct MarketDataTradeCommand;
struct MarketDataSessionCommand;

trait ReportCommandHandler {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError>;
}

struct PerformanceReportCommand;
struct DrawdownReportCommand;
struct RiskReportCommand;
struct ConcentrationReportCommand;
struct SummaryReportCommand;
struct MetricsReportCommand;
struct AttributionReportCommand;
struct BenchmarkReportCommand;
struct TimelineReportCommand;
struct BiasSummaryReportCommand;

impl MarketDataCommandHandler for MarketDataSnapshotCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.market_data_snapshot(sub_matches, format)
    }
}

impl MarketDataCommandHandler for MarketDataBarsCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.market_data_bars(sub_matches, format)
    }
}

impl MarketDataCommandHandler for MarketDataStreamCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.market_data_stream(sub_matches, format)
    }
}

impl MarketDataCommandHandler for MarketDataQuoteCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.market_data_quote(sub_matches, format)
    }
}

impl MarketDataCommandHandler for MarketDataTradeCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.market_data_trade(sub_matches, format)
    }
}

impl MarketDataCommandHandler for MarketDataSessionCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.market_data_session(sub_matches, format)
    }
}

impl ReportCommandHandler for PerformanceReportCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.performance_report(sub_matches, format)
    }
}

impl ReportCommandHandler for DrawdownReportCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.drawdown_report(sub_matches, format)
    }
}

impl ReportCommandHandler for RiskReportCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.risk_report(sub_matches, format)
    }
}

impl ReportCommandHandler for ConcentrationReportCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.concentration_report(sub_matches, format)
    }
}

impl ReportCommandHandler for SummaryReportCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.summary_report(sub_matches, format)
    }
}

impl ReportCommandHandler for MetricsReportCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.metrics_report(sub_matches, format)
    }
}

impl ReportCommandHandler for AttributionReportCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.attribution_report(sub_matches, format)
    }
}

impl ReportCommandHandler for BenchmarkReportCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.benchmark_report(sub_matches, format)
    }
}

impl ReportCommandHandler for TimelineReportCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.timeline_report(sub_matches, format)
    }
}

impl ReportCommandHandler for BiasSummaryReportCommand {
    fn execute(
        &self,
        dispatcher: &mut ArgDispatcher,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        dispatcher.bias_summary_report(sub_matches, format)
    }
}

impl ArgDispatcher {
    fn has_non_interactive_account_args(sub_matches: &ArgMatches) -> bool {
        sub_matches.get_one::<String>("name").is_some()
            || sub_matches.get_one::<String>("description").is_some()
            || sub_matches.get_one::<String>("environment").is_some()
            || sub_matches.get_one::<String>("taxes").is_some()
            || sub_matches.get_one::<String>("earnings").is_some()
            || sub_matches.get_one::<String>("type").is_some()
            || sub_matches.get_one::<String>("broker").is_some()
            || sub_matches.get_one::<String>("broker-account-id").is_some()
            || sub_matches.get_one::<String>("parent").is_some()
    }

    fn has_non_interactive_trade_args(sub_matches: &ArgMatches) -> bool {
        sub_matches.get_one::<String>("account").is_some()
            || sub_matches.get_one::<String>("symbol").is_some()
            || sub_matches.get_one::<String>("category").is_some()
            || sub_matches.get_one::<String>("entry").is_some()
            || sub_matches.get_one::<String>("stop").is_some()
            || sub_matches.get_one::<String>("safety-order-type").is_some()
            || sub_matches.get_one::<String>("target").is_some()
            || sub_matches.get_one::<String>("quantity").is_some()
    }

    fn is_valid_protected_keyword(expected: &str, provided: &str) -> bool {
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

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn dispatch(mut self, matches: ArgMatches) -> Result<(), CliError> {
        let zen_enabled = matches
            .try_get_one::<bool>("zen")
            .ok()
            .flatten()
            .copied()
            .unwrap_or(false);
        crate::zen::set_enabled(zen_enabled);

        match parse_top_level_command(&matches) {
            TopLevelCommand::Db(sub_matches) => self.dispatch_db(sub_matches)?,
            TopLevelCommand::Keys(sub_matches) => self.dispatch_keys(sub_matches)?,
            TopLevelCommand::Accounts(sub_matches) => self.dispatch_accounts(sub_matches)?,
            TopLevelCommand::Transaction(sub_matches) => self.dispatch_transactions(sub_matches)?,
            TopLevelCommand::Rule(sub_matches) => self.dispatch_rules(sub_matches)?,
            TopLevelCommand::TradingVehicle(sub_matches) => {
                self.dispatch_trading_vehicle(sub_matches)?
            }
            TopLevelCommand::Trade(sub_matches) => self.dispatch_trade(sub_matches)?,
            TopLevelCommand::Distribution(sub_matches) => {
                self.dispatch_distribution(sub_matches)?
            }
            TopLevelCommand::Report(sub_matches) => self.dispatch_report(sub_matches)?,
            TopLevelCommand::MarketData(sub_matches) => self.dispatch_market_data(sub_matches)?,
            TopLevelCommand::Grade(sub_matches) => self.dispatch_grade(sub_matches)?,
            TopLevelCommand::Level(sub_matches) => self.dispatch_level(sub_matches)?,
            TopLevelCommand::Onboarding(sub_matches) => self.dispatch_onboarding(sub_matches)?,
            TopLevelCommand::Policy(sub_matches) => self.dispatch_policy(sub_matches)?,
            TopLevelCommand::Metrics(sub_matches) => self.dispatch_metrics(sub_matches)?,
            TopLevelCommand::Advisor(sub_matches) => self.dispatch_advisor(sub_matches)?,
            TopLevelCommand::Session(sub_matches) => self.dispatch_session(sub_matches)?,
            TopLevelCommand::External {
                name,
                args: sub_matches,
            } => {
                let args = sub_matches
                    .get_many::<OsString>("")
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                println!("Calling out to {name:?} with {args:?}");
            }
        }

        Ok(())
    }

    fn dispatch_db(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        match parse_db_subcommand(sub_matches) {
            DbSubcommand::Export(sub_sub_matches) => self.db_export(sub_sub_matches)?,
            DbSubcommand::Import(sub_sub_matches) => self.db_import(sub_sub_matches)?,
        }
        Ok(())
    }

    fn dispatch_keys(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        match parse_keys_subcommand(sub_matches) {
            KeysSubcommand::Create(sub_sub_matches) => self.create_keys(sub_sub_matches)?,
            KeysSubcommand::Show => self.show_keys(),
            KeysSubcommand::Delete(sub_sub_matches) => self.delete_keys(sub_sub_matches)?,
            KeysSubcommand::ProtectedSet(sub_sub_matches) => {
                self.set_protected_keyword(sub_sub_matches)?
            }
            KeysSubcommand::ProtectedShow => self.show_protected_keyword(),
            KeysSubcommand::ProtectedDelete(sub_sub_matches) => {
                self.delete_protected_keyword(sub_sub_matches)?
            }
        }
        Ok(())
    }

    fn dispatch_accounts(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        match parse_accounts_subcommand(sub_matches) {
            AccountsSubcommand::Create(sub_sub_matches) => self.create_account(sub_sub_matches)?,
            AccountsSubcommand::Search => self.search_account(),
            AccountsSubcommand::List(sub_sub_matches) => self.list_accounts(sub_sub_matches)?,
            AccountsSubcommand::Balance(sub_sub_matches) => {
                self.accounts_balance(sub_sub_matches)?
            }
            AccountsSubcommand::Transfer(transfer_matches) => {
                self.transfer_accounts(transfer_matches)?
            }
            AccountsSubcommand::Delete(delete_matches) => self.delete_account(delete_matches)?,
        }
        Ok(())
    }

    fn dispatch_transactions(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        match parse_transactions_subcommand(sub_matches) {
            TransactionsSubcommand::Deposit(sub_sub_matches) => self.deposit(sub_sub_matches)?,
            TransactionsSubcommand::Withdraw(sub_sub_matches) => self.withdraw(sub_sub_matches)?,
        }
        Ok(())
    }

    fn dispatch_rules(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let format = Self::parse_report_format(sub_matches);
        match parse_rules_subcommand(sub_matches) {
            RulesSubcommand::Create(sub_sub_matches) => {
                self.create_rule(sub_sub_matches, format)?
            }
            RulesSubcommand::Remove(sub_sub_matches) => {
                self.remove_rule(sub_sub_matches, format)?
            }
        }
        Ok(())
    }

    fn dispatch_trading_vehicle(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        match parse_trading_vehicle_subcommand(sub_matches) {
            TradingVehicleSubcommand::Create(sub_sub_matches) => {
                self.create_trading_vehicle(sub_sub_matches)?
            }
            TradingVehicleSubcommand::UpdateBondTerms(sub_sub_matches) => {
                self.update_bond_terms(sub_sub_matches)?
            }
            TradingVehicleSubcommand::Search(sub_sub_matches) => {
                self.search_trading_vehicle(sub_sub_matches)?
            }
            TradingVehicleSubcommand::Stats(sub_sub_matches) => {
                self.trading_vehicle_stats(sub_sub_matches)?
            }
        }
        Ok(())
    }

    fn dispatch_trade(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        match parse_trade_subcommand(sub_matches) {
            TradeSubcommand::Create(sub_sub_matches) => self.create_trade(sub_sub_matches)?,
            TradeSubcommand::Fund(sub_sub_matches) => self.create_funding(sub_sub_matches)?,
            TradeSubcommand::Cancel(sub_sub_matches) => self.create_cancel(sub_sub_matches)?,
            TradeSubcommand::Submit(sub_sub_matches) => self.create_submit(sub_sub_matches)?,
            TradeSubcommand::ManuallyFill => self.create_fill(),
            TradeSubcommand::ManuallyStop => self.create_stop(),
            TradeSubcommand::ManuallyTarget => self.create_target(),
            TradeSubcommand::ManuallyClose(close_matches) => self.close(close_matches),
            TradeSubcommand::Sync(sub_sub_matches) => self.create_sync(sub_sub_matches)?,
            TradeSubcommand::Watch(sub_sub_matches) => self.create_watch(sub_sub_matches),
            TradeSubcommand::Search(sub_sub_matches) => self.search_trade(sub_sub_matches)?,
            TradeSubcommand::ListOpen(sub_sub_matches) => self.list_open_trades(sub_sub_matches)?,
            TradeSubcommand::Reconcile(sub_sub_matches) => {
                self.reconcile_trades(sub_sub_matches)?
            }
            TradeSubcommand::ModifyStop => self.modify_stop(),
            TradeSubcommand::ModifyTarget => self.modify_target(),
            TradeSubcommand::SizePreview(sub_sub_matches) => self
                .trade_size_preview(sub_sub_matches, Self::parse_report_format(sub_sub_matches))?,
            TradeSubcommand::Hypothesis(sub_sub_matches) => {
                self.trade_hypothesis(sub_sub_matches, Self::parse_report_format(sub_sub_matches))?
            }
            TradeSubcommand::Advisor(sub_sub_matches) => {
                self.trade_advisor(sub_sub_matches, Self::parse_report_format(sub_sub_matches))?
            }
            TradeSubcommand::Events(sub_sub_matches) => self.trade_events(sub_sub_matches)?,
            TradeSubcommand::Autopsy(sub_sub_matches) => self.trade_autopsy(sub_sub_matches)?,
        }
        Ok(())
    }

    fn dispatch_distribution(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        match parse_distribution_subcommand(sub_matches) {
            DistributionSubcommand::Configure(configure_matches) => {
                self.configure_distribution(configure_matches)?
            }
            DistributionSubcommand::Execute(execute_matches) => {
                self.execute_distribution(execute_matches)?
            }
            DistributionSubcommand::History(history_matches) => {
                self.distribution_history(history_matches)?
            }
            DistributionSubcommand::Rules(rules_matches) => {
                if let Some(("show", nested)) = rules_matches.subcommand() {
                    self.distribution_rules(nested)?
                } else {
                    self.distribution_rules(rules_matches)?
                }
            }
        }
        Ok(())
    }

    fn dispatch_report(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let format = Self::parse_report_format(sub_matches);
        let (handler, target_matches) =
            Self::report_handler_for(parse_report_subcommand(sub_matches));
        handler.execute(self, target_matches, format)?;
        Ok(())
    }

    fn report_handler_for(
        subcommand: ReportSubcommand<'_>,
    ) -> (&'static dyn ReportCommandHandler, &ArgMatches) {
        static PERFORMANCE: PerformanceReportCommand = PerformanceReportCommand;
        static DRAWDOWN: DrawdownReportCommand = DrawdownReportCommand;
        static RISK: RiskReportCommand = RiskReportCommand;
        static CONCENTRATION: ConcentrationReportCommand = ConcentrationReportCommand;
        static SUMMARY: SummaryReportCommand = SummaryReportCommand;
        static METRICS: MetricsReportCommand = MetricsReportCommand;
        static ATTRIBUTION: AttributionReportCommand = AttributionReportCommand;
        static BENCHMARK: BenchmarkReportCommand = BenchmarkReportCommand;
        static TIMELINE: TimelineReportCommand = TimelineReportCommand;
        static BIAS_SUMMARY: BiasSummaryReportCommand = BiasSummaryReportCommand;

        match subcommand {
            ReportSubcommand::Performance(sub_matches) => (&PERFORMANCE, sub_matches),
            ReportSubcommand::Drawdown(sub_matches) => (&DRAWDOWN, sub_matches),
            ReportSubcommand::Risk(sub_matches) => (&RISK, sub_matches),
            ReportSubcommand::Concentration(sub_matches) => (&CONCENTRATION, sub_matches),
            ReportSubcommand::Summary(sub_matches) => (&SUMMARY, sub_matches),
            ReportSubcommand::Metrics(sub_matches) => (&METRICS, sub_matches),
            ReportSubcommand::Attribution(sub_matches) => (&ATTRIBUTION, sub_matches),
            ReportSubcommand::Benchmark(sub_matches) => (&BENCHMARK, sub_matches),
            ReportSubcommand::Timeline(sub_matches) => (&TIMELINE, sub_matches),
            ReportSubcommand::BiasSummary(sub_matches) => (&BIAS_SUMMARY, sub_matches),
        }
    }

    fn dispatch_market_data(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let format = Self::parse_report_format(sub_matches);
        let subcommand = parse_market_data_subcommand(sub_matches);
        let (handler, target_matches) = Self::market_data_handler_for(subcommand);
        handler.execute(self, target_matches, format)?;
        Ok(())
    }

    fn market_data_handler_for(
        subcommand: MarketDataSubcommand<'_>,
    ) -> (&'static dyn MarketDataCommandHandler, &ArgMatches) {
        static SNAPSHOT: MarketDataSnapshotCommand = MarketDataSnapshotCommand;
        static BARS: MarketDataBarsCommand = MarketDataBarsCommand;
        static STREAM: MarketDataStreamCommand = MarketDataStreamCommand;
        static QUOTE: MarketDataQuoteCommand = MarketDataQuoteCommand;
        static TRADE: MarketDataTradeCommand = MarketDataTradeCommand;
        static SESSION: MarketDataSessionCommand = MarketDataSessionCommand;

        match subcommand {
            MarketDataSubcommand::Snapshot(sub_matches) => (&SNAPSHOT, sub_matches),
            MarketDataSubcommand::Bars(sub_matches) => (&BARS, sub_matches),
            MarketDataSubcommand::Stream(sub_matches) => (&STREAM, sub_matches),
            MarketDataSubcommand::Quote(sub_matches) => (&QUOTE, sub_matches),
            MarketDataSubcommand::Trade(sub_matches) => (&TRADE, sub_matches),
            MarketDataSubcommand::Session(sub_matches) => (&SESSION, sub_matches),
        }
    }

    fn dispatch_grade(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let format = Self::parse_report_format(sub_matches);
        match parse_grade_subcommand(sub_matches) {
            GradeSubcommand::Show(sub_sub_matches) => self.grade_show(sub_sub_matches, format)?,
            GradeSubcommand::Summary(sub_sub_matches) => {
                self.grade_summary(sub_sub_matches, format)?
            }
        }
        Ok(())
    }

    fn dispatch_level(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let format = Self::parse_report_format(sub_matches);
        match parse_level_subcommand(sub_matches) {
            LevelSubcommand::Status(sub_sub_matches) => {
                self.level_status(sub_sub_matches, format)?
            }
            LevelSubcommand::Triggers(sub_sub_matches) => {
                self.level_triggers(sub_sub_matches, format)?
            }
            LevelSubcommand::History(sub_sub_matches) => {
                self.level_history(sub_sub_matches, format)?
            }
            LevelSubcommand::Change(sub_sub_matches) => {
                self.level_change(sub_sub_matches, format)?
            }
            LevelSubcommand::Evaluate(sub_sub_matches) => {
                self.level_evaluate(sub_sub_matches, format)?
            }
            LevelSubcommand::Progress(sub_sub_matches) => {
                self.level_progress(sub_sub_matches, format)?
            }
            LevelSubcommand::RulesShow(sub_sub_matches) => {
                self.level_rules_show(sub_sub_matches, format)?
            }
            LevelSubcommand::RulesSet(sub_sub_matches) => {
                self.level_rules_set(sub_sub_matches, format)?
            }
        }
        Ok(())
    }

    fn dispatch_onboarding(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let format = Self::parse_report_format(sub_matches);
        match parse_onboarding_subcommand(sub_matches) {
            OnboardingSubcommand::Init(sub_sub_matches) => {
                self.onboarding_init(sub_sub_matches, format)?
            }
            OnboardingSubcommand::Status => self.onboarding_status(format)?,
        }
        Ok(())
    }

    fn dispatch_policy(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        self.policy_show(Self::parse_report_format(sub_matches))
    }

    fn dispatch_metrics(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        match parse_metrics_subcommand(sub_matches) {
            MetricsSubcommand::Advanced(sub_sub_matches) => self.metrics_advanced(sub_sub_matches),
            MetricsSubcommand::Compare(sub_sub_matches) => self.metrics_compare(sub_sub_matches),
            MetricsSubcommand::Bond(sub_sub_matches) => self.metrics_bond(sub_sub_matches)?,
        }
        Ok(())
    }

    fn dispatch_advisor(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        match parse_advisor_subcommand(sub_matches) {
            AdvisorSubcommand::Configure(sub_sub_matches) => {
                self.advisor_configure(sub_sub_matches)?
            }
            AdvisorSubcommand::Check(sub_sub_matches) => self.advisor_check(sub_sub_matches)?,
            AdvisorSubcommand::Status(sub_sub_matches) => self.advisor_status(sub_sub_matches)?,
            AdvisorSubcommand::History(sub_sub_matches) => self.advisor_history(sub_sub_matches)?,
        }
        Ok(())
    }

    fn dispatch_session(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let format = Self::parse_report_format(sub_matches);
        match parse_session_subcommand(sub_matches) {
            SessionSubcommand::Open(sub_sub_matches) => {
                self.session_open(sub_sub_matches, format)?
            }
            SessionSubcommand::Close(sub_sub_matches) => {
                self.session_close(sub_sub_matches, format)?
            }
            SessionSubcommand::List(sub_sub_matches) => {
                self.session_list(sub_sub_matches, format)?
            }
        }
        Ok(())
    }

    fn parse_report_format(matches: &ArgMatches) -> ReportOutputFormat {
        match matches.get_one::<String>("format").map(String::as_str) {
            Some("json") => ReportOutputFormat::Json,
            _ => ReportOutputFormat::Text,
        }
    }

    fn decimal_string(value: Decimal) -> String {
        value.round_dp(8).normalize().to_string()
    }

    fn print_json(payload: &Value) -> Result<(), CliError> {
        println!("{payload:#}");
        Ok(())
    }

    fn report_error(
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

    fn parse_decimal_arg(
        sub_matches: &ArgMatches,
        key: &'static str,
        format: ReportOutputFormat,
    ) -> Result<Decimal, CliError> {
        let raw = sub_matches.get_one::<String>(key).ok_or_else(|| {
            Self::report_error(
                format,
                "missing_argument",
                format!("Missing argument --{key}"),
            )
        })?;
        Decimal::from_str(raw).map_err(|_| {
            Self::report_error(
                format,
                "invalid_decimal",
                format!("Invalid decimal value for --{key}: {raw}"),
            )
        })
    }

    fn parse_optional_decimal_arg(
        sub_matches: &ArgMatches,
        key: &'static str,
        format: ReportOutputFormat,
    ) -> Result<Option<Decimal>, CliError> {
        sub_matches
            .get_one::<String>(key)
            .map(|raw| {
                Decimal::from_str(raw).map_err(|_| {
                    Self::report_error(
                        format,
                        "invalid_decimal",
                        format!("Invalid decimal value for --{key}: {raw}"),
                    )
                })
            })
            .transpose()
    }

    fn parse_uuid_arg(
        sub_matches: &ArgMatches,
        key: &'static str,
        format: ReportOutputFormat,
    ) -> Result<Uuid, CliError> {
        let raw = sub_matches.get_one::<String>(key).ok_or_else(|| {
            Self::report_error(
                format,
                "missing_argument",
                format!("Missing argument --{key}"),
            )
        })?;
        Uuid::from_str(raw).map_err(|_| {
            Self::report_error(
                format,
                "invalid_uuid",
                format!("Invalid UUID for --{key}: {raw}"),
            )
        })
    }

    fn transactions_for_scope(
        &mut self,
        account_id: Option<Uuid>,
        days_filter: Option<u32>,
    ) -> Vec<model::transaction::Transaction> {
        let mut transactions = if let Some(id) = account_id {
            self.trust
                .get_account_transactions(id)
                .unwrap_or_else(|_| Vec::new())
        } else {
            self.trust
                .get_all_transactions()
                .unwrap_or_else(|_| Vec::new())
        };

        if let Some(days) = days_filter {
            let now = Utc::now().naive_utc();
            let cutoff = now
                .checked_sub_signed(chrono::Duration::days(i64::from(days)))
                .unwrap_or(now);
            transactions.retain(|tx| tx.created_at >= cutoff);
        }

        transactions
    }

    fn drawdown_metrics_for_transactions(
        transactions: &[model::transaction::Transaction],
    ) -> core::calculators_drawdown::DrawdownMetrics {
        use core::calculators_drawdown::{
            DrawdownMetrics, RealizedDrawdownCalculator, RealizedEquityCurve,
        };

        let curve = RealizedDrawdownCalculator::calculate_equity_curve(transactions)
            .unwrap_or(RealizedEquityCurve { points: Vec::new() });

        RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap_or(DrawdownMetrics {
            current_equity: dec!(0),
            peak_equity: dec!(0),
            current_drawdown: dec!(0),
            current_drawdown_percentage: dec!(0),
            max_drawdown: dec!(0),
            max_drawdown_percentage: dec!(0),
            max_drawdown_date: None,
            recovery_from_max: dec!(0),
            recovery_percentage: dec!(0),
            days_since_peak: 0,
            days_in_drawdown: 0,
        })
    }

    fn starting_equity_for_transactions(
        transactions: &[model::transaction::Transaction],
    ) -> Option<Decimal> {
        use core::calculators_drawdown::RealizedDrawdownCalculator;

        let curve = RealizedDrawdownCalculator::calculate_equity_curve(transactions).ok()?;
        curve.points.first().map(|point| point.balance)
    }

    fn report_calmar_ratio(
        closed_trades: &[Trade],
        transactions: &[model::transaction::Transaction],
        drawdown_metrics: &core::calculators_drawdown::DrawdownMetrics,
    ) -> Option<Decimal> {
        let starting_equity = Self::starting_equity_for_transactions(transactions)?;
        core::calculators_advanced_metrics::AdvancedMetricsCalculator::calculate_calmar_ratio_for_report(
            closed_trades,
            starting_equity,
            drawdown_metrics.max_drawdown_percentage,
        )
    }

    fn resolve_account_arg(
        &mut self,
        raw: &str,
        format: ReportOutputFormat,
    ) -> Result<Account, CliError> {
        if let Ok(id) = Uuid::from_str(raw) {
            let accounts = self.trust.search_all_accounts().map_err(|error| {
                Self::report_error(
                    format,
                    "accounts_unavailable",
                    format!("Failed to load accounts: {error}"),
                )
            })?;
            return accounts
                .into_iter()
                .find(|account| account.id == id)
                .ok_or_else(|| {
                    Self::report_error(
                        format,
                        "account_not_found",
                        format!("No account found for id: {id}"),
                    )
                });
        }

        self.trust.search_account(raw).map_err(|error| {
            Self::report_error(
                format,
                "account_not_found",
                format!("No account found for '{raw}': {error}"),
            )
        })
    }

    fn account_by_id(
        &mut self,
        account_id: Uuid,
        format: ReportOutputFormat,
    ) -> Result<Account, CliError> {
        let accounts = self.trust.search_all_accounts().map_err(|error| {
            Self::report_error(
                format,
                "accounts_unavailable",
                format!("Failed to load accounts: {error}"),
            )
        })?;

        accounts
            .into_iter()
            .find(|account| account.id == account_id)
            .ok_or_else(|| {
                Self::report_error(
                    format,
                    "account_not_found",
                    format!("No account found for id: {account_id}"),
                )
            })
    }

    fn find_trade_by_id(
        &mut self,
        trade_id: Uuid,
        statuses: &[model::Status],
        format: ReportOutputFormat,
    ) -> Result<Trade, CliError> {
        let accounts = self.trust.search_all_accounts().map_err(|error| {
            Self::report_error(
                format,
                "accounts_unavailable",
                format!("Failed to load accounts: {error}"),
            )
        })?;

        for account in accounts {
            for status in statuses {
                if let Ok(trades) = self.trust.search_trades(account.id, *status) {
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
        Err(Self::report_error(
            format,
            "trade_not_found",
            format!("Trade {trade_id} not found in statuses [{statuses_str}] across all accounts"),
        ))
    }

    fn ensure_protected_keyword(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
        operation: &'static str,
    ) -> Result<(), CliError> {
        let expected = protected_keyword::read_expected().map_err(|_| {
            Self::report_error(
                format,
                "protected_keyword_not_configured",
                "Protected keyword is not configured. Run: trust keys protected-set --value <KEYWORD>",
            )
        })?;

        let provided = sub_matches
            .get_one::<String>("confirm-protected")
            .map(String::as_str)
            .ok_or_else(|| {
                Self::report_error(
                    format,
                    "protected_keyword_required",
                    format!("{operation} is protected. Provide --confirm-protected <KEYWORD>"),
                )
            })?;

        if !Self::is_valid_protected_keyword(&expected, provided) {
            return Err(Self::report_error(
                format,
                "protected_keyword_invalid",
                format!("Invalid protected keyword for {operation}."),
            ));
        }

        self.trust
            .authorize_protected_mutation(provided, &expected)
            .map_err(|_| {
                Self::report_error(
                    format,
                    "protected_keyword_invalid",
                    format!("Invalid protected keyword for {operation}."),
                )
            })?;
        Ok(())
    }

    fn resolve_level_account_id(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<Uuid, CliError> {
        if let Some(account_arg) = sub_matches.get_one::<String>("account") {
            return Uuid::from_str(account_arg).map_err(|_| {
                Self::report_error(format, "invalid_account_id", "Invalid account ID format")
            });
        }

        let accounts = self.trust.search_all_accounts().map_err(|error| {
            Self::report_error(
                format,
                "accounts_unavailable",
                format!("Failed to load accounts: {error}"),
            )
        })?;

        if let [account] = accounts.as_slice() {
            return Ok(account.id);
        }

        Err(Self::report_error(
            format,
            "account_selection_required",
            "Use --account when multiple accounts exist",
        ))
    }

    fn level_snapshot_from_args(
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<LevelPerformanceSnapshot, CliError> {
        Ok(LevelPerformanceSnapshot {
            profitable_trades: *sub_matches.get_one::<u32>("profitable-trades").ok_or_else(
                || {
                    Self::report_error(
                        format,
                        "missing_profitable_trades",
                        "Missing --profitable-trades",
                    )
                },
            )?,
            win_rate_percentage: Self::parse_decimal_arg(sub_matches, "win-rate", format)?,
            monthly_loss_percentage: Self::parse_decimal_arg(sub_matches, "monthly-loss", format)?,
            largest_loss_percentage: Self::parse_decimal_arg(sub_matches, "largest-loss", format)?,
            consecutive_wins: *sub_matches
                .get_one::<u32>("consecutive-wins")
                .ok_or_else(|| {
                    Self::report_error(
                        format,
                        "missing_consecutive_wins",
                        "Missing --consecutive-wins",
                    )
                })?,
        })
    }

    fn print_level_evaluation_text(outcome: &LevelEvaluationOutcome, apply: bool) {
        println!("Level evaluation complete.");
        println!(
            "Current: L{} ({})",
            outcome.current_level.current_level,
            model::Level::level_description(outcome.current_level.current_level)
        );
        if let Some(decision) = &outcome.decision {
            println!(
                "Decision: {:?} to L{} ({})",
                decision.direction, decision.target_level, decision.reason
            );
        } else {
            println!("Decision: no change");
        }
        if let Some(applied) = &outcome.applied_level {
            println!("Applied: new level L{}", applied.current_level);
        } else if apply {
            println!("Applied: no change");
        }
        Self::print_level_progress_text(&outcome.progress);
    }

    fn level_evaluation_payload(
        account_id: Uuid,
        apply: bool,
        outcome: &LevelEvaluationOutcome,
    ) -> Value {
        json!({
            "report": "level_evaluate",
            "format_version": 1,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "scope": { "account_id": account_id.to_string() },
            "apply": apply,
            "data": {
                "current_level": outcome.current_level.current_level,
                "decision": outcome.decision.as_ref().map(|decision| {
                    json!({
                        "target_level": decision.target_level,
                        "direction": format!("{:?}", decision.direction),
                        "reason": decision.reason,
                        "trigger_type": decision.trigger_type.to_string()
                    })
                }),
                "applied_level": outcome.applied_level.as_ref().map(|level| level.current_level),
                "progress": Self::level_progress_payload(&outcome.progress),
            }
        })
    }

    fn print_level_progress_text(progress: &LevelProgressReport) {
        println!("Progress to adjacent levels:");

        if progress.upgrade_paths.is_empty() {
            println!("- Upgrade: unavailable at current bounds");
        } else {
            for path in &progress.upgrade_paths {
                Self::print_level_path_text("Upgrade", path);
            }
        }

        if progress.downgrade_paths.is_empty() {
            println!("- Downgrade: unavailable at current bounds");
        } else {
            for path in &progress.downgrade_paths {
                Self::print_level_path_text("Downgrade", path);
            }
        }
    }

    fn print_level_path_text(direction_label: &str, path: &LevelPathProgress) {
        let target = path
            .target_level
            .map_or_else(|| "n/a".to_string(), |level| format!("L{level}"));
        println!(
            "- {direction_label} via {} -> {} [{}]",
            path.path,
            target,
            if path.all_met { "ready" } else { "not ready" }
        );
        for criterion in &path.criteria {
            Self::print_level_criterion_text(criterion);
        }
    }

    fn print_level_criterion_text(criterion: &LevelCriterionProgress) {
        println!(
            "  - {} {} {} (actual {}, missing {})",
            criterion.key,
            criterion.comparator,
            criterion.threshold,
            criterion.actual,
            criterion.missing
        );
    }

    fn level_progress_payload(progress: &LevelProgressReport) -> Value {
        json!({
            "current_level": progress.current_level,
            "status": progress.status.to_string(),
            "upgrade_paths": progress.upgrade_paths.iter().map(Self::level_path_payload).collect::<Vec<_>>(),
            "downgrade_paths": progress.downgrade_paths.iter().map(Self::level_path_payload).collect::<Vec<_>>(),
        })
    }

    fn level_path_payload(path: &LevelPathProgress) -> Value {
        json!({
            "path": path.path,
            "trigger_type": path.trigger_type.to_string(),
            "direction": format!("{:?}", path.direction),
            "target_level": path.target_level,
            "all_met": path.all_met,
            "criteria": path.criteria.iter().map(Self::level_criterion_payload).collect::<Vec<_>>(),
        })
    }

    fn level_criterion_payload(criterion: &LevelCriterionProgress) -> Value {
        json!({
            "key": criterion.key,
            "comparator": criterion.comparator,
            "actual": Self::decimal_string(criterion.actual),
            "threshold": Self::decimal_string(criterion.threshold),
            "missing": Self::decimal_string(criterion.missing),
            "met": criterion.met,
        })
    }

    fn summarize_agent_status(breaches: &[String]) -> &'static str {
        if breaches.iter().any(|b| b.starts_with("critical:")) {
            "critical"
        } else if breaches.iter().any(|b| b.starts_with("warn:")) {
            "warn"
        } else {
            "ok"
        }
    }

    fn summary_agent_signals(
        risk_pct: Decimal,
        top_concentration_pct: Decimal,
        max_drawdown_pct: Decimal,
        expectancy: Decimal,
    ) -> Value {
        let mut breaches: Vec<String> = Vec::new();
        let mut actions: Vec<String> = Vec::new();

        if risk_pct > dec!(20) {
            breaches.push("critical:capital_at_risk_above_20pct".to_string());
            actions.push("Reduce open exposure below 20% of equity".to_string());
        } else if risk_pct > dec!(10) {
            breaches.push("warn:capital_at_risk_above_10pct".to_string());
            actions.push("Review position sizing to reduce capital at risk".to_string());
        }

        if top_concentration_pct > dec!(60) {
            breaches.push("critical:concentration_above_60pct".to_string());
            actions.push("Diversify top concentration bucket below 60%".to_string());
        } else if top_concentration_pct > dec!(50) {
            breaches.push("warn:concentration_above_50pct".to_string());
            actions.push("Hedge or rebalance concentrated exposure".to_string());
        }

        if max_drawdown_pct > dec!(15) {
            breaches.push("critical:max_drawdown_above_15pct".to_string());
            actions.push("Pause new risk and review drawdown recovery plan".to_string());
        } else if max_drawdown_pct > dec!(10) {
            breaches.push("warn:max_drawdown_above_10pct".to_string());
            actions.push("Tighten risk controls during drawdown".to_string());
        }

        if expectancy < dec!(0) {
            breaches.push("warn:negative_expectancy".to_string());
            actions.push("Stop low-edge setups and review strategy quality".to_string());
        }

        json!({
            "status": Self::summarize_agent_status(&breaches),
            "breaches": breaches,
            "recommended_actions": actions
        })
    }

    fn metrics_agent_signals(
        expectancy: Decimal,
        win_rate_pct: Decimal,
        risk_of_ruin_proxy: Option<Decimal>,
        max_consecutive_losses: u32,
    ) -> Value {
        let mut breaches: Vec<String> = Vec::new();
        let mut actions: Vec<String> = Vec::new();

        if expectancy < dec!(0) {
            breaches.push("critical:negative_expectancy".to_string());
            actions.push("Disable strategy variants with negative expectancy".to_string());
        }
        if win_rate_pct < dec!(40) {
            breaches.push("warn:win_rate_below_40pct".to_string());
            actions.push("Increase selectivity and validate entries".to_string());
        }
        if max_consecutive_losses >= 5 {
            breaches.push("warn:loss_streak_ge_5".to_string());
            actions.push("Apply cooldown after long losing streaks".to_string());
        }
        if let Some(risk) = risk_of_ruin_proxy {
            if risk > dec!(0.4) {
                breaches.push("critical:risk_of_ruin_proxy_above_40pct".to_string());
                actions.push("Cut risk per trade and lower leverage".to_string());
            } else if risk > dec!(0.2) {
                breaches.push("warn:risk_of_ruin_proxy_above_20pct".to_string());
                actions.push("Reduce position size until risk-of-ruin improves".to_string());
            }
        }

        json!({
            "status": Self::summarize_agent_status(&breaches),
            "breaches": breaches,
            "recommended_actions": actions
        })
    }
}

// Account
impl ArgDispatcher {
    #[allow(clippy::too_many_lines)]
    fn create_account(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        self.ensure_protected_keyword(sub_matches, ReportOutputFormat::Text, "account creation")?;
        if Self::has_non_interactive_account_args(sub_matches) {
            let name = sub_matches.get_one::<String>("name").cloned();
            let description = sub_matches.get_one::<String>("description").cloned();
            let environment = sub_matches.get_one::<String>("environment").cloned();
            let taxes = sub_matches.get_one::<String>("taxes").cloned();
            let earnings = sub_matches.get_one::<String>("earnings").cloned();
            let account_type = sub_matches.get_one::<String>("type").cloned();
            let broker = sub_matches.get_one::<String>("broker").cloned();
            let broker_account_id = sub_matches.get_one::<String>("broker-account-id").cloned();
            let parent = sub_matches.get_one::<String>("parent").cloned();

            let name = name.ok_or_else(|| CliError::new("missing_name", "Missing --name"))?;
            let description = description
                .ok_or_else(|| CliError::new("missing_description", "Missing --description"))?;
            let environment = environment
                .ok_or_else(|| CliError::new("missing_environment", "Missing --environment"))?;
            let taxes = taxes.ok_or_else(|| CliError::new("missing_taxes", "Missing --taxes"))?;
            let earnings =
                earnings.ok_or_else(|| CliError::new("missing_earnings", "Missing --earnings"))?;

            let environment = Self::parse_environment(&environment)?;
            let taxes_percentage = Decimal::from_str_exact(&taxes)
                .map_err(|_| CliError::new("invalid_decimal", "Invalid --taxes decimal"))?
                .checked_div(Decimal::new(100, 0))
                .ok_or_else(|| CliError::new("invalid_decimal", "Invalid --taxes percentage"))?;
            let earnings_percentage = Decimal::from_str_exact(&earnings)
                .map_err(|_| CliError::new("invalid_decimal", "Invalid --earnings decimal"))?
                .checked_div(Decimal::new(100, 0))
                .ok_or_else(|| CliError::new("invalid_decimal", "Invalid --earnings percentage"))?;

            let account_type = match account_type {
                Some(v) => Self::parse_account_type(&v)?,
                None => AccountType::Primary,
            };
            let broker_kind = match broker {
                Some(v) => Self::parse_broker_kind(&v)?,
                None => BrokerKind::Alpaca,
            };
            let parent_account_id = match parent {
                Some(v) => Some(
                    Uuid::parse_str(&v)
                        .map_err(|_| CliError::new("invalid_uuid", "Invalid --parent UUID"))?,
                ),
                None => None,
            };
            let broker_account_id = broker_account_id.filter(|value| !value.trim().is_empty());

            if broker_kind == BrokerKind::Ibkr
                && account_type == AccountType::Primary
                && broker_account_id.is_none()
            {
                return Err(CliError::new(
                    "missing_broker_account_id",
                    "Missing --broker-account-id for an IBKR primary account",
                ));
            }

            let account = self
                .trust
                .create_account_with_profile(
                    &name,
                    &description,
                    environment,
                    taxes_percentage,
                    earnings_percentage,
                    account_type,
                    parent_account_id,
                    broker_kind,
                    broker_account_id.as_deref(),
                )
                .map_err(|e| CliError::new("create_account_failed", e.to_string()))?;
            println!("Account created:");
            println!("  id: {}", account.id);
            println!("  name: {}", account.name);
            println!("  type: {}", account.account_type);
            println!("  broker: {}", account.broker_kind);
            println!(
                "  broker_account_id: {}",
                account.broker_account_id.as_deref().unwrap_or("none")
            );
            println!(
                "  parent: {}",
                account
                    .parent_account_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "none".to_string())
            );
        } else {
            AccountDialogBuilder::new()
                .name()
                .description()
                .environment()
                .tax_percentage()
                .earnings_percentage()
                .build(&mut self.trust)
                .display();
        }
        Ok(())
    }

    fn search_account(&mut self) {
        AccountSearchDialog::new()
            .search(&mut self.trust)
            .display(&mut self.trust);
    }

    fn list_accounts(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        let hierarchy = matches.get_flag("hierarchy");
        let accounts = self
            .trust
            .search_all_accounts()
            .map_err(|e| CliError::new("accounts_list_failed", e.to_string()))?;

        if accounts.is_empty() {
            println!("No accounts found.");
            return Ok(());
        }

        if !hierarchy {
            for account in accounts {
                println!("{} {} {}", account.id, account.name, account.account_type);
            }
            return Ok(());
        }

        let mut roots: Vec<_> = accounts
            .iter()
            .filter(|a| a.parent_account_id.is_none())
            .cloned()
            .collect();
        roots.sort_by(|a, b| a.name.cmp(&b.name));

        for root in roots {
            println!("{} ({}) [{}]", root.name, root.id, root.account_type);
            let mut children: Vec<_> = accounts
                .iter()
                .filter(|a| a.parent_account_id == Some(root.id))
                .cloned()
                .collect();
            children.sort_by(|a, b| a.name.cmp(&b.name));
            for child in children {
                println!("  - {} ({}) [{}]", child.name, child.id, child.account_type);
            }
        }
        Ok(())
    }

    fn accounts_balance(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        let detailed = matches.get_flag("detailed");
        let accounts = self
            .trust
            .search_all_accounts()
            .map_err(|e| CliError::new("accounts_balance_failed", e.to_string()))?;
        if accounts.is_empty() {
            println!("No accounts found.");
            return Ok(());
        }

        for account in accounts {
            let balances = self
                .trust
                .search_all_balances(account.id)
                .map_err(|e| CliError::new("accounts_balance_failed", e.to_string()))?;

            if detailed {
                println!(
                    "{} ({}) [{}]",
                    account.name, account.id, account.account_type
                );
                for b in balances {
                    let basis = b.total_balance;
                    println!(
                        "  {} total={} available={} in_trade={} taxed={} earnings={}",
                        b.currency,
                        crate::zen::amount_share(b.total_balance, basis),
                        crate::zen::amount_share(b.total_available, basis),
                        crate::zen::amount_share(b.total_in_trade, basis),
                        crate::zen::amount_share(b.taxed, basis),
                        crate::zen::amount_share(b.total_earnings, basis)
                    );
                }
            } else {
                let total = balances.into_iter().fold(Decimal::ZERO, |acc, b| {
                    acc.checked_add(b.total_balance).unwrap_or(acc)
                });
                println!(
                    "{} ({}) total={}",
                    account.name,
                    account.id,
                    crate::zen::amount_share(total, total)
                );
            }
        }
        Ok(())
    }

    fn delete_account(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        self.ensure_protected_keyword(matches, ReportOutputFormat::Text, "account deletion")?;
        let account_id = matches
            .get_one::<String>("account")
            .ok_or_else(|| CliError::new("missing_account", "Missing --account"))?;
        let account_id = Uuid::parse_str(account_id)
            .map_err(|_| CliError::new("invalid_uuid", "Invalid --account UUID"))?;
        let force = matches.get_flag("force");

        let account = self
            .trust
            .delete_account(account_id, force)
            .map_err(|e| CliError::new("delete_account_failed", e.to_string()))?;

        println!("Account deleted:");
        println!("  id: {}", account.id);
        println!("  name: {}", account.name);
        println!("  force: {}", force);
        Ok(())
    }

    fn parse_environment(raw: &str) -> Result<Environment, CliError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "paper" | "sandbox" => Ok(Environment::Paper),
            "live" | "production" => Ok(Environment::Live),
            _ => Err(CliError::new(
                "invalid_environment",
                "Invalid --environment value (expected paper|live)",
            )),
        }
    }

    fn parse_account_type(raw: &str) -> Result<AccountType, CliError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "primary" => Ok(AccountType::Primary),
            "earnings" => Ok(AccountType::Earnings),
            "tax-reserve" | "tax_reserve" => Ok(AccountType::TaxReserve),
            "reinvestment" => Ok(AccountType::Reinvestment),
            _ => Err(CliError::new(
                "invalid_account_type",
                "Invalid --type value (expected primary|earnings|tax-reserve|reinvestment)",
            )),
        }
    }

    fn parse_broker_kind(raw: &str) -> Result<BrokerKind, CliError> {
        raw.parse::<BrokerKind>().map_err(|_| {
            CliError::new(
                "invalid_broker_kind",
                "Invalid --broker value (expected alpaca|ibkr)",
            )
        })
    }

    fn parse_trading_vehicle_category(
        raw: &str,
    ) -> Result<model::TradingVehicleCategory, CliError> {
        raw.parse::<model::TradingVehicleCategory>().map_err(|_| {
            CliError::new(
                "invalid_trading_vehicle_category",
                "Invalid --category value (expected stock|etf|bond|crypto|fiat)",
            )
        })
    }
}

// Transaction
impl ArgDispatcher {
    fn has_non_interactive_transaction_args(sub_matches: &ArgMatches) -> bool {
        sub_matches.get_one::<String>("account").is_some()
            || sub_matches.get_one::<String>("currency").is_some()
            || sub_matches.get_one::<String>("amount").is_some()
    }

    fn create_transaction_non_interactive(
        &mut self,
        sub_matches: &ArgMatches,
        category: TransactionCategory,
    ) -> Result<(), CliError> {
        let format = ReportOutputFormat::Text;
        let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --account")
        })?;
        let currency_raw = sub_matches.get_one::<String>("currency").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --currency")
        })?;
        let amount_raw = sub_matches.get_one::<String>("amount").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --amount")
        })?;

        let account = self.resolve_account_arg(account_raw, format)?;
        let currency = Currency::from_str(currency_raw.trim().to_ascii_uppercase().as_str())
            .map_err(|_| {
                Self::report_error(
                    format,
                    "invalid_currency",
                    format!(
                        "Invalid --currency value: {}",
                        currency_raw.to_ascii_lowercase()
                    ),
                )
            })?;
        let amount = Decimal::from_str_exact(amount_raw.trim()).map_err(|_| {
            Self::report_error(
                format,
                "invalid_decimal",
                format!("Invalid --amount decimal: {amount_raw}"),
            )
        })?;

        let (transaction, account_balance) = self
            .trust
            .create_transaction(&account, &category, amount, &currency)
            .map_err(|e| Self::report_error(format, "transaction_create_failed", e.to_string()))?;

        crate::views::TransactionView::display_with_basis(
            &transaction,
            &account.name,
            account_balance.total_balance,
        );
        crate::views::AccountBalanceView::display(account_balance, &account.name);
        Ok(())
    }

    fn deposit(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        self.ensure_protected_keyword(sub_matches, ReportOutputFormat::Text, "deposit")?;
        if Self::has_non_interactive_transaction_args(sub_matches) {
            self.create_transaction_non_interactive(sub_matches, TransactionCategory::Deposit)?;
        } else {
            TransactionDialogBuilder::new(TransactionCategory::Deposit)
                .account(&mut self.trust)
                .currency(&mut self.trust)
                .amount(&mut self.trust)
                .build(&mut self.trust)
                .display();
        }
        Ok(())
    }

    fn withdraw(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        self.ensure_protected_keyword(sub_matches, ReportOutputFormat::Text, "withdrawal")?;
        if Self::has_non_interactive_transaction_args(sub_matches) {
            self.create_transaction_non_interactive(sub_matches, TransactionCategory::Withdrawal)?;
        } else {
            TransactionDialogBuilder::new(TransactionCategory::Withdrawal)
                .account(&mut self.trust)
                .currency(&mut self.trust)
                .amount(&mut self.trust)
                .build(&mut self.trust)
                .display();
        }
        Ok(())
    }
}

// Rules
impl ArgDispatcher {
    fn create_rule(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        self.ensure_protected_keyword(sub_matches, format, "rule creation")?;
        RuleDialogBuilder::new()
            .account(&mut self.trust)
            .name()
            .risk()
            .description()
            .level()
            .build(&mut self.trust)
            .display();
        Ok(())
    }

    fn remove_rule(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        self.ensure_protected_keyword(sub_matches, format, "rule removal")?;
        RuleRemoveDialogBuilder::new()
            .account(&mut self.trust)
            .select_rule(&mut self.trust)
            .build(&mut self.trust)
            .display();
        Ok(())
    }
}

// Market Data
impl ArgDispatcher {
    fn market_snapshot_source_label(source: &MarketSnapshotSource) -> &'static str {
        match source {
            MarketSnapshotSource::QuoteTrade => "quote_trade",
            MarketSnapshotSource::BarsFallback => "bars_fallback",
        }
    }

    fn market_data_channel_text_label(channel: MarketDataChannel) -> &'static str {
        match channel {
            MarketDataChannel::Bars => "bar",
            MarketDataChannel::Quotes => "quote",
            MarketDataChannel::Trades => "trade",
        }
    }

    fn market_data_channel_json_label(channel: MarketDataChannel) -> &'static str {
        match channel {
            MarketDataChannel::Bars => "bars",
            MarketDataChannel::Quotes => "quotes",
            MarketDataChannel::Trades => "trades",
        }
    }

    fn parse_symbol_list(raw: &str) -> Vec<String> {
        raw.split(',')
            .map(|value| value.trim().to_uppercase())
            .filter(|value| !value.is_empty())
            .collect()
    }

    fn parse_market_data_stream_request(
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<MarketDataStreamRequest, CliError> {
        let symbols_raw = sub_matches.get_one::<String>("symbols").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --symbols")
        })?;
        let channels_raw = sub_matches.get_one::<String>("channels").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --channels")
        })?;
        let max_events_raw = sub_matches.get_one::<String>("max-events").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --max-events")
        })?;
        let timeout_raw = sub_matches
            .get_one::<String>("timeout-seconds")
            .ok_or_else(|| {
                Self::report_error(
                    format,
                    "missing_argument",
                    "Missing argument --timeout-seconds",
                )
            })?;

        let max_events = max_events_raw.parse::<usize>().map_err(|_| {
            Self::report_error(
                format,
                "invalid_argument",
                "--max-events must be a positive integer",
            )
        })?;
        let timeout_seconds = timeout_raw.parse::<u64>().map_err(|_| {
            Self::report_error(
                format,
                "invalid_argument",
                "--timeout-seconds must be a positive integer",
            )
        })?;
        let symbols = Self::parse_symbol_list(symbols_raw);
        if symbols.is_empty() {
            return Err(Self::report_error(
                format,
                "invalid_argument",
                "At least one symbol is required in --symbols",
            ));
        }
        let channels = Self::parse_market_data_channels(channels_raw, format)?;

        Ok(MarketDataStreamRequest {
            symbols,
            channels_raw: channels_raw.clone(),
            channels,
            max_events,
            timeout_seconds,
        })
    }

    fn market_snapshot_payload(
        account_id: Uuid,
        requested_at: DateTime<Utc>,
        snapshot: &model::MarketSnapshotV2,
    ) -> Value {
        let lag_seconds = requested_at
            .signed_duration_since(snapshot.as_of)
            .num_seconds()
            .max(0);
        json!({
            "report": "market_data_snapshot",
            "schema_version": 1,
            "generated_at": requested_at.to_rfc3339(),
            "source": {
                "broker": "alpaca",
                "provider": "alpaca_market_data"
            },
            "scope": {
                "account_id": account_id.to_string(),
                "symbol": snapshot.symbol,
            },
            "provenance": {
                "source_kind": Self::market_snapshot_source_label(&snapshot.source),
                "fallback_used": matches!(snapshot.source, MarketSnapshotSource::BarsFallback),
            },
            "freshness": {
                "as_of": snapshot.as_of.to_rfc3339(),
                "lag_seconds": lag_seconds,
            },
            "data": {
                "last_price": Self::decimal_string(snapshot.last_price),
                "open": Self::decimal_string(snapshot.open),
                "high": Self::decimal_string(snapshot.high),
                "low": Self::decimal_string(snapshot.low),
                "volume": snapshot.volume,
                "quote": snapshot.quote.as_ref().map(|quote| {
                    json!({
                        "as_of": quote.as_of.to_rfc3339(),
                        "bid_price": Self::decimal_string(quote.bid_price),
                        "bid_size": quote.bid_size,
                        "ask_price": Self::decimal_string(quote.ask_price),
                        "ask_size": quote.ask_size,
                    })
                }),
                "trade": snapshot.trade.as_ref().map(|trade| {
                    json!({
                        "as_of": trade.as_of.to_rfc3339(),
                        "price": Self::decimal_string(trade.price),
                        "size": trade.size,
                    })
                }),
            }
        })
    }

    fn market_data_stream_payload(
        account_id: Uuid,
        requested_at: DateTime<Utc>,
        request: &MarketDataStreamRequest,
        events: &[model::MarketDataStreamEvent],
    ) -> Value {
        let events_json: Vec<Value> = events
            .iter()
            .map(|event| {
                json!({
                    "channel": Self::market_data_channel_json_label(event.channel),
                    "symbol": event.symbol,
                    "as_of": event.as_of.to_rfc3339(),
                    "price": Self::decimal_string(event.price),
                    "size": event.size,
                })
            })
            .collect();
        json!({
            "report": "market_data_stream",
            "schema_version": 1,
            "generated_at": requested_at.to_rfc3339(),
            "source": {
                "broker": "alpaca",
                "provider": "alpaca_market_data_stream"
            },
            "scope": {
                "account_id": account_id.to_string(),
                "symbols": request.symbols,
                "channels": request.channels_raw,
            },
            "request": {
                "max_events": request.max_events,
                "timeout_seconds": request.timeout_seconds,
            },
            "data": {
                "count": events_json.len(),
                "events": events_json,
            }
        })
    }

    fn market_data_bars_payload(
        account_id: Uuid,
        requested_at: DateTime<Utc>,
        symbol: &str,
        timeframe_raw: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        bars: &[model::MarketBar],
    ) -> Value {
        let bars_json: Vec<Value> = bars
            .iter()
            .map(|bar| {
                json!({
                    "time": bar.time.to_rfc3339(),
                    "open": Self::decimal_string(bar.open),
                    "high": Self::decimal_string(bar.high),
                    "low": Self::decimal_string(bar.low),
                    "close": Self::decimal_string(bar.close),
                    "volume": bar.volume,
                })
            })
            .collect();

        json!({
            "report": "market_data_bars",
            "schema_version": 1,
            "generated_at": requested_at.to_rfc3339(),
            "source": {
                "broker": "alpaca",
                "provider": "alpaca_market_data"
            },
            "scope": {
                "account_id": account_id.to_string(),
                "symbol": symbol,
            },
            "request": {
                "timeframe": timeframe_raw,
                "start": start.to_rfc3339(),
                "end": end.to_rfc3339(),
            },
            "data": {
                "count": bars_json.len(),
                "bars": bars_json,
            }
        })
    }

    fn parse_bar_timeframe(
        raw: &str,
        format: ReportOutputFormat,
    ) -> Result<BarTimeframe, CliError> {
        match raw {
            "1m" => Ok(BarTimeframe::OneMinute),
            "1h" => Ok(BarTimeframe::OneHour),
            "1d" => Ok(BarTimeframe::OneDay),
            _ => Err(Self::report_error(
                format,
                "invalid_timeframe",
                format!("Unsupported timeframe '{raw}'. Allowed: 1m | 1h | 1d"),
            )),
        }
    }

    fn parse_rfc3339_timestamp(
        sub_matches: &ArgMatches,
        key: &'static str,
        format: ReportOutputFormat,
    ) -> Result<DateTime<Utc>, CliError> {
        let raw = sub_matches.get_one::<String>(key).ok_or_else(|| {
            Self::report_error(
                format,
                "missing_argument",
                format!("Missing argument --{key}"),
            )
        })?;

        DateTime::parse_from_rfc3339(raw)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|_| {
                Self::report_error(
                    format,
                    "invalid_timestamp",
                    format!("Invalid RFC3339 timestamp for --{key}: {raw}"),
                )
            })
    }

    fn market_data_snapshot(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --account")
        })?;
        let symbol_raw = sub_matches.get_one::<String>("symbol").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --symbol")
        })?;
        let symbol = symbol_raw.trim().to_uppercase();
        let account = self.resolve_account_arg(account_raw, format)?;
        let requested_at = Utc::now();

        let snapshot = self
            .trust
            .market_snapshot_v2(&account, &symbol, requested_at)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "market_data_snapshot_failed",
                    format!("Failed to retrieve market snapshot: {error}"),
                )
            })?;

        match format {
            ReportOutputFormat::Text => {
                println!("Market snapshot");
                println!("===============");
                println!("symbol: {}", snapshot.symbol);
                println!("as_of: {}", snapshot.as_of.to_rfc3339());
                println!("last_price: {}", crate::zen::amount(snapshot.last_price));
                println!("open: {}", crate::zen::amount(snapshot.open));
                println!("high: {}", crate::zen::amount(snapshot.high));
                println!("low: {}", crate::zen::amount(snapshot.low));
                println!("volume: {}", snapshot.volume);
                if let Some(quote) = &snapshot.quote {
                    println!("bid: {}", crate::zen::amount(quote.bid_price));
                    println!("ask: {}", crate::zen::amount(quote.ask_price));
                }
                if let Some(trade) = &snapshot.trade {
                    println!("last_trade_price: {}", crate::zen::amount(trade.price));
                    println!("last_trade_size: {}", trade.size);
                }
                let source = Self::market_snapshot_source_label(&snapshot.source);
                println!("source: {source}");
            }
            ReportOutputFormat::Json => {
                let payload = Self::market_snapshot_payload(account.id, requested_at, &snapshot);
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn load_snapshot_for_symbol(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<
        (
            model::Account,
            String,
            DateTime<Utc>,
            model::MarketSnapshotV2,
        ),
        CliError,
    > {
        let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --account")
        })?;
        let symbol_raw = sub_matches.get_one::<String>("symbol").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --symbol")
        })?;
        let symbol = symbol_raw.trim().to_uppercase();
        let account = self.resolve_account_arg(account_raw, format)?;
        let requested_at = Utc::now();
        let snapshot = self
            .trust
            .market_snapshot_v2(&account, &symbol, requested_at)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "market_data_snapshot_failed",
                    format!("Failed to retrieve market snapshot: {error}"),
                )
            })?;
        Ok((account, symbol, requested_at, snapshot))
    }

    fn market_data_quote(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let (account, symbol, requested_at, snapshot) =
            self.load_snapshot_for_symbol(sub_matches, format)?;
        let quote = snapshot.quote.ok_or_else(|| {
            Self::report_error(
                format,
                "market_data_quote_unavailable",
                format!("No quote available for symbol '{symbol}'"),
            )
        })?;

        match format {
            ReportOutputFormat::Text => {
                println!("Market quote");
                println!("============");
                println!("symbol: {symbol}");
                println!("as_of: {}", quote.as_of.to_rfc3339());
                println!("bid_price: {}", crate::zen::amount(quote.bid_price));
                println!("bid_size: {}", quote.bid_size);
                println!("ask_price: {}", crate::zen::amount(quote.ask_price));
                println!("ask_size: {}", quote.ask_size);
            }
            ReportOutputFormat::Json => {
                let lag_seconds = requested_at
                    .signed_duration_since(quote.as_of)
                    .num_seconds()
                    .max(0);
                let payload = json!({
                    "report": "market_data_quote",
                    "schema_version": 1,
                    "generated_at": requested_at.to_rfc3339(),
                    "source": {
                        "broker": "alpaca",
                        "provider": "alpaca_market_data"
                    },
                    "scope": {
                        "account_id": account.id.to_string(),
                        "symbol": symbol,
                    },
                    "freshness": {
                        "as_of": quote.as_of.to_rfc3339(),
                        "lag_seconds": lag_seconds,
                    },
                    "data": {
                        "bid_price": Self::decimal_string(quote.bid_price),
                        "bid_size": quote.bid_size,
                        "ask_price": Self::decimal_string(quote.ask_price),
                        "ask_size": quote.ask_size,
                    }
                });
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn market_data_trade(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let (account, symbol, requested_at, snapshot) =
            self.load_snapshot_for_symbol(sub_matches, format)?;
        let trade = snapshot.trade.ok_or_else(|| {
            Self::report_error(
                format,
                "market_data_trade_unavailable",
                format!("No trade tick available for symbol '{symbol}'"),
            )
        })?;

        match format {
            ReportOutputFormat::Text => {
                println!("Market trade");
                println!("============");
                println!("symbol: {symbol}");
                println!("as_of: {}", trade.as_of.to_rfc3339());
                println!("price: {}", crate::zen::amount(trade.price));
                println!("size: {}", trade.size);
            }
            ReportOutputFormat::Json => {
                let lag_seconds = requested_at
                    .signed_duration_since(trade.as_of)
                    .num_seconds()
                    .max(0);
                let payload = json!({
                    "report": "market_data_trade",
                    "schema_version": 1,
                    "generated_at": requested_at.to_rfc3339(),
                    "source": {
                        "broker": "alpaca",
                        "provider": "alpaca_market_data"
                    },
                    "scope": {
                        "account_id": account.id.to_string(),
                        "symbol": symbol,
                    },
                    "freshness": {
                        "as_of": trade.as_of.to_rfc3339(),
                        "lag_seconds": lag_seconds,
                    },
                    "data": {
                        "price": Self::decimal_string(trade.price),
                        "size": trade.size,
                    }
                });
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn market_data_session(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let (account, symbol, requested_at, snapshot) =
            self.load_snapshot_for_symbol(sub_matches, format)?;
        let lag_seconds = requested_at
            .signed_duration_since(snapshot.as_of)
            .num_seconds()
            .max(0);
        let source_kind = Self::market_snapshot_source_label(&snapshot.source);

        match format {
            ReportOutputFormat::Text => {
                println!("Market session");
                println!("==============");
                println!("symbol: {symbol}");
                println!("as_of: {}", snapshot.as_of.to_rfc3339());
                println!("source_kind: {source_kind}");
                println!("staleness_seconds: {lag_seconds}");
                println!("session_state: unknown");
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "market_data_session",
                    "schema_version": 1,
                    "generated_at": requested_at.to_rfc3339(),
                    "source": {
                        "broker": "alpaca",
                        "provider": "alpaca_market_data"
                    },
                    "scope": {
                        "account_id": account.id.to_string(),
                        "symbol": symbol,
                    },
                    "data": {
                        "as_of": snapshot.as_of.to_rfc3339(),
                        "source_kind": source_kind,
                        "staleness_seconds": lag_seconds,
                        "session_state": "unknown",
                    }
                });
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn parse_market_data_channels(
        raw: &str,
        format: ReportOutputFormat,
    ) -> Result<Vec<MarketDataChannel>, CliError> {
        let mut channels: Vec<MarketDataChannel> = Vec::new();
        for token in raw
            .split(',')
            .map(|value| value.trim().to_ascii_lowercase())
        {
            if token.is_empty() {
                continue;
            }
            let channel = match token.as_str() {
                "bars" => MarketDataChannel::Bars,
                "quotes" => MarketDataChannel::Quotes,
                "trades" => MarketDataChannel::Trades,
                _ => {
                    return Err(Self::report_error(
                        format,
                        "invalid_channel",
                        format!("Unsupported channel '{token}'. Allowed: bars,quotes,trades"),
                    ))
                }
            };
            if !channels.contains(&channel) {
                channels.push(channel);
            }
        }
        if channels.is_empty() {
            return Err(Self::report_error(
                format,
                "invalid_channel",
                "At least one channel is required (--channels bars,quotes,trades)",
            ));
        }
        Ok(channels)
    }

    fn market_data_stream(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --account")
        })?;
        let request = Self::parse_market_data_stream_request(sub_matches, format)?;
        let account = self.resolve_account_arg(account_raw, format)?;
        let requested_at = Utc::now();
        let events = self
            .trust
            .stream_market_data(
                &account,
                &request.symbols,
                &request.channels,
                request.max_events,
                request.timeout_seconds,
            )
            .map_err(|error| {
                Self::report_error(
                    format,
                    "market_data_stream_failed",
                    format!("Failed to stream market data: {error}"),
                )
            })?;

        match format {
            ReportOutputFormat::Text => {
                println!("Market data stream");
                println!("==================");
                println!("symbols: {}", request.symbols.join(","));
                println!("events: {}", events.len());
                for event in &events {
                    let channel = Self::market_data_channel_text_label(event.channel);
                    println!(
                        "{} {} {} price={} size={}",
                        event.as_of.to_rfc3339(),
                        channel,
                        event.symbol,
                        crate::zen::amount(event.price),
                        event.size
                    );
                }
            }
            ReportOutputFormat::Json => {
                let payload =
                    Self::market_data_stream_payload(account.id, requested_at, &request, &events);
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn market_data_bars(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --account")
        })?;
        let symbol_raw = sub_matches.get_one::<String>("symbol").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --symbol")
        })?;
        let timeframe_raw = sub_matches.get_one::<String>("timeframe").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --timeframe")
        })?;

        let start = Self::parse_rfc3339_timestamp(sub_matches, "start", format)?;
        let end = Self::parse_rfc3339_timestamp(sub_matches, "end", format)?;
        if end < start {
            return Err(Self::report_error(
                format,
                "invalid_time_range",
                "--end must be greater than or equal to --start",
            ));
        }

        let symbol = symbol_raw.trim().to_uppercase();
        let account = self.resolve_account_arg(account_raw, format)?;
        let timeframe = Self::parse_bar_timeframe(timeframe_raw, format)?;
        let requested_at = Utc::now();

        let bars = self
            .trust
            .market_bars(&account, &symbol, start, end, timeframe)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "market_data_bars_failed",
                    format!("Failed to retrieve market bars: {error}"),
                )
            })?;

        match format {
            ReportOutputFormat::Text => {
                println!("Market bars");
                println!("===========");
                println!("symbol: {symbol}");
                println!("timeframe: {timeframe_raw}");
                println!("start: {}", start.to_rfc3339());
                println!("end: {}", end.to_rfc3339());
                println!("count: {}", bars.len());
                for bar in bars.iter().take(10) {
                    println!(
                        "{} o={} h={} l={} c={} v={}",
                        bar.time.to_rfc3339(),
                        crate::zen::amount(bar.open),
                        crate::zen::amount(bar.high),
                        crate::zen::amount(bar.low),
                        crate::zen::amount(bar.close),
                        bar.volume
                    );
                }
                if bars.len() > 10 {
                    println!("... ({} more bars omitted)", bars.len().saturating_sub(10));
                }
            }
            ReportOutputFormat::Json => {
                let payload = Self::market_data_bars_payload(
                    account.id,
                    requested_at,
                    &symbol,
                    timeframe_raw,
                    start,
                    end,
                    &bars,
                );
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }
}

// Level
impl ArgDispatcher {
    fn level_triggers(
        &mut self,
        _sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let triggers = model::LevelTrigger::known_values();

        match format {
            ReportOutputFormat::Text => {
                println!("Supported level triggers");
                println!("========================");
                for trigger in triggers {
                    println!("- {trigger}");
                }
                println!("- custom values are accepted and normalized to lowercase");
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "level_triggers",
                    "format_version": 1,
                    "generated_at": chrono::Utc::now().to_rfc3339(),
                    "data": {
                        "supported": triggers,
                        "custom_allowed": true
                    }
                });
                Self::print_json(&payload)?;
            }
        }
        Ok(())
    }

    fn level_status(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        use crate::views::LevelView;

        let account_id = self.resolve_level_account_id(sub_matches, format)?;
        let level = self.trust.level_for_account(account_id).map_err(|error| {
            Self::report_error(
                format,
                "level_status_failed",
                format!("Unable to load level status: {error}"),
            )
        })?;

        match format {
            ReportOutputFormat::Text => LevelView::status(&level),
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "level_status",
                    "format_version": 1,
                    "generated_at": chrono::Utc::now().to_rfc3339(),
                    "scope": { "account_id": account_id.to_string() },
                    "data": {
                        "current_level": level.current_level,
                        "description": model::Level::level_description(level.current_level),
                        "risk_multiplier": Self::decimal_string(level.risk_multiplier),
                        "status": level.status.to_string(),
                        "trades_at_level": level.trades_at_level,
                        "level_start_date": level.level_start_date.to_string()
                    }
                });
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn level_history(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        use crate::views::LevelView;

        let account_id = self.resolve_level_account_id(sub_matches, format)?;
        let days = sub_matches.get_one::<u32>("days").copied();
        let changes = self
            .trust
            .level_history_for_account(account_id, days)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "level_history_failed",
                    format!("Unable to load level history: {error}"),
                )
            })?;

        match format {
            ReportOutputFormat::Text => LevelView::history(&changes),
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "level_history",
                    "format_version": 1,
                    "generated_at": chrono::Utc::now().to_rfc3339(),
                    "scope": { "account_id": account_id.to_string() },
                    "filters": { "days": days },
                    "data": changes.iter().map(|change| {
                        json!({
                            "id": change.id.to_string(),
                            "old_level": change.old_level,
                            "new_level": change.new_level,
                            "change_reason": change.change_reason,
                            "trigger_type": change.trigger_type.to_string(),
                            "changed_at": change.changed_at.to_string()
                        })
                    }).collect::<Vec<_>>()
                });
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn level_change(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        use crate::views::LevelView;

        self.ensure_protected_keyword(sub_matches, format, "manual level change")?;
        let account_id = self.resolve_level_account_id(sub_matches, format)?;
        let target_level = *sub_matches.get_one::<u8>("to").ok_or_else(|| {
            Self::report_error(
                format,
                "missing_level",
                "Missing required --to level argument",
            )
        })?;
        let reason = sub_matches
            .get_one::<String>("reason")
            .ok_or_else(|| Self::report_error(format, "missing_reason", "Missing --reason"))?;
        let trigger = sub_matches
            .get_one::<String>("trigger")
            .ok_or_else(|| Self::report_error(format, "missing_trigger", "Missing --trigger"))?;
        let parsed_trigger = LevelTrigger::from_str(trigger).map_err(|error| {
            Self::report_error(
                format,
                "invalid_trigger",
                format!("Invalid --trigger value '{trigger}': {error}"),
            )
        })?;

        let (level, change) = self
            .trust
            .change_level(account_id, target_level, reason, parsed_trigger)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "level_change_failed",
                    format!("Unable to apply level change: {error}"),
                )
            })?;

        match format {
            ReportOutputFormat::Text => {
                println!("Level change applied successfully.");
                LevelView::status(&level);
                println!();
                LevelView::history(&[change]);
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "level_change",
                    "format_version": 1,
                    "generated_at": chrono::Utc::now().to_rfc3339(),
                    "scope": { "account_id": account_id.to_string() },
                    "data": {
                        "level": {
                            "current_level": level.current_level,
                            "description": model::Level::level_description(level.current_level),
                            "risk_multiplier": Self::decimal_string(level.risk_multiplier),
                            "status": level.status.to_string(),
                            "level_start_date": level.level_start_date.to_string(),
                        },
                        "event": {
                            "old_level": change.old_level,
                            "new_level": change.new_level,
                            "change_reason": change.change_reason,
                            "trigger_type": change.trigger_type.to_string(),
                            "changed_at": change.changed_at.to_string(),
                        }
                    }
                });
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn level_evaluate(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_id = self.resolve_level_account_id(sub_matches, format)?;
        let snapshot = Self::level_snapshot_from_args(sub_matches, format)?;
        let apply = sub_matches.get_flag("apply");
        if apply {
            self.ensure_protected_keyword(sub_matches, format, "level policy apply")?;
        }

        let outcome = self
            .trust
            .evaluate_level_transition(account_id, snapshot, apply)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "level_evaluation_failed",
                    format!("Unable to evaluate level transition: {error}"),
                )
            })?;

        match format {
            ReportOutputFormat::Text => Self::print_level_evaluation_text(&outcome, apply),
            ReportOutputFormat::Json => {
                let payload = Self::level_evaluation_payload(account_id, apply, &outcome);
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn level_progress(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_id = self.resolve_level_account_id(sub_matches, format)?;
        let snapshot = Self::level_snapshot_from_args(sub_matches, format)?;
        let outcome = self
            .trust
            .evaluate_level_transition(account_id, snapshot, false)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "level_progress_failed",
                    format!("Unable to calculate level progress: {error}"),
                )
            })?;

        match format {
            ReportOutputFormat::Text => Self::print_level_evaluation_text(&outcome, false),
            ReportOutputFormat::Json => {
                let payload = Self::level_evaluation_payload(account_id, false, &outcome);
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn level_rules_show(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_id = self.resolve_level_account_id(sub_matches, format)?;
        let rules = self
            .trust
            .level_adjustment_rules_for_account(account_id)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "level_rules_show_failed",
                    format!("Unable to load level rules: {error}"),
                )
            })?;

        match format {
            ReportOutputFormat::Text => {
                println!("Level Adjustment Rules");
                println!("======================");
                println!(
                    "monthly_loss_downgrade_pct: {}",
                    rules.monthly_loss_downgrade_pct
                );
                println!(
                    "single_loss_downgrade_pct: {}",
                    rules.single_loss_downgrade_pct
                );
                println!(
                    "upgrade_profitable_trades: {}",
                    rules.upgrade_profitable_trades
                );
                println!("upgrade_win_rate_pct: {}", rules.upgrade_win_rate_pct);
                println!(
                    "upgrade_consecutive_wins: {}",
                    rules.upgrade_consecutive_wins
                );
                println!(
                    "cooldown_profitable_trades: {}",
                    rules.cooldown_profitable_trades
                );
                println!("cooldown_win_rate_pct: {}", rules.cooldown_win_rate_pct);
                println!(
                    "cooldown_consecutive_wins: {}",
                    rules.cooldown_consecutive_wins
                );
                println!(
                    "recovery_profitable_trades: {}",
                    rules.recovery_profitable_trades
                );
                println!("recovery_win_rate_pct: {}", rules.recovery_win_rate_pct);
                println!(
                    "recovery_consecutive_wins: {}",
                    rules.recovery_consecutive_wins
                );
                println!(
                    "min_trades_at_level_for_upgrade: {}",
                    rules.min_trades_at_level_for_upgrade
                );
                println!("max_changes_in_30_days: {}", rules.max_changes_in_30_days);
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "level_rules",
                    "format_version": 1,
                    "generated_at": chrono::Utc::now().to_rfc3339(),
                    "scope": { "account_id": account_id.to_string() },
                    "data": Self::level_rules_payload(&rules),
                });
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn level_rules_set(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        self.ensure_protected_keyword(sub_matches, format, "level rules update")?;
        let account_id = self.resolve_level_account_id(sub_matches, format)?;
        let rule_key = sub_matches
            .get_one::<String>("rule")
            .ok_or_else(|| Self::report_error(format, "missing_rule_key", "Missing --rule"))?;
        let value = sub_matches
            .get_one::<String>("value")
            .ok_or_else(|| Self::report_error(format, "missing_rule_value", "Missing --value"))?;

        let mut rules = self
            .trust
            .level_adjustment_rules_for_account(account_id)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "level_rules_read_failed",
                    format!("Unable to read current level rules: {error}"),
                )
            })?;

        Self::apply_level_rule_update(&mut rules, rule_key, value, format)?;
        let updated = self
            .trust
            .set_level_adjustment_rules(account_id, &rules)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "level_rules_set_failed",
                    format!("Unable to update level rules: {error}"),
                )
            })?;

        match format {
            ReportOutputFormat::Text => {
                println!("Level rule updated: {}={}", rule_key, value);
                println!();
                self.level_rules_show(sub_matches, format)?;
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "level_rules_set",
                    "format_version": 1,
                    "generated_at": chrono::Utc::now().to_rfc3339(),
                    "scope": { "account_id": account_id.to_string() },
                    "data": {
                        "updated_key": rule_key,
                        "updated_value": value,
                        "rules": Self::level_rules_payload(&updated),
                    }
                });
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn apply_level_rule_update(
        rules: &mut LevelAdjustmentRules,
        key: &str,
        value: &str,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let parse_decimal = || {
            Decimal::from_str(value).map_err(|error| {
                Self::report_error(
                    format,
                    "invalid_rule_value",
                    format!("Invalid decimal value '{value}': {error}"),
                )
            })
        };
        let parse_u32 = || {
            value.parse::<u32>().map_err(|error| {
                Self::report_error(
                    format,
                    "invalid_rule_value",
                    format!("Invalid integer value '{value}': {error}"),
                )
            })
        };

        match key {
            "monthly_loss_downgrade_pct" => rules.monthly_loss_downgrade_pct = parse_decimal()?,
            "single_loss_downgrade_pct" => rules.single_loss_downgrade_pct = parse_decimal()?,
            "upgrade_profitable_trades" => rules.upgrade_profitable_trades = parse_u32()?,
            "upgrade_win_rate_pct" => rules.upgrade_win_rate_pct = parse_decimal()?,
            "upgrade_consecutive_wins" => rules.upgrade_consecutive_wins = parse_u32()?,
            "cooldown_profitable_trades" => rules.cooldown_profitable_trades = parse_u32()?,
            "cooldown_win_rate_pct" => rules.cooldown_win_rate_pct = parse_decimal()?,
            "cooldown_consecutive_wins" => rules.cooldown_consecutive_wins = parse_u32()?,
            "recovery_profitable_trades" => rules.recovery_profitable_trades = parse_u32()?,
            "recovery_win_rate_pct" => rules.recovery_win_rate_pct = parse_decimal()?,
            "recovery_consecutive_wins" => rules.recovery_consecutive_wins = parse_u32()?,
            "min_trades_at_level_for_upgrade" => {
                rules.min_trades_at_level_for_upgrade = parse_u32()?
            }
            "max_changes_in_30_days" => rules.max_changes_in_30_days = parse_u32()?,
            _ => {
                return Err(Self::report_error(
                    format,
                    "invalid_rule_key",
                    format!("Unsupported --rule key '{key}'"),
                ));
            }
        }

        Ok(())
    }

    fn level_rules_payload(rules: &LevelAdjustmentRules) -> Value {
        json!({
            "monthly_loss_downgrade_pct": Self::decimal_string(rules.monthly_loss_downgrade_pct),
            "single_loss_downgrade_pct": Self::decimal_string(rules.single_loss_downgrade_pct),
            "upgrade_profitable_trades": rules.upgrade_profitable_trades,
            "upgrade_win_rate_pct": Self::decimal_string(rules.upgrade_win_rate_pct),
            "upgrade_consecutive_wins": rules.upgrade_consecutive_wins,
            "cooldown_profitable_trades": rules.cooldown_profitable_trades,
            "cooldown_win_rate_pct": Self::decimal_string(rules.cooldown_win_rate_pct),
            "cooldown_consecutive_wins": rules.cooldown_consecutive_wins,
            "recovery_profitable_trades": rules.recovery_profitable_trades,
            "recovery_win_rate_pct": Self::decimal_string(rules.recovery_win_rate_pct),
            "recovery_consecutive_wins": rules.recovery_consecutive_wins,
            "min_trades_at_level_for_upgrade": rules.min_trades_at_level_for_upgrade,
            "max_changes_in_30_days": rules.max_changes_in_30_days,
        })
    }
}

// Trading Vehicle
impl ArgDispatcher {
    fn create_trading_vehicle(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        self.ensure_protected_keyword(
            matches,
            ReportOutputFormat::Text,
            "trading-vehicle creation",
        )?;
        if matches.get_flag("from-alpaca") {
            return self.create_trading_vehicle_from_broker(matches, BrokerKind::Alpaca);
        }
        if let Some(broker) = matches.get_one::<String>("from-broker") {
            return self
                .create_trading_vehicle_from_broker(matches, Self::parse_broker_kind(broker)?);
        }

        TradingVehicleDialogBuilder::new()
            .category()
            .symbol()
            .broker()
            .isin()
            .build(&mut self.trust)
            .display();
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn create_trading_vehicle_from_broker(
        &mut self,
        matches: &ArgMatches,
        broker_kind: BrokerKind,
    ) -> Result<(), CliError> {
        let account_name = match matches.get_one::<String>("account") {
            Some(value) if !value.trim().is_empty() => value.trim(),
            _ => {
                return Err(CliError::new(
                    "broker_import_invalid_args",
                    "--account is required when importing from broker metadata",
                ));
            }
        };

        let symbol = match matches.get_one::<String>("symbol") {
            Some(value) if !value.trim().is_empty() => value.trim(),
            _ => {
                return Err(CliError::new(
                    "broker_import_invalid_args",
                    "--symbol is required when importing from broker metadata",
                ));
            }
        };

        let account = match self.trust.search_account(account_name) {
            Ok(account) => account,
            Err(error) => {
                return Err(CliError::new(
                    "broker_import_account_not_found",
                    format!("{error}"),
                ));
            }
        };

        let category_hint = matches
            .get_one::<String>("category")
            .map(|value| Self::parse_trading_vehicle_category(value))
            .transpose()?;

        let imported = trading_vehicle_import::import_from_broker(
            &account,
            symbol,
            broker_kind,
            category_hint,
        )
        .map_err(|error| CliError::new(error.code(), error.message().to_string()))?;

        self.store_imported_trading_vehicle(matches, imported)
    }

    fn store_imported_trading_vehicle(
        &mut self,
        matches: &ArgMatches,
        mut imported: trading_vehicle_import::ImportedTradingVehicle,
    ) -> Result<(), CliError> {
        imported.upsert.fixed_income =
            Self::parse_fixed_income_terms(matches, imported.upsert.category)?;

        let result = self.trust.upsert_trading_vehicle(imported.upsert);

        match result {
            Ok(tv) => {
                println!("{}", imported.summary);
                crate::views::TradingVehicleView::display(tv);
            }
            Err(error) => {
                return Err(CliError::new(
                    "broker_import_store_failed",
                    format!("{error}"),
                ));
            }
        }
        Ok(())
    }

    fn parse_fixed_income_terms(
        matches: &ArgMatches,
        category: model::TradingVehicleCategory,
    ) -> Result<Option<FixedIncomeTerms>, CliError> {
        if !Self::fixed_income_args_present(matches) {
            return Ok(None);
        }

        if category != model::TradingVehicleCategory::Bond {
            return Err(CliError::new(
                "invalid_fixed_income_terms",
                "Bond term flags require --category bond",
            ));
        }

        Ok(Some(Self::merge_fixed_income_terms(matches, None)?))
    }

    fn update_bond_terms(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        self.ensure_protected_keyword(matches, ReportOutputFormat::Text, "bond term update")?;

        if !Self::fixed_income_args_present(matches) {
            return Err(CliError::new(
                "missing_bond_terms",
                "At least one bond term flag is required",
            ));
        }

        let symbol = Self::required_trimmed_arg(matches, "symbol")?;
        let broker = Self::required_trimmed_arg(matches, "broker")?;
        let vehicle = self.find_trading_vehicle_by_symbol_broker(symbol, broker)?;
        if vehicle.category != model::TradingVehicleCategory::Bond {
            return Err(CliError::new(
                "bond_vehicle_wrong_category",
                format!(
                    "Trading vehicle {broker}:{symbol} is category {}, expected bond",
                    vehicle.category
                ),
            ));
        }

        let fixed_income = Self::merge_fixed_income_terms(matches, vehicle.fixed_income.as_ref())?;
        let updated = self
            .trust
            .upsert_trading_vehicle(Self::upsert_from_existing_vehicle(
                &vehicle,
                Some(fixed_income),
            ))
            .map_err(|error| CliError::new("bond_terms_update_failed", format!("{error}")))?;

        crate::views::TradingVehicleView::display(updated);
        Ok(())
    }

    fn merge_fixed_income_terms(
        matches: &ArgMatches,
        existing: Option<&FixedIncomeTerms>,
    ) -> Result<FixedIncomeTerms, CliError> {
        Ok(FixedIncomeTerms {
            face_value: Self::parse_optional_decimal_arg(
                matches,
                "face-value",
                ReportOutputFormat::Text,
            )?
            .or_else(|| existing.and_then(|terms| terms.face_value)),
            annual_coupon_rate_pct: Self::parse_optional_decimal_arg(
                matches,
                "coupon-rate",
                ReportOutputFormat::Text,
            )?
            .or_else(|| existing.and_then(|terms| terms.annual_coupon_rate_pct)),
            maturity_date: Self::parse_maturity_date_arg(matches)?
                .or_else(|| existing.and_then(|terms| terms.maturity_date)),
            coupon_frequency_per_year: Self::parse_coupon_frequency_arg(matches)?
                .or_else(|| existing.and_then(|terms| terms.coupon_frequency_per_year)),
        })
    }

    fn fixed_income_args_present(matches: &ArgMatches) -> bool {
        matches.get_one::<String>("face-value").is_some()
            || matches.get_one::<String>("coupon-rate").is_some()
            || matches.get_one::<String>("maturity-date").is_some()
            || matches.get_one::<u16>("coupon-frequency").is_some()
    }

    fn parse_maturity_date_arg(matches: &ArgMatches) -> Result<Option<NaiveDate>, CliError> {
        matches
            .get_one::<String>("maturity-date")
            .map(|raw| {
                NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| {
                    CliError::new(
                        "invalid_maturity_date",
                        format!("Invalid --maturity-date value (expected YYYY-MM-DD): {raw}"),
                    )
                })
            })
            .transpose()
    }

    fn parse_coupon_frequency_arg(matches: &ArgMatches) -> Result<Option<u16>, CliError> {
        let coupon_frequency_per_year = matches.get_one::<u16>("coupon-frequency").copied();
        if coupon_frequency_per_year == Some(0) {
            return Err(CliError::new(
                "invalid_coupon_frequency",
                "--coupon-frequency must be greater than zero",
            ));
        }
        Ok(coupon_frequency_per_year)
    }

    fn required_trimmed_arg<'a>(
        matches: &'a ArgMatches,
        key: &'static str,
    ) -> Result<&'a str, CliError> {
        matches
            .get_one::<String>(key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CliError::new("missing_argument", format!("Missing argument --{key}")))
    }

    fn find_trading_vehicle_by_symbol_broker(
        &mut self,
        symbol: &str,
        broker: &str,
    ) -> Result<model::TradingVehicle, CliError> {
        let symbol_norm = symbol.to_uppercase();
        let broker_norm = broker.to_lowercase();
        let vehicles = self
            .trust
            .search_trading_vehicles()
            .map_err(|error| CliError::new("trading_vehicle_lookup_failed", format!("{error}")))?;
        vehicles
            .into_iter()
            .find(|vehicle| {
                vehicle.symbol == symbol_norm && vehicle.broker.eq_ignore_ascii_case(&broker_norm)
            })
            .ok_or_else(|| {
                CliError::new(
                    "trading_vehicle_not_found",
                    format!("No trading vehicle found for {broker}:{symbol}"),
                )
            })
    }

    fn upsert_from_existing_vehicle(
        vehicle: &model::TradingVehicle,
        fixed_income: Option<FixedIncomeTerms>,
    ) -> model::database::TradingVehicleUpsert {
        model::database::TradingVehicleUpsert {
            symbol: vehicle.symbol.clone(),
            isin: vehicle.isin.clone(),
            category: vehicle.category,
            broker: vehicle.broker.clone(),
            broker_asset_id: vehicle.broker_asset_id.clone(),
            exchange: vehicle.exchange.clone(),
            broker_asset_class: vehicle.broker_asset_class.clone(),
            broker_asset_status: vehicle.broker_asset_status.clone(),
            tradable: vehicle.tradable,
            marginable: vehicle.marginable,
            shortable: vehicle.shortable,
            easy_to_borrow: vehicle.easy_to_borrow,
            fractionable: vehicle.fractionable,
            fixed_income,
        }
    }

    fn trading_vehicle_stats(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        let format = Self::parse_report_format(matches);
        let vehicles = self
            .trust
            .search_trading_vehicles()
            .map_err(|error| CliError::new("trading_vehicle_stats_failed", format!("{error}")))?;
        let stats = Self::calculate_trading_vehicle_inventory_stats(&vehicles)?;

        match format {
            ReportOutputFormat::Text => {
                Self::print_trading_vehicle_stats(&stats);
                Ok(())
            }
            ReportOutputFormat::Json => {
                Self::print_json(&Self::trading_vehicle_stats_payload(&stats))
            }
        }
    }

    fn calculate_trading_vehicle_inventory_stats(
        vehicles: &[model::TradingVehicle],
    ) -> Result<TradingVehicleInventoryStats, CliError> {
        let mut stats = TradingVehicleInventoryStats {
            total: vehicles.len(),
            by_category: BTreeMap::new(),
            by_broker: BTreeMap::new(),
            bonds: BondInventoryStats {
                total: 0,
                complete_terms: 0,
                missing_terms: 0,
                coupon_rate_count: 0,
                average_coupon_rate_pct: None,
                earliest_maturity: None,
                latest_maturity: None,
            },
        };
        let mut coupon_rate_sum = Decimal::ZERO;
        let mut coupon_rate_divisor = Decimal::ZERO;

        for vehicle in vehicles {
            Self::increment_count(&mut stats.by_category, vehicle.category.to_string())?;
            Self::increment_count(&mut stats.by_broker, vehicle.broker.to_ascii_lowercase())?;
            if vehicle.category == model::TradingVehicleCategory::Bond {
                Self::add_bond_inventory_stats(
                    &mut stats.bonds,
                    vehicle,
                    &mut coupon_rate_sum,
                    &mut coupon_rate_divisor,
                )?;
            }
        }

        if coupon_rate_divisor > Decimal::ZERO {
            stats.bonds.average_coupon_rate_pct = Some(
                coupon_rate_sum
                    .checked_div(coupon_rate_divisor)
                    .ok_or_else(|| {
                        CliError::new("trading_vehicle_stats_failed", "average coupon overflow")
                    })?,
            );
        }

        Ok(stats)
    }

    fn add_bond_inventory_stats(
        stats: &mut BondInventoryStats,
        vehicle: &model::TradingVehicle,
        coupon_rate_sum: &mut Decimal,
        coupon_rate_divisor: &mut Decimal,
    ) -> Result<(), CliError> {
        stats.total = Self::checked_increment_usize(stats.total)?;
        if Self::is_bond_missing_terms(vehicle) {
            stats.missing_terms = Self::checked_increment_usize(stats.missing_terms)?;
        } else {
            stats.complete_terms = Self::checked_increment_usize(stats.complete_terms)?;
        }

        if let Some(terms) = &vehicle.fixed_income {
            if let Some(coupon_rate) = terms.annual_coupon_rate_pct {
                *coupon_rate_sum = coupon_rate_sum.checked_add(coupon_rate).ok_or_else(|| {
                    CliError::new("trading_vehicle_stats_failed", "coupon sum overflow")
                })?;
                *coupon_rate_divisor =
                    coupon_rate_divisor
                        .checked_add(Decimal::ONE)
                        .ok_or_else(|| {
                            CliError::new("trading_vehicle_stats_failed", "coupon count overflow")
                        })?;
                stats.coupon_rate_count = Self::checked_increment_usize(stats.coupon_rate_count)?;
            }
            if let Some(maturity) = terms.maturity_date {
                stats.earliest_maturity = Some(
                    stats
                        .earliest_maturity
                        .map(|current| current.min(maturity))
                        .unwrap_or(maturity),
                );
                stats.latest_maturity = Some(
                    stats
                        .latest_maturity
                        .map(|current| current.max(maturity))
                        .unwrap_or(maturity),
                );
            }
        }

        Ok(())
    }

    fn checked_increment_usize(value: usize) -> Result<usize, CliError> {
        value
            .checked_add(1)
            .ok_or_else(|| CliError::new("trading_vehicle_stats_failed", "count overflow"))
    }

    fn increment_count(map: &mut BTreeMap<String, usize>, key: String) -> Result<(), CliError> {
        let next = map
            .get(&key)
            .copied()
            .unwrap_or_default()
            .checked_add(1)
            .ok_or_else(|| CliError::new("trading_vehicle_stats_failed", "count overflow"))?;
        map.insert(key, next);
        Ok(())
    }

    fn print_trading_vehicle_stats(stats: &TradingVehicleInventoryStats) {
        println!("Trading Vehicle Stats");
        println!("=====================");
        println!("total: {}", stats.total);
        println!("by_category:");
        for (category, count) in &stats.by_category {
            println!("  {category}: {count}");
        }
        println!("by_broker:");
        for (broker, count) in &stats.by_broker {
            println!("  {broker}: {count}");
        }
        println!("bonds:");
        println!("  total: {}", stats.bonds.total);
        println!("  complete_terms: {}", stats.bonds.complete_terms);
        println!("  missing_terms: {}", stats.bonds.missing_terms);
        println!(
            "  average_coupon_rate: {}",
            stats
                .bonds
                .average_coupon_rate_pct
                .map(|value| format!("{}%", Self::decimal_string(value)))
                .unwrap_or_else(|| "n/a".to_string())
        );
        println!(
            "  maturity_range: {}",
            Self::bond_maturity_range_text(&stats.bonds)
        );
    }

    fn bond_maturity_range_text(stats: &BondInventoryStats) -> String {
        match (stats.earliest_maturity, stats.latest_maturity) {
            (Some(earliest), Some(latest)) => format!("{earliest}..{latest}"),
            _ => "n/a".to_string(),
        }
    }

    fn trading_vehicle_stats_payload(stats: &TradingVehicleInventoryStats) -> Value {
        json!({
            "report": "trading_vehicle_stats",
            "total": stats.total,
            "by_category": &stats.by_category,
            "by_broker": &stats.by_broker,
            "bonds": {
                "total": stats.bonds.total,
                "complete_terms": stats.bonds.complete_terms,
                "missing_terms": stats.bonds.missing_terms,
                "coupon_rate_count": stats.bonds.coupon_rate_count,
                "average_coupon_rate_pct": stats.bonds.average_coupon_rate_pct.map(Self::decimal_string),
                "earliest_maturity": stats.bonds.earliest_maturity.map(|value| value.to_string()),
                "latest_maturity": stats.bonds.latest_maturity.map(|value| value.to_string()),
            }
        })
    }

    fn search_trading_vehicle(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        let filters = Self::parse_trading_vehicle_search_filters(matches)?;
        if !filters.has_non_interactive_filters() {
            TradingVehicleSearchDialogBuilder::new()
                .search(&mut self.trust)
                .display();
            return Ok(());
        }

        let vehicles = self
            .trust
            .search_trading_vehicles()
            .map_err(|error| CliError::new("trading_vehicle_search_failed", format!("{error}")))?;
        let filtered = Self::filter_trading_vehicles(vehicles, &filters);

        match filters.format {
            ReportOutputFormat::Text => {
                if filtered.is_empty() {
                    println!("No trading vehicles matched the filters.");
                } else {
                    crate::views::TradingVehicleView::display_table(filtered);
                }
                Ok(())
            }
            ReportOutputFormat::Json => {
                Self::print_json(&Self::trading_vehicle_search_payload(&filtered, &filters))
            }
        }
    }

    fn parse_trading_vehicle_search_filters(
        matches: &ArgMatches,
    ) -> Result<TradingVehicleSearchFilters, CliError> {
        Ok(TradingVehicleSearchFilters {
            category: matches
                .get_one::<String>("category")
                .map(|value| Self::parse_trading_vehicle_category(value))
                .transpose()?,
            broker: Self::optional_trimmed_lowercase_arg(matches, "broker"),
            symbol: Self::optional_trimmed_uppercase_arg(matches, "symbol"),
            isin: Self::optional_trimmed_uppercase_arg(matches, "isin"),
            missing_bond_terms: matches.get_flag("missing-bond-terms"),
            list_all: matches.get_flag("all"),
            format: Self::parse_report_format(matches),
        })
    }

    fn optional_trimmed_lowercase_arg(matches: &ArgMatches, key: &'static str) -> Option<String> {
        matches
            .get_one::<String>(key)
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
    }

    fn optional_trimmed_uppercase_arg(matches: &ArgMatches, key: &'static str) -> Option<String> {
        matches
            .get_one::<String>(key)
            .map(|value| value.trim().to_ascii_uppercase())
            .filter(|value| !value.is_empty())
    }

    fn filter_trading_vehicles(
        vehicles: Vec<model::TradingVehicle>,
        filters: &TradingVehicleSearchFilters,
    ) -> Vec<model::TradingVehicle> {
        let mut filtered: Vec<model::TradingVehicle> = vehicles
            .into_iter()
            .filter(|vehicle| Self::trading_vehicle_matches_filters(vehicle, filters))
            .collect();
        filtered.sort_by(|left, right| {
            left.category
                .to_string()
                .cmp(&right.category.to_string())
                .then_with(|| left.symbol.cmp(&right.symbol))
                .then_with(|| left.broker.cmp(&right.broker))
        });
        filtered
    }

    fn trading_vehicle_matches_filters(
        vehicle: &model::TradingVehicle,
        filters: &TradingVehicleSearchFilters,
    ) -> bool {
        if filters
            .category
            .is_some_and(|category| vehicle.category != category)
        {
            return false;
        }
        if filters
            .broker
            .as_ref()
            .is_some_and(|broker| !vehicle.broker.to_ascii_lowercase().contains(broker))
        {
            return false;
        }
        if filters
            .symbol
            .as_ref()
            .is_some_and(|symbol| !vehicle.symbol.to_ascii_uppercase().contains(symbol))
        {
            return false;
        }
        if filters.isin.as_ref().is_some_and(|isin| {
            vehicle
                .isin
                .as_ref()
                .is_none_or(|value| !value.to_ascii_uppercase().contains(isin))
        }) {
            return false;
        }
        if filters.missing_bond_terms && !Self::is_bond_missing_terms(vehicle) {
            return false;
        }
        true
    }

    fn is_bond_missing_terms(vehicle: &model::TradingVehicle) -> bool {
        if vehicle.category != model::TradingVehicleCategory::Bond {
            return false;
        }
        match &vehicle.fixed_income {
            Some(terms) => {
                terms.face_value.is_none()
                    || terms.annual_coupon_rate_pct.is_none()
                    || terms.maturity_date.is_none()
                    || terms.coupon_frequency_per_year.is_none()
            }
            None => true,
        }
    }

    fn trading_vehicle_search_payload(
        vehicles: &[model::TradingVehicle],
        filters: &TradingVehicleSearchFilters,
    ) -> Value {
        json!({
            "report": "trading_vehicle_search",
            "filters": {
                "category": filters.category.map(|category| category.to_string()),
                "broker": &filters.broker,
                "symbol": &filters.symbol,
                "isin": &filters.isin,
                "missing_bond_terms": filters.missing_bond_terms,
                "all": filters.list_all,
            },
            "count": vehicles.len(),
            "data": vehicles.iter().map(Self::trading_vehicle_payload).collect::<Vec<_>>(),
        })
    }

    fn trading_vehicle_payload(vehicle: &model::TradingVehicle) -> Value {
        json!({
            "id": vehicle.id.to_string(),
            "category": vehicle.category.to_string(),
            "symbol": &vehicle.symbol,
            "broker": &vehicle.broker,
            "isin": &vehicle.isin,
            "broker_asset_id": &vehicle.broker_asset_id,
            "exchange": &vehicle.exchange,
            "broker_asset_class": &vehicle.broker_asset_class,
            "tradable": vehicle.tradable,
            "marginable": vehicle.marginable,
            "shortable": vehicle.shortable,
            "fractionable": vehicle.fractionable,
            "fixed_income": vehicle.fixed_income.as_ref().map(|terms| json!({
                "face_value": terms.face_value.map(Self::decimal_string),
                "annual_coupon_rate_pct": terms.annual_coupon_rate_pct.map(Self::decimal_string),
                "maturity_date": terms.maturity_date.map(|value| value.to_string()),
                "coupon_frequency_per_year": terms.coupon_frequency_per_year,
            })),
        })
    }
}

impl TradingVehicleSearchFilters {
    fn has_non_interactive_filters(&self) -> bool {
        self.list_all
            || self.category.is_some()
            || self.broker.is_some()
            || self.symbol.is_some()
            || self.isin.is_some()
            || self.missing_bond_terms
            || self.format == ReportOutputFormat::Json
    }
}

#[cfg(test)]
fn category_to_str(category: model::TradingVehicleCategory) -> &'static str {
    match category {
        model::TradingVehicleCategory::Crypto => "crypto",
        model::TradingVehicleCategory::Fiat => "fiat",
        model::TradingVehicleCategory::Stock => "stock",
        model::TradingVehicleCategory::Etf => "etf",
        model::TradingVehicleCategory::Bond => "bond",
        _ => "unknown",
    }
}

// Trade
impl ArgDispatcher {
    #[allow(clippy::too_many_lines)]
    fn trade_size_preview(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let preview = self.parse_trade_preview_args(sub_matches, format)?;

        let sizing = self
            .trust
            .calculate_level_adjusted_quantity(
                preview.account_id,
                preview.entry_price,
                preview.stop_price,
                &preview.currency,
            )
            .map_err(|error| {
                Self::report_error(
                    format,
                    "size_preview_failed",
                    format!("Unable to calculate level-adjusted quantity: {error}"),
                )
            })?;

        let current_level = self
            .trust
            .level_for_account(preview.account_id)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "size_preview_level_load_failed",
                    format!("Unable to read account level: {error}"),
                )
            })?;

        let risk_per_share = Self::trade_preview_risk_per_share(&preview);
        let levels = Self::size_preview_levels(
            sizing.base_quantity,
            current_level.current_level,
            risk_per_share,
        );

        match format {
            ReportOutputFormat::Text => {
                for line in Self::trade_size_preview_text_lines(
                    &preview,
                    sizing.base_quantity,
                    sizing.final_quantity,
                    sizing.level_multiplier,
                    &levels,
                ) {
                    println!("{line}");
                }
            }
            ReportOutputFormat::Json => {
                let payload = Self::trade_size_preview_payload(
                    &preview,
                    risk_per_share,
                    sizing.base_quantity,
                    current_level.current_level,
                    sizing.level_multiplier,
                    sizing.final_quantity,
                    levels,
                );
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn trade_hypothesis(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let hypothesis_args = self.parse_trade_hypothesis_args(sub_matches, format)?;

        let hypothesis = self
            .trust
            .calculate_trade_hypothesis(
                hypothesis_args.preview.account_id,
                hypothesis_args.preview.entry_price,
                hypothesis_args.preview.stop_price,
                hypothesis_args.target_price,
                hypothesis_args.quantity,
                &hypothesis_args.preview.currency,
            )
            .map_err(|error| {
                Self::report_error(
                    format,
                    "trade_hypothesis_failed",
                    format!("Unable to calculate trade hypothesis: {error}"),
                )
            })?;
        let report = TradeHypothesisReport {
            available_capital: hypothesis.available_capital,
            capital_required: hypothesis.capital_required,
            capital_required_pct_of_available: hypothesis.capital_required_pct_of_available,
            risk_per_share: hypothesis.risk_per_share,
            reward_per_share: hypothesis.reward_per_share,
            max_loss: hypothesis.max_loss,
            max_loss_pct_of_available: hypothesis.max_loss_pct_of_available,
            max_gain: hypothesis.max_gain,
            max_gain_pct_of_available: hypothesis.max_gain_pct_of_available,
            risk_reward_ratio: hypothesis.risk_reward_ratio,
        };

        match format {
            ReportOutputFormat::Text => {
                for line in Self::trade_hypothesis_text_lines(&hypothesis_args, &report) {
                    println!("{line}");
                }
            }
            ReportOutputFormat::Json => {
                let payload = Self::trade_hypothesis_payload(&hypothesis_args, &report);
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn parse_trade_preview_args(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<TradePreviewArgs, CliError> {
        Ok(TradePreviewArgs {
            account_id: self.resolve_level_account_id(sub_matches, format)?,
            entry_price: Self::parse_decimal_arg(sub_matches, "entry", format)?,
            stop_price: Self::parse_decimal_arg(sub_matches, "stop", format)?,
            currency: Self::parse_currency_arg(sub_matches, format)?,
        })
    }

    fn parse_trade_hypothesis_args(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<TradeHypothesisArgs, CliError> {
        Ok(TradeHypothesisArgs {
            preview: self.parse_trade_preview_args(sub_matches, format)?,
            target_price: Self::parse_decimal_arg(sub_matches, "target", format)?,
            quantity: Self::parse_positive_quantity_arg(sub_matches, format)?,
        })
    }

    fn parse_currency_arg(
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<Currency, CliError> {
        let currency_raw = sub_matches
            .get_one::<String>("currency")
            .map(String::as_str)
            .unwrap_or("usd");
        Currency::from_str(&currency_raw.to_ascii_uppercase()).map_err(|_| {
            Self::report_error(
                format,
                "invalid_currency",
                format!("Unsupported currency '{currency_raw}'. Use USD, EUR, or BTC."),
            )
        })
    }

    fn parse_positive_quantity_arg(
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<i64, CliError> {
        let quantity_raw = sub_matches.get_one::<String>("quantity").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --quantity")
        })?;
        let quantity = quantity_raw.parse::<i64>().map_err(|_| {
            Self::report_error(
                format,
                "invalid_quantity",
                format!("Invalid integer quantity: {quantity_raw}"),
            )
        })?;
        if quantity <= 0 {
            return Err(Self::report_error(
                format,
                "invalid_quantity",
                "Quantity must be greater than 0",
            ));
        }
        Ok(quantity)
    }

    fn trade_preview_risk_per_share(preview: &TradePreviewArgs) -> Decimal {
        preview
            .entry_price
            .checked_sub(preview.stop_price)
            .map(|value| value.abs())
            .unwrap_or(Decimal::ZERO)
    }

    fn size_preview_levels(
        base_quantity: i64,
        current_level: u8,
        risk_per_share: Decimal,
    ) -> Vec<Value> {
        (0_u8..=4_u8)
            .map(|level| {
                let multiplier = Level::multiplier_for_level(level).unwrap_or(Decimal::ZERO);
                let quantity = Decimal::from(base_quantity)
                    .checked_mul(multiplier)
                    .and_then(|value| value.to_i64())
                    .unwrap_or(0)
                    .max(0);
                let risk_amount = risk_per_share
                    .checked_mul(Decimal::from(quantity))
                    .unwrap_or(Decimal::ZERO);

                json!({
                    "level": level,
                    "description": Level::level_description(level),
                    "multiplier": Self::decimal_string(multiplier),
                    "quantity": quantity,
                    "risk_amount": Self::decimal_string(risk_amount),
                    "current": level == current_level,
                })
            })
            .collect()
    }

    fn trade_size_preview_text_lines(
        preview: &TradePreviewArgs,
        base_quantity: i64,
        final_quantity: i64,
        level_multiplier: Decimal,
        levels: &[Value],
    ) -> Vec<String> {
        let mut lines = vec![
            "Position Size Preview".to_string(),
            "====================".to_string(),
            format!("Account: {}", preview.account_id),
            format!(
                "Entry: {} {} | Stop: {} {} | Risk/Share: {} {}",
                crate::zen::price_relative_to(preview.entry_price, preview.entry_price),
                preview.currency,
                crate::zen::price_relative_to(preview.stop_price, preview.entry_price),
                preview.currency,
                crate::zen::amount_share(
                    Self::trade_preview_risk_per_share(preview),
                    preview.entry_price,
                ),
                preview.currency
            ),
            format!(
                "Base quantity (rules only): {} | Current level-adjusted: {} ({}x)",
                base_quantity,
                final_quantity,
                Self::decimal_string(level_multiplier)
            ),
            String::new(),
            "Level ladder:".to_string(),
        ];

        for item in levels {
            let current_marker = if item["current"].as_bool().unwrap_or(false) {
                "  <- current"
            } else {
                ""
            };
            lines.push(format!(
                "L{} ({}x): qty {} | risk {} {}{}",
                item["level"].as_u64().unwrap_or(0),
                item["multiplier"].as_str().unwrap_or("0"),
                item["quantity"].as_i64().unwrap_or(0),
                if crate::zen::is_enabled() {
                    "hidden"
                } else {
                    item["risk_amount"].as_str().unwrap_or("0")
                },
                preview.currency,
                current_marker
            ));
        }

        lines
    }

    fn trade_size_preview_payload(
        preview: &TradePreviewArgs,
        risk_per_share: Decimal,
        base_quantity: i64,
        current_level: u8,
        current_multiplier: Decimal,
        current_quantity: i64,
        levels: Vec<Value>,
    ) -> Value {
        json!({
            "report": "trade_size_preview",
            "format_version": 1,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "scope": {
                "account_id": preview.account_id.to_string(),
            },
            "data": {
                "currency": preview.currency.to_string(),
                "entry_price": Self::decimal_string(preview.entry_price),
                "stop_price": Self::decimal_string(preview.stop_price),
                "risk_per_share": Self::decimal_string(risk_per_share),
                "base_quantity": base_quantity,
                "current_level": current_level,
                "current_multiplier": Self::decimal_string(current_multiplier),
                "current_quantity": current_quantity,
                "levels": levels,
            }
        })
    }

    fn format_optional_decimal(value: Option<Decimal>) -> String {
        value
            .map(Self::decimal_string)
            .unwrap_or_else(|| "n/a".to_string())
    }

    fn format_optional_percentage(value: Option<Decimal>) -> String {
        value
            .map(|decimal| format!("{}%", Self::decimal_string(decimal)))
            .unwrap_or_else(|| "n/a".to_string())
    }

    fn trade_hypothesis_text_lines(
        args: &TradeHypothesisArgs,
        report: &TradeHypothesisReport,
    ) -> Vec<String> {
        vec![
            "Trade Hypothesis".to_string(),
            "================".to_string(),
            format!("Account: {}", args.preview.account_id),
            format!(
                "Entry: {} {} | Stop: {} {} | Target: {} {} | Quantity: {}",
                crate::zen::price_relative_to(args.preview.entry_price, args.preview.entry_price),
                args.preview.currency,
                crate::zen::price_relative_to(args.preview.stop_price, args.preview.entry_price),
                args.preview.currency,
                crate::zen::price_relative_to(args.target_price, args.preview.entry_price),
                args.preview.currency,
                args.quantity
            ),
            format!(
                "Available capital: {} {}",
                crate::zen::amount_share(report.available_capital, report.available_capital),
                args.preview.currency
            ),
            format!(
                "Capital required: {} {} ({})",
                crate::zen::amount_share(report.capital_required, report.available_capital),
                args.preview.currency,
                Self::format_optional_percentage(report.capital_required_pct_of_available)
            ),
            format!(
                "Risk/share: {} {} | Reward/share: {} {} | R:R {}",
                crate::zen::amount_share(report.risk_per_share, args.preview.entry_price),
                args.preview.currency,
                crate::zen::amount_share(report.reward_per_share, args.preview.entry_price),
                args.preview.currency,
                Self::format_optional_decimal(report.risk_reward_ratio)
            ),
            format!(
                "Max loss: {} {} ({})",
                crate::zen::amount_share(report.max_loss, report.available_capital),
                args.preview.currency,
                Self::format_optional_percentage(report.max_loss_pct_of_available)
            ),
            format!(
                "Max gain: {} {} ({})",
                crate::zen::amount_share(report.max_gain, report.available_capital),
                args.preview.currency,
                Self::format_optional_percentage(report.max_gain_pct_of_available)
            ),
        ]
    }

    fn trade_hypothesis_payload(
        args: &TradeHypothesisArgs,
        report: &TradeHypothesisReport,
    ) -> Value {
        json!({
            "report": "trade_hypothesis",
            "format_version": 1,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "scope": {
                "account_id": args.preview.account_id.to_string(),
            },
            "data": {
                "currency": args.preview.currency.to_string(),
                "quantity": args.quantity,
                "available_capital": Self::decimal_string(report.available_capital),
                "entry_price": Self::decimal_string(args.preview.entry_price),
                "stop_price": Self::decimal_string(args.preview.stop_price),
                "target_price": Self::decimal_string(args.target_price),
                "capital_required": Self::decimal_string(report.capital_required),
                "capital_required_pct_of_available": report.capital_required_pct_of_available.map(Self::decimal_string),
                "risk_per_share": Self::decimal_string(report.risk_per_share),
                "reward_per_share": Self::decimal_string(report.reward_per_share),
                "max_loss": Self::decimal_string(report.max_loss),
                "max_loss_pct_of_available": report.max_loss_pct_of_available.map(Self::decimal_string),
                "max_gain": Self::decimal_string(report.max_gain),
                "max_gain_pct_of_available": report.max_gain_pct_of_available.map(Self::decimal_string),
                "risk_reward_ratio": report.risk_reward_ratio.map(Self::decimal_string),
            }
        })
    }

    fn trade_autopsy(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let format = ReportOutputFormat::Text;
        let trade_id = Self::parse_uuid_arg(sub_matches, "trade-id", format)?;
        let trade = self.trust.read_trade(trade_id).map_err(|error| {
            Self::report_error(
                format,
                "trade_not_found",
                format!("Trade {trade_id} not found: {error}"),
            )
        })?;

        if !Self::is_closed_trade_status(trade.status) {
            return Err(Self::report_error(
                format,
                "trade_not_closed",
                format!("Trade {trade_id} is not closed"),
            ));
        }

        let grade = self
            .trust
            .latest_trade_grade(trade.id)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "grade_read_failed",
                    format!("Failed to read latest trade grade: {error}"),
                )
            })?
            .ok_or_else(|| {
                Self::report_error(
                    format,
                    "trade_not_graded",
                    format!("Trade {trade_id} has not been graded"),
                )
            })?;

        let mut io = ConsoleDialogIo::default();
        let mistake = Self::trade_autopsy_with_io(&trade, &grade, &mut io)?;
        let persisted = self.trust.create_mistake(mistake).map_err(|error| {
            Self::report_error(
                format,
                "mistake_create_failed",
                format!("Failed to store trade autopsy: {error}"),
            )
        })?;

        Self::print_trade_autopsy_summary(&trade, &grade, &persisted);
        Ok(())
    }

    fn trade_autopsy_with_io(
        trade: &Trade,
        grade: &TradeGrade,
        io: &mut dyn DialogIo,
    ) -> Result<Mistake, CliError> {
        Self::print_trade_autopsy_context(trade, grade);
        let mut bias_tags = Self::select_autopsy_biases(io)?;
        let lollapalooza = bias_tags.len() >= 2;
        if lollapalooza && !bias_tags.contains(&MungerTendency::Lollapalooza) {
            bias_tags.push(MungerTendency::Lollapalooza);
        }
        let error_type = Self::select_autopsy_error_type(io)?;
        let rule_violated = Self::read_optional_autopsy_text(io, "What rule was violated?")?;
        let counterfactual_r = Self::read_counterfactual_r(io)?;
        let lesson = Self::read_required_autopsy_text(io, "One-sentence lesson")?;
        let now = Utc::now().naive_utc();

        Ok(Mistake {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: trade.id,
            bias_tags,
            lollapalooza,
            error_type,
            rule_violated,
            counterfactual_r,
            lesson,
        })
    }

    fn print_trade_autopsy_context(trade: &Trade, grade: &TradeGrade) {
        let entry_price = trade
            .entry
            .average_filled_price
            .unwrap_or(trade.entry.unit_price);
        println!("Trade autopsy");
        println!("Trade: {} {}", trade.id, trade.trading_vehicle.symbol);
        println!(
            "Entry: {}",
            crate::zen::price_relative_to(entry_price, entry_price)
        );
        println!(
            "Exit: {}",
            Self::trade_exit_price(trade)
                .map(|value| crate::zen::price_relative_to(value, entry_price))
                .unwrap_or_else(|| "n/a".to_string())
        );
        println!(
            "R-multiple: {}",
            Self::trade_r_multiple(trade)
                .map(Self::decimal_string)
                .unwrap_or_else(|| "n/a".to_string())
        );
        println!(
            "Grade: {} ({}/100)",
            grade.overall_grade, grade.overall_score
        );
        println!(
            "Thesis: {}",
            trade
                .thesis
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or("n/a")
        );
        println!();
        println!("Common firing patterns:");
        println!("Loser held too long: #11 + #14");
        println!("Doubled down on a loser: #14 + #5");
        println!("Chased a breakout late: #15 + #18");
        println!("Sized up after win streak: #12 + #13");
        println!("Followed a tip/guru: #22 + #15");
        println!("Two or more biases together: #25 Lollapalooza");
    }

    fn print_trade_autopsy_summary(trade: &Trade, grade: &TradeGrade, mistake: &Mistake) {
        println!();
        println!("Stored trade autopsy: {}", mistake.id);
        println!("Trade: {} {}", trade.id, trade.trading_vehicle.symbol);
        println!(
            "Grade: {} ({}/100)",
            grade.overall_grade, grade.overall_score
        );
        println!(
            "Biases: {}",
            mistake
                .bias_tags
                .iter()
                .map(|tag| Self::munger_tendency_display(*tag))
                .collect::<Vec<String>>()
                .join(", ")
        );
        println!(
            "Lollapalooza: {}",
            if mistake.lollapalooza { "yes" } else { "no" }
        );
        println!("Error type: {}", mistake.error_type);
        println!(
            "Rule violated: {}",
            mistake.rule_violated.as_deref().unwrap_or("n/a")
        );
        println!(
            "Counterfactual R: {}",
            Self::decimal_string(mistake.counterfactual_r)
        );
        println!("Lesson: {}", mistake.lesson);
    }

    fn select_autopsy_biases(io: &mut dyn DialogIo) -> Result<Vec<MungerTendency>, CliError> {
        let options = Self::autopsy_bias_options();
        let labels = options
            .iter()
            .map(|tag| Self::munger_tendency_display(*tag))
            .collect::<Vec<String>>();
        let selected = io
            .multi_select_indices("Which biases were active? (select all that apply)", &labels)
            .map_err(|error| {
                Self::report_error(
                    ReportOutputFormat::Text,
                    "autopsy_cancelled",
                    format!("Bias selection failed: {error}"),
                )
            })?;

        if selected.is_empty() {
            return Err(Self::report_error(
                ReportOutputFormat::Text,
                "missing_bias_tags",
                "Select at least one active bias",
            ));
        }

        selected
            .into_iter()
            .map(|index| {
                options.get(index).copied().ok_or_else(|| {
                    Self::report_error(
                        ReportOutputFormat::Text,
                        "invalid_bias_selection",
                        format!("Invalid bias selection index: {index}"),
                    )
                })
            })
            .collect()
    }

    fn select_autopsy_error_type(io: &mut dyn DialogIo) -> Result<MistakeErrorType, CliError> {
        let labels = vec!["Commission".to_string(), "Omission".to_string()];
        let selected = io
            .select_index("Error of commission or omission?", &labels, 0)
            .map_err(|error| {
                Self::report_error(
                    ReportOutputFormat::Text,
                    "autopsy_cancelled",
                    format!("Error type selection failed: {error}"),
                )
            })?
            .ok_or_else(|| {
                Self::report_error(
                    ReportOutputFormat::Text,
                    "autopsy_cancelled",
                    "Autopsy cancelled before error type selection",
                )
            })?;

        match selected {
            0 => Ok(MistakeErrorType::Commission),
            1 => Ok(MistakeErrorType::Omission),
            _ => Err(Self::report_error(
                ReportOutputFormat::Text,
                "invalid_error_type_selection",
                format!("Invalid error type selection index: {selected}"),
            )),
        }
    }

    fn read_optional_autopsy_text(
        io: &mut dyn DialogIo,
        prompt: &str,
    ) -> Result<Option<String>, CliError> {
        let value = io.input_text(prompt, true).map_err(|error| {
            Self::report_error(
                ReportOutputFormat::Text,
                "autopsy_cancelled",
                format!("Autopsy input failed: {error}"),
            )
        })?;
        let trimmed = value.trim();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed.to_string()))
        }
    }

    fn read_required_autopsy_text(io: &mut dyn DialogIo, prompt: &str) -> Result<String, CliError> {
        let value = io.input_text(prompt, false).map_err(|error| {
            Self::report_error(
                ReportOutputFormat::Text,
                "autopsy_cancelled",
                format!("Autopsy input failed: {error}"),
            )
        })?;
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(Self::report_error(
                ReportOutputFormat::Text,
                "missing_autopsy_input",
                format!("{prompt} cannot be empty"),
            ));
        }
        Ok(trimmed.to_string())
    }

    fn read_counterfactual_r(io: &mut dyn DialogIo) -> Result<Decimal, CliError> {
        let value = Self::read_required_autopsy_text(
            io,
            "What would have happened if you had followed the rule? (counterfactual R-multiple)",
        )?;
        Decimal::from_str(&value).map_err(|_| {
            Self::report_error(
                ReportOutputFormat::Text,
                "invalid_counterfactual_r",
                format!("Invalid counterfactual R-multiple: {value}"),
            )
        })
    }

    fn autopsy_bias_options() -> Vec<MungerTendency> {
        MungerTendency::all()
            .into_iter()
            .filter(|tag| *tag != MungerTendency::Lollapalooza)
            .collect()
    }

    fn munger_tendency_display(tag: MungerTendency) -> String {
        format!("#{} {}", tag.number(), Self::munger_tendency_label(tag))
    }

    fn munger_tendency_label_by_number(number: i32) -> String {
        MungerTendency::all()
            .into_iter()
            .find(|tag| i32::from(tag.number()) == number)
            .map(Self::munger_tendency_label)
            .unwrap_or("Unknown")
            .to_string()
    }

    fn munger_tendency_label(tag: MungerTendency) -> &'static str {
        match tag {
            MungerTendency::RewardPunishment => "Reward/Punishment Superresponse",
            MungerTendency::InconsistencyAvoidance => "Inconsistency-Avoidance",
            MungerTendency::MereAssociation => "Influence-from-Mere-Association",
            MungerTendency::PainAvoidingDenial => "Pain-Avoiding Psychological Denial",
            MungerTendency::ExcessiveSelfRegard => "Excessive Self-Regard",
            MungerTendency::Overoptimism => "Overoptimism",
            MungerTendency::DeprivalSuperreaction => "Deprival Superreaction",
            MungerTendency::SocialProof => "Social-Proof",
            MungerTendency::ContrastMisreaction => "Contrast-Misreaction",
            MungerTendency::StressInfluence => "Stress-Influence",
            MungerTendency::AvailabilityMisweighing => "Availability-Misweighing",
            MungerTendency::AuthorityMisinfluence => "Authority-Misinfluence",
            MungerTendency::Lollapalooza => "Lollapalooza",
        }
    }

    fn is_closed_trade_status(status: Status) -> bool {
        matches!(status, Status::ClosedTarget | Status::ClosedStopLoss)
    }

    fn trade_exit_price(trade: &Trade) -> Option<Decimal> {
        match trade.status {
            Status::ClosedTarget => Some(
                trade
                    .target
                    .average_filled_price
                    .unwrap_or(trade.target.unit_price),
            ),
            Status::ClosedStopLoss => Some(
                trade
                    .safety_stop
                    .average_filled_price
                    .unwrap_or(trade.safety_stop.unit_price),
            ),
            _ => None,
        }
    }

    fn trade_r_multiple(trade: &Trade) -> Option<Decimal> {
        let entry = trade
            .entry
            .average_filled_price
            .unwrap_or(trade.entry.unit_price);
        let stop = trade.safety_stop.unit_price;
        let exit = Self::trade_exit_price(trade)?;
        let risk = match trade.category {
            TradeCategory::Long => entry.checked_sub(stop)?,
            TradeCategory::Short => stop.checked_sub(entry)?,
        };
        if risk <= Decimal::ZERO {
            return None;
        }
        let pnl = match trade.category {
            TradeCategory::Long => exit.checked_sub(entry)?,
            TradeCategory::Short => entry.checked_sub(exit)?,
        };
        pnl.checked_div(risk)
    }

    fn trade_advisor(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let trade_id = Self::parse_uuid_arg(sub_matches, "trade-id", format)?;
        let hold_days = Self::parse_hold_days(sub_matches, format)?;
        let hold_through = sub_matches.get_flag("hold-through");
        let setup = sub_matches
            .get_one::<String>("setup")
            .map(String::as_str)
            .unwrap_or("breakout");
        let today = Utc::now().date_naive();
        let end_date = today
            .checked_add_days(Days::new(hold_days))
            .ok_or_else(|| {
                Self::report_error(format, "invalid_hold_days", "--hold-days is too large")
            })?;

        let trade = self.trust.read_trade(trade_id).map_err(|error| {
            Self::report_error(
                format,
                "trade_not_found",
                format!("Trade {trade_id} not found: {error}"),
            )
        })?;
        let account = self.account_by_id(trade.account_id, format)?;
        let catalyst_scan_warning = self.scan_trade_catalysts(&trade, today, end_date);
        let events = self
            .trust
            .trade_events_for_trade(trade.id)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "trade_events_unavailable",
                    format!("Failed to load trade events: {error}"),
                )
            })?;

        let catalyst = Self::trade_advisor_catalyst_check(
            &events,
            today,
            hold_days,
            hold_through,
            catalyst_scan_warning,
        );
        let correlation = self.trade_advisor_correlation_check(&trade, &account);
        let regime = self.trade_advisor_regime_check(&trade, &account, setup, sub_matches);
        let verdict = Self::trade_advisor_verdict(&[&catalyst, &correlation, &regime]);

        match format {
            ReportOutputFormat::Text => {
                println!("Trade advisor");
                println!("Trade: {} {}", trade.id, trade.trading_vehicle.symbol);
                println!("Verdict: {verdict}");
                println!("Catalyst: {}", catalyst.status);
                println!("Correlation: {}", correlation.status);
                println!("Regime: {}", regime.status);
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "trade_id": trade.id.to_string(),
                    "verdict": verdict,
                    "catalyst": catalyst.payload,
                    "correlation": correlation.payload,
                    "regime": regime.payload,
                });
                Self::print_json(&payload)?;
            }
        }
        Ok(())
    }

    fn trade_events(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        match sub_matches.subcommand() {
            Some(("add", add_matches)) => self.trade_events_add(add_matches),
            Some(("list", list_matches)) => {
                self.trade_events_list(list_matches, Self::parse_report_format(list_matches))
            }
            _ => unreachable!("No trade events subcommand provided"),
        }
    }

    fn trade_events_add(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let format = ReportOutputFormat::Text;
        let trade_id = Self::parse_uuid_arg(sub_matches, "trade-id", format)?;
        let event_type = Self::parse_trade_event_type(sub_matches, format)?;
        let event_date = Self::parse_date_arg(sub_matches, "date", format)?;
        let severity = Self::parse_trade_event_severity(sub_matches, format)?;
        let notes = sub_matches.get_one::<String>("notes").cloned();

        let event = self
            .trust
            .create_trade_event(trade_id, event_type, event_date, severity, notes)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "trade_event_create_failed",
                    format!("Failed to add trade event: {error}"),
                )
            })?;

        println!("Trade event added: {}", event.id);
        println!(
            "{} {} {} {}",
            event.symbol,
            Self::trade_event_type_label(event.event_type),
            event.event_date,
            Self::trade_event_severity_label(event.severity)
        );
        Ok(())
    }

    fn trade_events_list(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let trade_id = Self::parse_uuid_arg(sub_matches, "trade-id", format)?;
        let events = self
            .trust
            .trade_events_for_trade(trade_id)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "trade_events_unavailable",
                    format!("Failed to load trade events: {error}"),
                )
            })?;
        let today = Utc::now().date_naive();

        match format {
            ReportOutputFormat::Text => {
                println!("Trade events: {}", events.len());
                for event in &events {
                    println!(
                        "{} {} {} {} {}",
                        event.id,
                        event.symbol,
                        Self::trade_event_type_label(event.event_type),
                        event.event_date,
                        Self::trade_event_severity_label(event.severity)
                    );
                }
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "trade_id": trade_id.to_string(),
                    "events": events
                        .iter()
                        .map(|event| Self::trade_event_payload(event, today))
                        .collect::<Vec<Value>>(),
                });
                Self::print_json(&payload)?;
            }
        }
        Ok(())
    }

    fn scan_trade_catalysts(
        &mut self,
        trade: &Trade,
        start_date: NaiveDate,
        end_date: NaiveDate,
    ) -> Option<String> {
        let credentials = match CalendarCredentials::read() {
            Ok(credentials) => credentials,
            Err(error) => return Some(format!("calendar credentials unavailable: {error}")),
        };
        let request = CatalystScanRequest::new(
            trade.id,
            trade.trading_vehicle.symbol.clone(),
            start_date,
            end_date,
        );
        match self.trust.scan_trade_catalysts(&request, credentials) {
            Ok(result) => result.warning,
            Err(error) => Some(format!("calendar scan failed: {error}")),
        }
    }

    fn trade_advisor_catalyst_check(
        events: &[TradeEvent],
        today: NaiveDate,
        hold_days: u64,
        hold_through: bool,
        scan_warning: Option<String>,
    ) -> TradeAdvisorCheck {
        let mut blocked = false;
        let mut warning = false;
        for event in events {
            let days_away = event.event_date.signed_duration_since(today).num_days();
            let in_hold_window =
                days_away >= 0 && u64::try_from(days_away).is_ok_and(|days| days <= hold_days);
            if !in_hold_window {
                continue;
            }
            match event.severity {
                TradeEventSeverity::High if days_away <= 3 && !hold_through => blocked = true,
                TradeEventSeverity::High | TradeEventSeverity::Medium => warning = true,
                TradeEventSeverity::Low => {}
            }
        }

        let status = if blocked {
            "BLOCKED"
        } else if warning {
            "WARNING"
        } else if scan_warning.is_some() {
            "SKIPPED"
        } else {
            "OK"
        };

        TradeAdvisorCheck {
            status,
            payload: json!({
                "events": events
                    .iter()
                    .map(|event| Self::trade_event_payload(event, today))
                    .collect::<Vec<Value>>(),
                "status": status,
                "warning": scan_warning,
            }),
        }
    }

    fn trade_advisor_correlation_check(
        &mut self,
        trade: &Trade,
        account: &Account,
    ) -> TradeAdvisorCheck {
        let account_balance = match self.trust.search_balance(account.id, &trade.currency) {
            Ok(balance) => balance.total_balance,
            Err(error) => {
                return Self::skipped_advisor_check(
                    "correlation",
                    format!("account balance unavailable: {error}"),
                );
            }
        };
        let mut open_trades = Self::open_trades_for_scope(&mut self.trust, Some(account.id));
        open_trades.retain(|open_trade| open_trade.id != trade.id);
        let portfolio_symbols = Self::unique_trade_symbols(&open_trades);
        let mut heat = open_trades
            .iter()
            .map(|open_trade| {
                PositionHeat::new(
                    open_trade.trading_vehicle.symbol.clone(),
                    Self::trade_risk_heat_pct(open_trade, account_balance),
                )
            })
            .collect::<Vec<PositionHeat>>();
        heat.push(PositionHeat::new(
            trade.trading_vehicle.symbol.clone(),
            Self::trade_risk_heat_pct(trade, account_balance),
        ));

        let request =
            CorrelationRequest::new(trade.trading_vehicle.symbol.clone(), portfolio_symbols)
                .with_position_heat(heat);
        let advisory =
            match self
                .trust
                .correlation_advisory(&request, account, CorrelationConfig::default())
            {
                Ok(advisory) => advisory,
                Err(error) => {
                    return Self::skipped_advisor_check(
                        "correlation",
                        format!("correlation data unavailable: {error}"),
                    );
                }
            };
        let risk_cap = self
            .trust
            .advisory_thresholds(account.id)
            .map(|thresholds| thresholds.single_position_limit_pct)
            .unwrap_or_else(|_| AdvisoryThresholds::default().single_position_limit_pct);
        let status = if advisory.heat_adjusted_pct > risk_cap {
            "BLOCKED"
        } else if advisory.max_corr > dec!(0.70) {
            "WARNING"
        } else {
            "OK"
        };

        TradeAdvisorCheck {
            status,
            payload: json!({
                "max_corr": Self::decimal_string(advisory.max_corr),
                "corr_with": advisory.corr_with,
                "cluster": advisory.cluster,
                "heat_adjusted_pct": Self::decimal_string(advisory.heat_adjusted_pct),
                "status": status,
            }),
        }
    }

    fn trade_advisor_regime_check(
        &mut self,
        _trade: &Trade,
        account: &Account,
        setup: &str,
        sub_matches: &ArgMatches,
    ) -> TradeAdvisorCheck {
        let breadth_universe =
            Self::parse_optional_symbol_list(sub_matches.get_one::<String>("breadth-universe"));
        let request = RegimeRequest::default().with_breadth_universe(breadth_universe);
        let snapshot = match self
            .trust
            .regime_advisory(&request, account, RegimeConfig::default())
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return Self::skipped_advisor_check(
                    "regime",
                    format!("regime data unavailable: {error}"),
                );
            }
        };
        let permitted = match setup {
            "mean-reversion" => snapshot.mean_reversion_permitted,
            _ => snapshot.breakouts_permitted,
        };
        let status = if permitted { "OK" } else { "BLOCKED" };

        TradeAdvisorCheck {
            status,
            payload: json!({
                "vol": Self::vol_regime_label(snapshot.vol_regime),
                "trend": Self::trend_regime_label(snapshot.trend_regime),
                "breadth": snapshot.breadth_regime.map(Self::breadth_regime_label),
                "composite": Self::composite_regime_label(snapshot.composite),
                "breakouts_permitted": snapshot.breakouts_permitted,
                "status": status,
            }),
        }
    }

    fn skipped_advisor_check(kind: &'static str, warning: String) -> TradeAdvisorCheck {
        let payload = match kind {
            "correlation" => json!({
                "max_corr": null,
                "corr_with": null,
                "cluster": null,
                "heat_adjusted_pct": null,
                "status": "SKIPPED",
                "warning": warning,
            }),
            "regime" => json!({
                "vol": null,
                "trend": null,
                "breadth": null,
                "composite": null,
                "breakouts_permitted": null,
                "status": "SKIPPED",
                "warning": warning,
            }),
            _ => json!({
                "status": "SKIPPED",
                "warning": warning,
            }),
        };
        TradeAdvisorCheck {
            status: "SKIPPED",
            payload,
        }
    }

    fn trade_advisor_verdict(checks: &[&TradeAdvisorCheck]) -> &'static str {
        if checks.iter().any(|check| check.status == "BLOCKED") {
            return "NO_TRADE";
        }
        if checks.iter().any(|check| check.status == "WARNING") {
            return "CAUTION";
        }
        "PROCEED"
    }

    fn trade_event_payload(event: &TradeEvent, today: NaiveDate) -> Value {
        json!({
            "id": event.id.to_string(),
            "trade_id": event.trade_id.to_string(),
            "symbol": event.symbol,
            "type": Self::trade_event_type_label(event.event_type),
            "date": event.event_date.to_string(),
            "severity": Self::trade_event_severity_label(event.severity),
            "source": event.source.to_string(),
            "notes": event.notes,
            "days_away": event.event_date.signed_duration_since(today).num_days(),
        })
    }

    fn parse_hold_days(
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<u64, CliError> {
        let raw = sub_matches
            .get_one::<String>("hold-days")
            .map(String::as_str)
            .unwrap_or("30");
        raw.parse::<u64>().map_err(|_| {
            Self::report_error(
                format,
                "invalid_hold_days",
                format!("Invalid --hold-days value: {raw}"),
            )
        })
    }

    fn parse_date_arg(
        sub_matches: &ArgMatches,
        key: &'static str,
        format: ReportOutputFormat,
    ) -> Result<NaiveDate, CliError> {
        let raw = sub_matches.get_one::<String>(key).ok_or_else(|| {
            Self::report_error(
                format,
                "missing_argument",
                format!("Missing argument --{key}"),
            )
        })?;
        NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| {
            Self::report_error(
                format,
                "invalid_date",
                format!("Invalid --{key} date: {raw}. Use YYYY-MM-DD."),
            )
        })
    }

    fn parse_trade_event_type(
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<TradeEventType, CliError> {
        let raw = sub_matches.get_one::<String>("type").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --type")
        })?;
        TradeEventType::from_str(raw).map_err(|_| {
            Self::report_error(
                format,
                "invalid_trade_event_type",
                format!("Invalid --type value: {raw}"),
            )
        })
    }

    fn parse_trade_event_severity(
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<TradeEventSeverity, CliError> {
        let raw = sub_matches.get_one::<String>("severity").ok_or_else(|| {
            Self::report_error(format, "missing_argument", "Missing argument --severity")
        })?;
        TradeEventSeverity::from_str(raw).map_err(|_| {
            Self::report_error(
                format,
                "invalid_trade_event_severity",
                format!("Invalid --severity value: {raw}"),
            )
        })
    }

    fn unique_trade_symbols(trades: &[Trade]) -> Vec<String> {
        let mut symbols = trades
            .iter()
            .map(|trade| trade.trading_vehicle.symbol.clone())
            .collect::<Vec<String>>();
        symbols.sort();
        symbols.dedup();
        symbols
    }

    fn trade_risk_heat_pct(trade: &Trade, account_balance: Decimal) -> Decimal {
        if account_balance <= Decimal::ZERO {
            return Decimal::ZERO;
        }
        let risk_per_share = trade
            .entry
            .unit_price
            .checked_sub(trade.safety_stop.unit_price)
            .map(|value| value.abs())
            .unwrap_or(Decimal::ZERO);
        risk_per_share
            .checked_mul(Decimal::from(trade.entry.quantity))
            .and_then(|risk| risk.checked_mul(dec!(100)))
            .and_then(|risk_pct| risk_pct.checked_div(account_balance))
            .unwrap_or(Decimal::ZERO)
    }

    fn parse_optional_symbol_list(raw: Option<&String>) -> Vec<String> {
        raw.map(|symbols| {
            symbols
                .split(',')
                .map(str::trim)
                .filter(|symbol| !symbol.is_empty())
                .map(str::to_ascii_uppercase)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default()
    }

    fn trade_event_type_label(event_type: TradeEventType) -> &'static str {
        match event_type {
            TradeEventType::Earnings => "Earnings",
            TradeEventType::Fed => "Fed",
            TradeEventType::Cpi => "CPI",
            TradeEventType::Nfp => "NFP",
            TradeEventType::ExDividend => "ExDividend",
            TradeEventType::Guidance => "Guidance",
            TradeEventType::Other => "Other",
        }
    }

    fn trade_event_severity_label(severity: TradeEventSeverity) -> &'static str {
        match severity {
            TradeEventSeverity::Low => "Low",
            TradeEventSeverity::Medium => "Medium",
            TradeEventSeverity::High => "High",
        }
    }

    fn vol_regime_label(regime: advisor::VolRegime) -> &'static str {
        match regime {
            advisor::VolRegime::Calm => "Calm",
            advisor::VolRegime::Normal => "Normal",
            advisor::VolRegime::Elevated => "Elevated",
        }
    }

    fn trend_regime_label(regime: advisor::TrendRegime) -> &'static str {
        match regime {
            advisor::TrendRegime::Choppy => "Choppy",
            advisor::TrendRegime::Trending => "Trending",
            advisor::TrendRegime::StrongTrend => "StrongTrend",
        }
    }

    fn breadth_regime_label(regime: advisor::BreadthRegime) -> &'static str {
        match regime {
            advisor::BreadthRegime::Broad => "Broad",
            advisor::BreadthRegime::Narrow => "Narrow",
        }
    }

    fn composite_regime_label(regime: advisor::CompositeRegime) -> &'static str {
        match regime {
            advisor::CompositeRegime::CalmTrending => "Calm",
            advisor::CompositeRegime::NormalTrending => "Normal",
            advisor::CompositeRegime::ElevatedTrending => "Elevated",
            advisor::CompositeRegime::Choppy => "Choppy",
            advisor::CompositeRegime::ElevatedChoppy => "ElevatedChoppy",
            advisor::CompositeRegime::NarrowBreadth => "NarrowBreadth",
        }
    }

    #[allow(clippy::too_many_lines)]
    fn create_trade(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        if !Self::has_non_interactive_trade_args(sub_matches) {
            TradeDialogBuilder::new()
                .account(&mut self.trust)
                .trading_vehicle(&mut self.trust)
                .category()
                .entry_price()
                .stop_price()
                .safety_order_category()
                .currency(&mut self.trust)
                .quantity(&mut self.trust)
                .target_price()
                .thesis()
                .sector()
                .asset_class()
                .context()
                .build(&mut self.trust)
                .display();
            return Ok(());
        }

        let format = ReportOutputFormat::Text;
        let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
            Self::report_error(
                format,
                "missing_argument",
                "Missing --account for non-interactive trade creation",
            )
        })?;
        let symbol_raw = sub_matches.get_one::<String>("symbol").ok_or_else(|| {
            Self::report_error(
                format,
                "missing_argument",
                "Missing --symbol for non-interactive trade creation",
            )
        })?;
        let category_raw = sub_matches.get_one::<String>("category").ok_or_else(|| {
            Self::report_error(
                format,
                "missing_argument",
                "Missing --category for non-interactive trade creation",
            )
        })?;
        let quantity_raw = sub_matches.get_one::<String>("quantity").ok_or_else(|| {
            Self::report_error(
                format,
                "missing_argument",
                "Missing --quantity for non-interactive trade creation",
            )
        })?;

        let account = self.resolve_account_arg(account_raw, format)?;
        let symbol = symbol_raw.trim().to_uppercase();
        let trading_vehicle = self
            .trust
            .search_trading_vehicles()
            .map_err(|error| {
                Self::report_error(
                    format,
                    "trading_vehicle_lookup_failed",
                    format!("Failed to load trading vehicles: {error}"),
                )
            })?
            .into_iter()
            .find(|vehicle| vehicle.symbol.eq_ignore_ascii_case(&symbol))
            .ok_or_else(|| {
                Self::report_error(
                    format,
                    "trading_vehicle_not_found",
                    format!(
                        "Trading vehicle '{symbol}' not found. Create it first (e.g. trust trading-vehicle create --from-alpaca --account {} --symbol {symbol} --confirm-protected <KEYWORD>)",
                        account.name
                    ),
                )
            })?;

        let category = TradeCategory::from_str(category_raw).map_err(|_| {
            Self::report_error(
                format,
                "invalid_trade_category",
                format!("Invalid --category value: {category_raw}"),
            )
        })?;

        let entry_price = Self::parse_decimal_arg(sub_matches, "entry", format)?;
        let stop_price = Self::parse_decimal_arg(sub_matches, "stop", format)?;
        let target_price = Self::parse_decimal_arg(sub_matches, "target", format)?;
        let safety_order_category = sub_matches
            .get_one::<String>("safety-order-type")
            .map(|raw| {
                let category = OrderCategory::from_str(raw).map_err(|_| {
                    Self::report_error(
                        format,
                        "invalid_safety_order_type",
                        format!("Invalid --safety-order-type value: {raw}"),
                    )
                })?;
                if matches!(category, OrderCategory::Stop | OrderCategory::StopLimit) {
                    Ok(category)
                } else {
                    Err(Self::report_error(
                        format,
                        "invalid_safety_order_type",
                        format!("Invalid --safety-order-type value: {raw}"),
                    ))
                }
            })
            .transpose()?
            .unwrap_or(OrderCategory::Stop);

        let quantity = quantity_raw.parse::<i64>().map_err(|_| {
            Self::report_error(
                format,
                "invalid_quantity",
                format!("Invalid integer quantity: {quantity_raw}"),
            )
        })?;
        if quantity <= 0 {
            return Err(Self::report_error(
                format,
                "invalid_quantity",
                "Quantity must be greater than 0",
            ));
        }

        let currency_raw = sub_matches
            .get_one::<String>("currency")
            .map(std::string::String::as_str)
            .unwrap_or("usd")
            .to_uppercase();
        let currency = Currency::from_str(currency_raw.as_str()).map_err(|_| {
            Self::report_error(
                format,
                "invalid_currency",
                format!("Invalid --currency value: {}", currency_raw.to_lowercase()),
            )
        })?;

        let draft = DraftTrade {
            account: account.clone(),
            trading_vehicle,
            quantity,
            currency,
            category,
            thesis: sub_matches.get_one::<String>("thesis").cloned(),
            sector: sub_matches.get_one::<String>("sector").cloned(),
            asset_class: sub_matches.get_one::<String>("asset-class").cloned(),
            context: sub_matches.get_one::<String>("context").cloned(),
        };

        let trade = self
            .trust
            .create_trade_with_safety_order_category(
                draft,
                stop_price,
                entry_price,
                target_price,
                safety_order_category,
            )
            .map_err(|error| {
                Self::report_error(
                    format,
                    "trade_create_failed",
                    format!("Failed to create trade: {error}"),
                )
            })?;

        println!("Trade created:");
        crate::views::TradeView::display(&trade, &account.name);
        crate::views::TradeBalanceView::display(&trade.balance);

        let auto_submit = sub_matches.get_flag("auto-submit");
        let auto_fund = auto_submit || sub_matches.get_flag("auto-fund");

        if auto_fund {
            let (funded_trade, tx, account_balance, trade_balance) =
                self.trust.fund_trade(&trade).map_err(|error| {
                    Self::report_error(
                        format,
                        "trade_fund_failed",
                        format!("Failed to fund trade: {error}"),
                    )
                })?;
            println!("Trade funded:");
            crate::views::TradeView::display(&funded_trade, &account.name);
            crate::views::TradeBalanceView::display(&trade_balance);
            crate::views::TransactionView::display_with_basis(
                &tx,
                &account.name,
                account_balance.total_balance,
            );
            crate::views::AccountBalanceView::display(account_balance, &account.name);

            if auto_submit {
                let (submitted_trade, log) =
                    self.trust.submit_trade(&funded_trade).map_err(|error| {
                        Self::report_error(
                            format,
                            "trade_submit_failed",
                            format!("Failed to submit trade: {error}"),
                        )
                    })?;
                println!("Trade submitted:");
                crate::views::TradeView::display(&submitted_trade, &account.name);
                crate::views::TradeBalanceView::display(&submitted_trade.balance);
                crate::views::LogView::display(&log);
            }
        }

        Ok(())
    }

    fn create_cancel(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        if sub_matches.get_one::<String>("trade-id").is_none() {
            CancelDialogBuilder::new()
                .account(&mut self.trust)
                .search(&mut self.trust)
                .build(&mut self.trust)
                .display();
            return Ok(());
        }

        let format = ReportOutputFormat::Text;
        let trade_id = Self::parse_uuid_arg(sub_matches, "trade-id", format)?;
        let trade = self.find_trade_by_id(
            trade_id,
            &[model::Status::Funded, model::Status::Submitted],
            format,
        )?;
        let account = self.account_by_id(trade.account_id, format)?;

        let (trade_balance, account_balance, transaction) = match trade.status {
            model::Status::Funded => self.trust.cancel_funded_trade(&trade),
            model::Status::Submitted => self.trust.cancel_submitted_trade(&trade),
            _ => Err("Trade must be funded or submitted to be canceled".into()),
        }
        .map_err(|error| {
            Self::report_error(
                format,
                "trade_cancel_failed",
                format!("Failed to cancel trade: {error}"),
            )
        })?;

        println!("Trade canceled:");
        crate::views::TradeBalanceView::display(&trade_balance);
        crate::views::TransactionView::display_with_basis(
            &transaction,
            &account.name,
            account_balance.total_balance,
        );
        crate::views::AccountBalanceView::display(account_balance, &account.name);
        Ok(())
    }

    fn create_funding(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        if sub_matches.get_one::<String>("trade-id").is_none() {
            FundingDialogBuilder::new()
                .account(&mut self.trust)
                .search(&mut self.trust)
                .build(&mut self.trust)
                .display();
            return Ok(());
        }

        let format = ReportOutputFormat::Text;
        let trade_id = Self::parse_uuid_arg(sub_matches, "trade-id", format)?;
        let trade = self.find_trade_by_id(trade_id, &[model::Status::New], format)?;
        let account = self.account_by_id(trade.account_id, format)?;

        let (funded_trade, tx, account_balance, trade_balance) =
            self.trust.fund_trade(&trade).map_err(|error| {
                Self::report_error(
                    format,
                    "trade_fund_failed",
                    format!("Failed to fund trade: {error}"),
                )
            })?;

        println!("Trade funded:");
        crate::views::TradeView::display(&funded_trade, &account.name);
        crate::views::TradeBalanceView::display(&trade_balance);
        crate::views::TransactionView::display_with_basis(
            &tx,
            &account.name,
            account_balance.total_balance,
        );
        crate::views::AccountBalanceView::display(account_balance, &account.name);
        Ok(())
    }

    fn create_submit(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        if sub_matches.get_one::<String>("trade-id").is_none() {
            SubmitDialogBuilder::new()
                .account(&mut self.trust)
                .search(&mut self.trust)
                .build(&mut self.trust)
                .display();
            return Ok(());
        }

        let format = ReportOutputFormat::Text;
        let trade_id = Self::parse_uuid_arg(sub_matches, "trade-id", format)?;
        let trade = self.find_trade_by_id(trade_id, &[model::Status::Funded], format)?;
        let account = self.account_by_id(trade.account_id, format)?;
        let (submitted_trade, log) = self.trust.submit_trade(&trade).map_err(|error| {
            Self::report_error(
                format,
                "trade_submit_failed",
                format!("Failed to submit trade: {error}"),
            )
        })?;

        println!("Trade submitted:");
        crate::views::TradeView::display(&submitted_trade, &account.name);
        crate::views::TradeBalanceView::display(&submitted_trade.balance);
        println!("Stop:");
        crate::views::OrderView::display(submitted_trade.safety_stop);
        println!("Entry:");
        crate::views::OrderView::display(submitted_trade.entry);
        println!("Target:");
        crate::views::OrderView::display(submitted_trade.target);
        crate::views::LogView::display(&log);
        Ok(())
    }

    fn create_fill(&mut self) {
        FillTradeDialogBuilder::new()
            .account(&mut self.trust)
            .search(&mut self.trust)
            .fee()
            .build(&mut self.trust)
            .display();
    }

    fn create_stop(&mut self) {
        ExitDialogBuilder::new()
            .account(&mut self.trust)
            .search(&mut self.trust)
            .fee()
            .build_stop(&mut self.trust)
            .display();
    }

    fn create_target(&mut self) {
        ExitDialogBuilder::new()
            .account(&mut self.trust)
            .search(&mut self.trust)
            .fee()
            .build_target(&mut self.trust)
            .display();
    }

    fn create_sync(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        if sub_matches.get_one::<String>("trade-id").is_none() {
            SyncTradeDialogBuilder::new()
                .account(&mut self.trust)
                .search(&mut self.trust)
                .build(&mut self.trust)
                .display();
            return Ok(());
        }

        let format = ReportOutputFormat::Text;
        let trade_id = Self::parse_uuid_arg(sub_matches, "trade-id", format)?;
        let trade = self.find_trade_by_id(
            trade_id,
            &[
                model::Status::Submitted,
                model::Status::PartiallyFilled,
                model::Status::Filled,
            ],
            format,
        )?;
        let account = self.account_by_id(trade.account_id, format)?;
        let (status, orders, log) = self.trust.sync_trade(&trade, &account).map_err(|error| {
            Self::report_error(
                format,
                "trade_sync_failed",
                format!("Failed to sync trade: {error}"),
            )
        })?;

        println!("Trade synced:");
        println!("Trade id: {}", trade.id);
        println!("New status: {}", status);
        if orders.is_empty() {
            println!("No order updates returned from broker.");
        } else {
            println!("Updated orders:");
            for order in orders {
                crate::views::OrderView::display(order);
            }
        }
        crate::views::LogView::display(&log);
        Ok(())
    }

    fn create_watch(&mut self, sub_matches: &ArgMatches) {
        let mut builder = TradeWatchDialogBuilder::new().account(&mut self.trust);
        if sub_matches.get_flag("latest") {
            builder = builder.latest(&mut self.trust);
        } else {
            builder = builder.search(&mut self.trust);
        }
        builder.build(&mut self.trust).display();
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn search_trade(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        if sub_matches.get_one::<String>("account").is_none()
            && sub_matches.get_one::<String>("status").is_none()
            && sub_matches.get_one::<String>("symbol").is_none()
            && sub_matches.get_one::<String>("from").is_none()
            && sub_matches.get_one::<String>("to").is_none()
        {
            TradeSearchDialogBuilder::new()
                .account(&mut self.trust)
                .status()
                .show_balance()
                .search(&mut self.trust)
                .display();
            return Ok(());
        }

        let format = Self::parse_report_format(sub_matches);
        let account_id = match sub_matches.get_one::<String>("account") {
            Some(account_raw) => Some(self.resolve_account_arg(account_raw, format)?.id),
            None => None,
        };
        let status_filter = match sub_matches.get_one::<String>("status") {
            Some(raw) => Some(Status::from_str(raw).map_err(|_| {
                Self::report_error(
                    format,
                    "invalid_status",
                    format!("Invalid --status value '{raw}'"),
                )
            })?),
            None => None,
        };
        let symbol_filter = sub_matches
            .get_one::<String>("symbol")
            .map(|value| value.trim().to_uppercase());
        let from = sub_matches
            .get_one::<String>("from")
            .map(|raw| NaiveDate::parse_from_str(raw, "%Y-%m-%d"))
            .transpose()
            .map_err(|_| Self::report_error(format, "invalid_date", "Invalid --from date"))?;
        let to = sub_matches
            .get_one::<String>("to")
            .map(|raw| NaiveDate::parse_from_str(raw, "%Y-%m-%d"))
            .transpose()
            .map_err(|_| Self::report_error(format, "invalid_date", "Invalid --to date"))?;

        if let (Some(from), Some(to)) = (from, to) {
            if from > to {
                return Err(Self::report_error(
                    format,
                    "invalid_date_range",
                    "--from must be less than or equal to --to",
                ));
            }
        }

        let open_statuses = [
            Status::New,
            Status::Funded,
            Status::Submitted,
            Status::PartiallyFilled,
            Status::Filled,
        ];
        let all_statuses = Status::all();
        let statuses: Vec<Status> = match status_filter {
            Some(status) => vec![status],
            None => all_statuses.clone(),
        };

        let mut trades: Vec<Trade> = Vec::new();
        if let Some(account_id) = account_id {
            for status in statuses {
                if let Ok(mut status_trades) = self.trust.search_trades(account_id, status) {
                    trades.append(&mut status_trades);
                }
            }
        } else if status_filter.is_none() {
            for status in all_statuses {
                for trade in Self::open_trades_for_scope(&mut self.trust, None) {
                    if trade.status == status {
                        trades.push(trade);
                    }
                }
            }
        } else if let Ok(accounts) = self.trust.search_all_accounts() {
            for account in accounts {
                for status in statuses.clone() {
                    if let Ok(mut status_trades) = self.trust.search_trades(account.id, status) {
                        trades.append(&mut status_trades);
                    }
                }
            }
        }

        if status_filter.is_none() {
            trades.retain(|trade| {
                open_statuses.contains(&trade.status)
                    || trade.status.to_string().contains("closed")
                    || matches!(
                        trade.status,
                        Status::Canceled | Status::Expired | Status::Rejected
                    )
            });
        }
        if let Some(symbol) = symbol_filter {
            trades.retain(|trade| trade.trading_vehicle.symbol.eq_ignore_ascii_case(&symbol));
        }
        if from.is_some() || to.is_some() {
            trades.retain(|trade| Self::trade_in_window(trade, from, to));
        }
        trades.sort_by_key(|trade| trade.updated_at);
        trades.reverse();

        match format {
            ReportOutputFormat::Text => {
                println!("Trades found: {}", trades.len());
                for trade in trades.iter().take(50) {
                    println!(
                        "{} {} {} {} qty={} updated_at={}",
                        trade.id,
                        trade.status,
                        trade.trading_vehicle.symbol,
                        trade.category,
                        trade.entry.quantity,
                        trade.updated_at
                    );
                }
                if trades.len() > 50 {
                    println!(
                        "... ({} more trades omitted)",
                        trades.len().saturating_sub(50)
                    );
                }
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "trade_search",
                    "schema_version": 1,
                    "generated_at": Utc::now().to_rfc3339(),
                    "filters": {
                        "account": sub_matches.get_one::<String>("account"),
                        "status": sub_matches.get_one::<String>("status"),
                        "symbol": sub_matches.get_one::<String>("symbol"),
                        "from": sub_matches.get_one::<String>("from"),
                        "to": sub_matches.get_one::<String>("to"),
                    },
                    "data": {
                        "count": trades.len(),
                        "trades": trades.iter().map(|trade| json!({
                            "id": trade.id.to_string(),
                            "account_id": trade.account_id.to_string(),
                            "symbol": trade.trading_vehicle.symbol,
                            "status": trade.status.to_string(),
                            "category": trade.category.to_string(),
                            "quantity": trade.entry.quantity,
                            "entry_price": Self::decimal_string(trade.entry.unit_price),
                            "stop_price": Self::decimal_string(trade.safety_stop.unit_price),
                            "target_price": Self::decimal_string(trade.target.unit_price),
                            "updated_at": trade.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
                        })).collect::<Vec<Value>>(),
                    }
                });
                Self::print_json(&payload)?;
            }
        }
        Ok(())
    }

    fn list_open_trades(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let format = Self::parse_report_format(sub_matches);
        let account_id = match sub_matches.get_one::<String>("account") {
            Some(account_raw) => Some(self.resolve_account_arg(account_raw, format)?.id),
            None => None,
        };
        let mut trades = Self::open_trades_for_scope(&mut self.trust, account_id);
        trades.sort_by_key(|trade| trade.updated_at);
        trades.reverse();

        match format {
            ReportOutputFormat::Text => {
                println!("Open trades: {}", trades.len());
                for trade in &trades {
                    println!(
                        "{} {} {} qty={} updated_at={}",
                        trade.id,
                        trade.status,
                        trade.trading_vehicle.symbol,
                        trade.entry.quantity,
                        trade.updated_at
                    );
                }
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "trade_list_open",
                    "schema_version": 1,
                    "generated_at": Utc::now().to_rfc3339(),
                    "scope": { "account_id": account_id.map(|id| id.to_string()) },
                    "data": {
                        "count": trades.len(),
                        "trades": trades.iter().map(|trade| json!({
                            "id": trade.id.to_string(),
                            "account_id": trade.account_id.to_string(),
                            "symbol": trade.trading_vehicle.symbol,
                            "status": trade.status.to_string(),
                            "quantity": trade.entry.quantity,
                            "updated_at": trade.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
                        })).collect::<Vec<Value>>()
                    }
                });
                Self::print_json(&payload)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn reconcile_trades(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let format = Self::parse_report_format(sub_matches);
        let mut trades: Vec<Trade> =
            if let Some(trade_id_raw) = sub_matches.get_one::<String>("trade-id") {
                let trade_id = Uuid::parse_str(trade_id_raw).map_err(|_| {
                    Self::report_error(format, "invalid_trade_id", "Invalid --trade-id UUID")
                })?;
                vec![self.find_trade_by_id(
                    trade_id,
                    &[Status::Submitted, Status::PartiallyFilled, Status::Filled],
                    format,
                )?]
            } else {
                let account_id = match sub_matches.get_one::<String>("account") {
                    Some(account_raw) => Some(self.resolve_account_arg(account_raw, format)?.id),
                    None => None,
                };
                Self::open_trades_for_scope(&mut self.trust, account_id)
                    .into_iter()
                    .filter(|trade| {
                        matches!(
                            trade.status,
                            Status::Submitted | Status::PartiallyFilled | Status::Filled
                        )
                    })
                    .collect()
            };

        trades.sort_by_key(|trade| trade.updated_at);
        trades.reverse();

        let mut successes: Vec<Value> = Vec::new();
        let mut failures: Vec<Value> = Vec::new();
        for trade in trades {
            match self.account_by_id(trade.account_id, format) {
                Ok(account) => match self.trust.sync_trade(&trade, &account) {
                    Ok((new_status, orders, _log)) => {
                        successes.push(json!({
                            "trade_id": trade.id.to_string(),
                            "status_before": trade.status.to_string(),
                            "status_after": new_status.to_string(),
                            "updated_orders": orders.len(),
                        }));
                    }
                    Err(error) => failures.push(json!({
                        "trade_id": trade.id.to_string(),
                        "error": error.to_string(),
                    })),
                },
                Err(error) => failures.push(json!({
                    "trade_id": trade.id.to_string(),
                    "error": error.to_string(),
                })),
            }
        }

        match format {
            ReportOutputFormat::Text => {
                println!(
                    "Reconcile completed: success={} failed={}",
                    successes.len(),
                    failures.len()
                );
                for entry in &successes {
                    println!(
                        "OK {} {} -> {} orders={}",
                        entry["trade_id"].as_str().unwrap_or(""),
                        entry["status_before"].as_str().unwrap_or(""),
                        entry["status_after"].as_str().unwrap_or(""),
                        entry["updated_orders"].as_u64().unwrap_or(0)
                    );
                }
                for entry in &failures {
                    println!(
                        "ERR {} {}",
                        entry["trade_id"].as_str().unwrap_or(""),
                        entry["error"].as_str().unwrap_or("")
                    );
                }
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "trade_reconcile",
                    "schema_version": 1,
                    "generated_at": Utc::now().to_rfc3339(),
                    "data": {
                        "success_count": successes.len(),
                        "failure_count": failures.len(),
                        "successes": successes,
                        "failures": failures,
                    }
                });
                Self::print_json(&payload)?;
            }
        }
        Ok(())
    }

    fn close(&mut self, matches: &ArgMatches) {
        // Check if auto-distribute flag is set
        let auto_distribute = matches.get_flag("auto-distribute");

        if auto_distribute {
            println!("🚀 Enhanced trade closure with automatic profit distribution enabled!");
            println!("   If the trade is profitable, profits will be automatically distributed.");
        }

        CloseDialogBuilder::new()
            .account(&mut self.trust)
            .search(&mut self.trust)
            .auto_distribute(auto_distribute)
            .build(&mut self.trust)
            .display();
    }

    fn modify_stop(&mut self) {
        ModifyDialogBuilder::new()
            .account(&mut self.trust)
            .search(&mut self.trust)
            .new_price()
            .build_stop(&mut self.trust)
            .display();
    }

    fn modify_target(&mut self) {
        ModifyDialogBuilder::new()
            .account(&mut self.trust)
            .search(&mut self.trust)
            .new_price()
            .build_target(&mut self.trust)
            .display();
    }
}

impl ArgDispatcher {
    fn open_trades_for_scope(trust: &mut TrustFacade, account_id: Option<Uuid>) -> Vec<Trade> {
        use std::collections::HashSet;

        let mut out: Vec<Trade> = Vec::new();
        let mut seen: HashSet<Uuid> = HashSet::new();

        let mut push_unique = |trades: Vec<Trade>| {
            for t in trades {
                if seen.insert(t.id) {
                    out.push(t);
                }
            }
        };

        let statuses = [model::Status::Filled, model::Status::PartiallyFilled];

        if let Some(id) = account_id {
            for status in statuses {
                if let Ok(trades) = trust.search_trades(id, status) {
                    push_unique(trades);
                }
            }
            return out;
        }

        if let Ok(accounts) = trust.search_all_accounts() {
            for account in accounts {
                for status in statuses {
                    if let Ok(trades) = trust.search_trades(account.id, status) {
                        push_unique(trades);
                    }
                }
            }
        }

        out
    }

    fn create_keys(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        self.ensure_protected_keyword(
            sub_matches,
            ReportOutputFormat::Text,
            "broker key creation",
        )?;
        KeysWriteDialogBuilder::new()
            .account(&mut self.trust)
            .environment()
            .url()
            .key_id()
            .key_secret()
            .build()
            .display();
        Ok(())
    }

    fn show_keys(&mut self) {
        KeysReadDialogBuilder::new()
            .account(&mut self.trust)
            .environment()
            .build()
            .display();
    }

    fn delete_keys(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        self.ensure_protected_keyword(
            sub_matches,
            ReportOutputFormat::Text,
            "broker key deletion",
        )?;
        KeysDeleteDialogBuilder::new()
            .account(&mut self.trust)
            .environment()
            .build()
            .display();
        Ok(())
    }

    fn set_protected_keyword(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
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

    fn show_protected_keyword(&mut self) {
        match protected_keyword::read_expected() {
            Ok(_) => println!("Protected mutation keyword is configured."),
            Err(_) => println!("Protected mutation keyword is not configured."),
        }
    }

    fn delete_protected_keyword(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        self.ensure_protected_keyword(
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

    fn onboarding_init(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        if protected_keyword::read_expected().is_ok() {
            return Err(Self::report_error(
                format,
                "onboarding_already_initialized",
                "Protected keyword already configured. Use `trust keys protected-set --value <NEW> --confirm-protected <CURRENT>` to rotate.",
            ));
        }
        let value = sub_matches
            .get_one::<String>("protected-keyword")
            .ok_or_else(|| {
                Self::report_error(format, "missing_keyword", "Missing --protected-keyword")
            })?;
        protected_keyword::store(value).map_err(|error| {
            Self::report_error(
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
                Self::print_json(&payload)?;
            }
        }
        Ok(())
    }

    fn onboarding_status(&mut self, format: ReportOutputFormat) -> Result<(), CliError> {
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
                Self::print_json(&payload)?;
            }
        }
        Ok(())
    }

    fn policy_show(&mut self, format: ReportOutputFormat) -> Result<(), CliError> {
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
                Self::print_json(&payload)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn drawdown_report(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        use crate::views::DrawdownView;
        use core::calculators_drawdown::RealizedDrawdownCalculator;
        use std::str::FromStr;

        // Get account ID if provided
        let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
            match Uuid::from_str(account_arg) {
                Ok(id) => Some(id),
                Err(_) => {
                    return Err(Self::report_error(
                        format,
                        "invalid_account_id",
                        "Invalid account ID format",
                    ));
                }
            }
        } else {
            None
        };

        // Fetch transactions for the account
        let transactions = if let Some(account_id) = account_id {
            match self.trust.get_account_transactions(account_id) {
                Ok(txns) => txns,
                Err(_) => {
                    return Err(Self::report_error(
                        format,
                        "transaction_data_unavailable",
                        "Unable to retrieve transaction data. Please check your account settings and try again.",
                    ));
                }
            }
        } else {
            // If no account specified, get all transactions for all accounts
            match self.trust.get_all_transactions() {
                Ok(txns) => txns,
                Err(_) => {
                    return Err(Self::report_error(
                        format,
                        "transaction_data_unavailable",
                        "Unable to retrieve transaction data. Please check your settings and try again.",
                    ));
                }
            }
        };

        // Calculate equity curve
        let curve = match RealizedDrawdownCalculator::calculate_equity_curve(&transactions) {
            Ok(c) => c,
            Err(e) => {
                return Err(Self::report_error(
                    format,
                    "drawdown_equity_curve_failed",
                    format!("Error calculating equity curve: {e}"),
                ));
            }
        };

        // Calculate drawdown metrics
        let metrics = match RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve) {
            Ok(m) => m,
            Err(e) => {
                return Err(Self::report_error(
                    format,
                    "drawdown_calculation_failed",
                    format!("Error calculating drawdown metrics: {e}"),
                ));
            }
        };

        match format {
            ReportOutputFormat::Text => DrawdownView::display(metrics),
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "drawdown",
                    "format_version": 1,
                    "generated_at": chrono::Utc::now().to_rfc3339(),
                    "scope": { "account_id": account_id.map(|id| id.to_string()) },
                    "data": {
                        "current_equity": Self::decimal_string(metrics.current_equity),
                        "peak_equity": Self::decimal_string(metrics.peak_equity),
                        "current_drawdown": Self::decimal_string(metrics.current_drawdown),
                        "current_drawdown_percentage": Self::decimal_string(metrics.current_drawdown_percentage),
                        "max_drawdown": Self::decimal_string(metrics.max_drawdown),
                        "max_drawdown_percentage": Self::decimal_string(metrics.max_drawdown_percentage),
                        "max_drawdown_date": metrics.max_drawdown_date.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                        "recovery_from_max": Self::decimal_string(metrics.recovery_from_max),
                        "recovery_percentage": Self::decimal_string(metrics.recovery_percentage),
                        "days_since_peak": metrics.days_since_peak,
                        "days_in_drawdown": metrics.days_in_drawdown
                    },
                    "consistency": {
                        "drawdown_non_negative": metrics.current_drawdown >= dec!(0) && metrics.max_drawdown >= dec!(0),
                        "max_ge_current": metrics.max_drawdown >= metrics.current_drawdown
                    }
                });
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn performance_report(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        use crate::views::PerformanceView;
        use core::calculators_performance::PerformanceCalculator;
        use std::str::FromStr;

        // Get account ID if provided
        let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
            match Uuid::from_str(account_arg) {
                Ok(id) => Some(id),
                Err(_) => {
                    return Err(Self::report_error(
                        format,
                        "invalid_account_id",
                        "Invalid account ID format",
                    ));
                }
            }
        } else {
            None
        };

        // Get days filter if provided
        let days_filter = sub_matches.get_one::<u32>("days").copied();

        // Use the new helper method to get all closed trades
        let mut all_trades = match self.trust.search_closed_trades(account_id) {
            Ok(trades) => trades,
            Err(_) => {
                return Err(Self::report_error(
                    format,
                    "trading_data_unavailable",
                    "Unable to retrieve trading data. Please check your account settings and try again.",
                ));
            }
        };

        // Apply days filter if specified
        if let Some(days) = days_filter {
            all_trades = PerformanceCalculator::filter_trades_by_days(&all_trades, days);
        }

        if format == ReportOutputFormat::Text {
            PerformanceView::display(all_trades);
            return Ok(());
        }

        let stats = PerformanceCalculator::calculate_performance_stats(&all_trades);
        let calculated_win_rate = if stats.total_trades == 0 {
            dec!(0)
        } else {
            Decimal::from(stats.winning_trades)
                .checked_mul(dec!(100))
                .and_then(|v| v.checked_div(Decimal::from(stats.total_trades)))
                .unwrap_or(dec!(0))
        };

        let payload = json!({
            "report": "performance",
            "format_version": 1,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "scope": { "account_id": account_id.map(|id| id.to_string()) },
            "filters": { "days": days_filter },
            "data": {
                "total_trades": stats.total_trades,
                "winning_trades": stats.winning_trades,
                "losing_trades": stats.losing_trades,
                "win_rate_percentage": Self::decimal_string(stats.win_rate),
                "average_win": Self::decimal_string(stats.average_win),
                "average_loss": Self::decimal_string(stats.average_loss),
                "average_r_multiple": Self::decimal_string(stats.average_r_multiple),
                "best_trade": stats.best_trade.map(Self::decimal_string),
                "worst_trade": stats.worst_trade.map(Self::decimal_string)
            },
            "consistency": {
                "trade_count_balanced": stats.total_trades == stats.winning_trades.saturating_add(stats.losing_trades),
                "win_rate_recomputed_percentage": Self::decimal_string(calculated_win_rate),
                "win_rate_delta_percentage": Self::decimal_string(
                    stats
                        .win_rate
                        .checked_sub(calculated_win_rate)
                        .unwrap_or(dec!(0))
                        .abs()
                )
            }
        });
        Self::print_json(&payload)?;

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn risk_report(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        use crate::views::RiskView;
        use core::calculators_risk::CapitalAtRiskCalculator;
        use model::Currency;
        use std::str::FromStr;

        // Get account ID if provided
        let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
            match Uuid::from_str(account_arg) {
                Ok(id) => Some(id),
                Err(_) => {
                    return Err(Self::report_error(
                        format,
                        "invalid_account_id",
                        "Invalid account ID format",
                    ));
                }
            }
        } else {
            None
        };

        // Calculate open positions
        let positions = match self.trust.calculate_open_positions(account_id) {
            Ok(p) => p,
            Err(e) => {
                return Err(Self::report_error(
                    format,
                    "open_positions_failed",
                    format!("Error calculating open positions: {e}"),
                ));
            }
        };

        // Calculate total capital at risk
        let total_capital_at_risk =
            match CapitalAtRiskCalculator::calculate_total_capital_at_risk(&positions) {
                Ok(total) => total,
                Err(e) => {
                    return Err(Self::report_error(
                        format,
                        "capital_at_risk_failed",
                        format!("Error calculating total capital at risk: {e}"),
                    ));
                }
            };

        // Get account balance for equity calculation.
        let account_balance = if let Some(id) = account_id {
            match self.trust.search_balance(id, &Currency::USD) {
                Ok(balance) => balance.total_balance,
                Err(_) => {
                    return Err(Self::report_error(
                        format,
                        "account_balance_unavailable",
                        "Unable to retrieve account balance",
                    ));
                }
            }
        } else {
            match self.trust.search_all_accounts() {
                Ok(accounts) => accounts
                    .iter()
                    .map(|account| self.trust.search_balance(account.id, &Currency::USD))
                    .filter_map(Result::ok)
                    .fold(dec!(0), |acc, balance| {
                        acc.checked_add(balance.total_balance).unwrap_or(acc)
                    }),
                Err(_) => dec!(0),
            }
        };

        if format == ReportOutputFormat::Text {
            RiskView::display(positions, total_capital_at_risk, account_balance);
            return Ok(());
        }

        let position_sum = positions.iter().fold(dec!(0), |acc, position| {
            acc.checked_add(position.capital_amount).unwrap_or(acc)
        });
        let risk_percent = if account_balance > dec!(0) {
            total_capital_at_risk
                .checked_mul(dec!(100))
                .and_then(|v| v.checked_div(account_balance))
                .unwrap_or(dec!(0))
        } else {
            dec!(0)
        };
        let positions_json: Vec<Value> = positions
            .iter()
            .map(|position| {
                json!({
                    "trade_id": position.trade_id.to_string(),
                    "symbol": position.symbol,
                    "capital_amount": Self::decimal_string(position.capital_amount),
                    "status": format!("{:?}", position.status),
                    "funded_date": position.funded_date.format("%Y-%m-%dT%H:%M:%S").to_string()
                })
            })
            .collect();
        let payload = json!({
            "report": "risk",
            "format_version": 1,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "scope": { "account_id": account_id.map(|id| id.to_string()) },
            "data": {
                "account_equity": Self::decimal_string(account_balance),
                "total_capital_at_risk": Self::decimal_string(total_capital_at_risk),
                "capital_at_risk_percentage": Self::decimal_string(risk_percent),
                "open_positions_count": positions.len(),
                "open_positions": positions_json
            },
            "consistency": {
                "position_sum": Self::decimal_string(position_sum),
                "position_sum_matches_total": position_sum == total_capital_at_risk,
                "delta_total_minus_sum": Self::decimal_string(
                    total_capital_at_risk
                        .checked_sub(position_sum)
                        .unwrap_or(dec!(0))
                        .abs()
                )
            }
        });
        Self::print_json(&payload)?;

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn concentration_report(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        use crate::views::ConcentrationView;
        use core::calculators_concentration::{ConcentrationCalculator, MetadataField};
        use std::str::FromStr;

        // Get account ID if provided
        let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
            match Uuid::from_str(account_arg) {
                Ok(id) => Some(id),
                Err(_) => {
                    return Err(Self::report_error(
                        format,
                        "invalid_account_id",
                        "Invalid account ID format",
                    ));
                }
            }
        } else {
            None
        };

        // Check if open-only flag is set
        let open_only = sub_matches.get_flag("open-only");

        // Get all trades (or filter by account)
        let all_trades = if let Some(id) = account_id {
            // Get trades for specific account - need to get all statuses
            let mut trades = Vec::new();
            for status in model::Status::all() {
                if let Ok(mut status_trades) = self.trust.search_trades(id, status) {
                    trades.append(&mut status_trades);
                }
            }
            trades
        } else {
            // Get trades for all accounts
            match self.trust.search_all_accounts() {
                Ok(accounts) => {
                    let mut all_trades = Vec::new();
                    for account in accounts {
                        for status in model::Status::all() {
                            if let Ok(mut trades) = self.trust.search_trades(account.id, status) {
                                all_trades.append(&mut trades);
                            }
                        }
                    }
                    all_trades
                }
                Err(e) => {
                    return Err(Self::report_error(
                        format,
                        "accounts_fetch_failed",
                        format!("Error fetching accounts: {e}"),
                    ));
                }
            }
        };

        if all_trades.is_empty() {
            match format {
                ReportOutputFormat::Text => {
                    println!("\nNo trades found for the specified criteria.\n")
                }
                ReportOutputFormat::Json => {
                    let payload = json!({
                        "report": "concentration",
                        "format_version": 1,
                        "generated_at": chrono::Utc::now().to_rfc3339(),
                        "scope": { "account_id": account_id.map(|id| id.to_string()) },
                        "filters": { "open_only": open_only },
                        "data": {
                            "sector": { "total_risk": "0", "groups": [], "warnings": [] },
                            "asset_class": { "total_risk": "0", "groups": [], "warnings": [] }
                        },
                        "consistency": {
                            "sector_group_sum": "0",
                            "sector_group_sum_matches_total": true,
                            "asset_group_sum": "0",
                            "asset_group_sum_matches_total": true
                        }
                    });
                    Self::print_json(&payload)?;
                }
            }
            return Ok(());
        }

        // Filter for open positions if requested
        let trades_to_analyze = if open_only {
            ConcentrationCalculator::filter_open_trades(&all_trades)
        } else {
            all_trades
        };

        if trades_to_analyze.is_empty() {
            match format {
                ReportOutputFormat::Text => {
                    println!("\nNo open trades found for the specified criteria.\n");
                }
                ReportOutputFormat::Json => {
                    let payload = json!({
                        "report": "concentration",
                        "format_version": 1,
                        "generated_at": chrono::Utc::now().to_rfc3339(),
                        "scope": { "account_id": account_id.map(|id| id.to_string()) },
                        "filters": { "open_only": open_only },
                        "data": {
                            "sector": { "total_risk": "0", "groups": [], "warnings": [] },
                            "asset_class": { "total_risk": "0", "groups": [], "warnings": [] }
                        },
                        "consistency": {
                            "sector_group_sum": "0",
                            "sector_group_sum_matches_total": true,
                            "asset_group_sum": "0",
                            "asset_group_sum_matches_total": true
                        }
                    });
                    Self::print_json(&payload)?;
                }
            }
            return Ok(());
        }

        // Analyze concentration by sector
        let sector_analysis =
            ConcentrationCalculator::analyze_by_metadata(&trades_to_analyze, MetadataField::Sector);

        // Analyze concentration by asset class
        let asset_class_analysis = ConcentrationCalculator::analyze_by_metadata(
            &trades_to_analyze,
            MetadataField::AssetClass,
        );

        if format == ReportOutputFormat::Text {
            ConcentrationView::display(sector_analysis, asset_class_analysis, open_only);
            return Ok(());
        }

        let sector_total = sector_analysis.groups.iter().fold(dec!(0), |acc, group| {
            acc.checked_add(group.current_open_risk).unwrap_or(acc)
        });
        let asset_total = asset_class_analysis
            .groups
            .iter()
            .fold(dec!(0), |acc, group| {
                acc.checked_add(group.current_open_risk).unwrap_or(acc)
            });
        let sector_groups =
            Self::concentration_groups_json(&sector_analysis.groups, sector_analysis.total_risk);
        let asset_groups = Self::concentration_groups_json(
            &asset_class_analysis.groups,
            asset_class_analysis.total_risk,
        );

        let payload = json!({
            "report": "concentration",
            "format_version": 1,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "scope": { "account_id": account_id.map(|id| id.to_string()) },
            "filters": { "open_only": open_only },
            "data": {
                "sector": {
                    "total_risk": Self::decimal_string(sector_analysis.total_risk),
                    "groups": sector_groups,
                    "warnings": Self::warnings_json(&sector_analysis.concentration_warnings)
                },
                "asset_class": {
                    "total_risk": Self::decimal_string(asset_class_analysis.total_risk),
                    "groups": asset_groups,
                    "warnings": Self::warnings_json(&asset_class_analysis.concentration_warnings)
                }
            },
            "consistency": {
                "sector_group_sum": Self::decimal_string(sector_total),
                "sector_group_sum_matches_total": sector_total == sector_analysis.total_risk,
                "asset_group_sum": Self::decimal_string(asset_total),
                "asset_group_sum_matches_total": asset_total == asset_class_analysis.total_risk
            }
        });
        Self::print_json(&payload)?;

        Ok(())
    }

    fn concentration_groups_json(
        groups: &[core::calculators_concentration::ConcentrationGroup],
        total_risk: Decimal,
    ) -> Vec<Value> {
        let mut sorted_groups = groups.to_vec();
        sorted_groups.sort_by(|a, b| {
            b.current_open_risk
                .cmp(&a.current_open_risk)
                .then_with(|| a.name.cmp(&b.name))
        });
        sorted_groups
            .into_iter()
            .map(|group| {
                let risk_share = if total_risk > dec!(0) {
                    group
                        .current_open_risk
                        .checked_mul(dec!(100))
                        .and_then(|v| v.checked_div(total_risk))
                        .unwrap_or(dec!(0))
                } else {
                    dec!(0)
                };
                json!({
                    "name": group.name,
                    "trade_count": group.trade_count,
                    "total_capital_deployed": Self::decimal_string(group.total_capital_deployed),
                    "realized_pnl": Self::decimal_string(group.realized_pnl),
                    "current_open_risk": Self::decimal_string(group.current_open_risk),
                    "open_risk_share_percentage": Self::decimal_string(risk_share)
                })
            })
            .collect()
    }

    fn warnings_json(
        warnings: &[core::calculators_concentration::ConcentrationWarning],
    ) -> Vec<Value> {
        warnings
            .iter()
            .map(|warning| {
                json!({
                    "group_name": warning.group_name,
                    "risk_percentage": Self::decimal_string(warning.risk_percentage),
                    "level": format!("{:?}", warning.level)
                })
            })
            .collect()
    }

    #[allow(clippy::too_many_lines)]
    fn summary_report(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        use core::calculators_advanced_metrics::AdvancedMetricsCalculator;
        use core::calculators_performance::PerformanceCalculator;
        use core::calculators_risk::CapitalAtRiskCalculator;
        use std::str::FromStr;

        // Get account ID if provided
        let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
            match Uuid::from_str(account_arg) {
                Ok(id) => Some(id),
                Err(_) => {
                    return Err(Self::report_error(
                        format,
                        "invalid_account_id",
                        "Invalid account ID format",
                    ));
                }
            }
        } else {
            None
        };

        // Get trading summary
        let summary = match self.trust.get_trading_summary(account_id) {
            Ok(summary) => summary,
            Err(e) => {
                return Err(Self::report_error(
                    format,
                    "summary_generation_failed",
                    format!("Error generating trading summary: {e}"),
                ));
            }
        };

        let closed_trades = self
            .trust
            .search_closed_trades(account_id)
            .unwrap_or_else(|_| Vec::new());
        let performance = summary
            .performance
            .clone()
            .unwrap_or_else(|| PerformanceCalculator::calculate_performance_stats(&closed_trades));
        let total_risk =
            CapitalAtRiskCalculator::calculate_total_capital_at_risk(&summary.capital_at_risk)
                .unwrap_or(dec!(0));
        let risk_pct = if summary.equity > dec!(0) {
            total_risk
                .checked_mul(dec!(100))
                .and_then(|v| v.checked_div(summary.equity))
                .unwrap_or(dec!(0))
        } else {
            dec!(0)
        };

        // Text mode should stay fast and avoid heavy computations that are only used for JSON.
        if format == ReportOutputFormat::Text {
            println!("Trust Trading Summary");
            println!("====================");
            println!(
                "Scope: {}",
                summary
                    .account_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "all_accounts".to_string())
            );
            println!(
                "Equity: {}",
                crate::zen::currency_share(summary.equity, summary.equity)
            );
            println!(
                "Performance: {} trades ({}W/{}L), win rate {:.1}%, avg R {:.2}",
                performance.total_trades,
                performance.winning_trades,
                performance.losing_trades,
                performance.win_rate,
                performance.average_r_multiple
            );
            println!(
                "Risk: {} at risk ({:.2}% of equity), {} open positions",
                crate::zen::currency_share(total_risk, summary.equity),
                risk_pct,
                summary.capital_at_risk.len()
            );
            println!("Concentration groups: {}", summary.concentration.len());
            return Ok(());
        }

        let transactions = Self::transactions_for_scope(self, summary.account_id, None);
        let drawdown_metrics = Self::drawdown_metrics_for_transactions(&transactions);
        let report_calmar =
            Self::report_calmar_ratio(&closed_trades, &transactions, &drawdown_metrics);
        let profit_concentration =
            AdvancedMetricsCalculator::calculate_profit_concentration_metrics(&closed_trades);

        let advanced = json!({
            "gross_profit": Self::decimal_string(AdvancedMetricsCalculator::calculate_gross_profit(&closed_trades)),
            "gross_loss": Self::decimal_string(AdvancedMetricsCalculator::calculate_gross_loss(&closed_trades)),
            "net_profit": Self::decimal_string(AdvancedMetricsCalculator::calculate_net_profit(&closed_trades)),
            "average_trade_pnl": Self::decimal_string(AdvancedMetricsCalculator::calculate_average_trade_pnl(&closed_trades)),
            "median_trade_pnl": AdvancedMetricsCalculator::calculate_median_trade_pnl(&closed_trades).map(Self::decimal_string),
            "payoff_ratio": AdvancedMetricsCalculator::calculate_payoff_ratio(&closed_trades).map(Self::decimal_string),
            "profit_factor": AdvancedMetricsCalculator::calculate_profit_factor(&closed_trades).map(Self::decimal_string),
            "expectancy": Self::decimal_string(AdvancedMetricsCalculator::calculate_expectancy(&closed_trades)),
            "sharpe_ratio": AdvancedMetricsCalculator::calculate_sharpe_ratio(&closed_trades, dec!(0.05)).map(Self::decimal_string),
            "adjusted_sharpe_ratio": AdvancedMetricsCalculator::calculate_adjusted_sharpe_ratio(&closed_trades, dec!(0.05)).map(Self::decimal_string),
            "sortino_ratio": AdvancedMetricsCalculator::calculate_sortino_ratio(&closed_trades, dec!(0.05)).map(Self::decimal_string),
            "adjusted_sortino_ratio": AdvancedMetricsCalculator::calculate_adjusted_sortino_ratio(&closed_trades, dec!(0.05)).map(Self::decimal_string),
            "calmar_ratio": report_calmar.map(Self::decimal_string),
            "value_at_risk_95": AdvancedMetricsCalculator::calculate_value_at_risk(&closed_trades, dec!(0.95)).map(Self::decimal_string),
            "expected_shortfall_95": AdvancedMetricsCalculator::calculate_expected_shortfall(&closed_trades, dec!(0.95)).map(Self::decimal_string),
            "kelly_criterion": AdvancedMetricsCalculator::calculate_kelly_criterion(&closed_trades).map(Self::decimal_string),
            "max_consecutive_wins": AdvancedMetricsCalculator::calculate_max_consecutive_wins(&closed_trades),
            "max_consecutive_losses": AdvancedMetricsCalculator::calculate_max_consecutive_losses(&closed_trades),
            "ulcer_index": AdvancedMetricsCalculator::calculate_ulcer_index(&closed_trades).map(Self::decimal_string),
            "risk_of_ruin_proxy_100t_8l": AdvancedMetricsCalculator::calculate_risk_of_ruin_proxy(&closed_trades, 100, 8).map(Self::decimal_string),
            "profit_concentration": {
                "top_20pct_profit_share_percentage": Self::decimal_string(profit_concentration.top_20pct_profit_share_percentage),
                "trade_share_to_reach_80pct_profit_percentage": Self::decimal_string(profit_concentration.trade_share_to_reach_80pct_profit_percentage)
            }
        });
        let rolling = AdvancedMetricsCalculator::calculate_rolling_metrics(
            &closed_trades,
            &[30, 90, 252],
            dec!(0.05),
        );
        let rolling_json: Vec<Value> = rolling
            .iter()
            .map(|window| {
                json!({
                    "window_days": window.window_days,
                    "trade_count": window.trade_count,
                    "sharpe_ratio": window.sharpe_ratio.map(Self::decimal_string),
                    "sortino_ratio": window.sortino_ratio.map(Self::decimal_string),
                    "calmar_ratio": window.calmar_ratio.map(Self::decimal_string),
                    "expectancy": Self::decimal_string(window.expectancy),
                    "max_drawdown": Self::decimal_string(window.max_drawdown)
                })
            })
            .collect();
        let execution = AdvancedMetricsCalculator::calculate_execution_quality(&closed_trades);
        let journal = AdvancedMetricsCalculator::calculate_journal_quality(&closed_trades);
        let streaks_detail = AdvancedMetricsCalculator::calculate_streak_metrics(&closed_trades);
        let open_trades_for_scope = Self::open_trades_for_scope(&mut self.trust, account_id);
        let exposure =
            AdvancedMetricsCalculator::calculate_exposure_metrics(&open_trades_for_scope);
        let bootstrap = AdvancedMetricsCalculator::calculate_bootstrap_confidence_intervals(
            &closed_trades,
            200,
            dec!(0.05),
        );

        let (fee_open_total, fee_close_total, fees_total, fees_per_closed_trade) = {
            let mut fee_open = dec!(0);
            let mut fee_close = dec!(0);
            for tx in &transactions {
                match tx.category {
                    model::TransactionCategory::FeeOpen(_) => {
                        fee_open = fee_open.checked_add(tx.amount).unwrap_or(fee_open);
                    }
                    model::TransactionCategory::FeeClose(_) => {
                        fee_close = fee_close.checked_add(tx.amount).unwrap_or(fee_close);
                    }
                    _ => {}
                }
            }
            let total = fee_open.checked_add(fee_close).unwrap_or(fee_open);
            let per_trade = if closed_trades.is_empty() {
                dec!(0)
            } else {
                total
                    .checked_div(Decimal::from(closed_trades.len()))
                    .unwrap_or(dec!(0))
            };
            (fee_open, fee_close, total, per_trade)
        };

        let concentration_open_risk_total =
            summary.concentration.iter().fold(dec!(0), |acc, group| {
                acc.checked_add(group.current_open_risk).unwrap_or(acc)
            });
        let top_concentration_pct = if concentration_open_risk_total > dec!(0) {
            summary
                .concentration
                .iter()
                .map(|group| group.current_open_risk)
                .max()
                .unwrap_or(dec!(0))
                .checked_mul(dec!(100))
                .and_then(|v| v.checked_div(concentration_open_risk_total))
                .unwrap_or(dec!(0))
        } else {
            dec!(0)
        };
        let agent_signals = Self::summary_agent_signals(
            risk_pct,
            top_concentration_pct,
            drawdown_metrics.max_drawdown_percentage,
            AdvancedMetricsCalculator::calculate_expectancy(&closed_trades),
        );

        let payload = json!({
            "report": "summary",
            "format_version": 1,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "scope": { "account_id": summary.account_id.map(|id| id.to_string()) },
            "data": {
                "equity": Self::decimal_string(summary.equity),
                "performance": {
                    "total_trades": performance.total_trades,
                    "winning_trades": performance.winning_trades,
                    "losing_trades": performance.losing_trades,
                    "win_rate_percentage": Self::decimal_string(performance.win_rate),
                    "average_r_multiple": Self::decimal_string(performance.average_r_multiple),
                    "average_win": Self::decimal_string(performance.average_win),
                    "average_loss": Self::decimal_string(performance.average_loss),
                    "best_trade": performance.best_trade.map(Self::decimal_string),
                    "worst_trade": performance.worst_trade.map(Self::decimal_string)
                },
                "risk": {
                    "total_capital_at_risk": Self::decimal_string(total_risk),
                    "capital_at_risk_percentage": Self::decimal_string(risk_pct),
                    "open_positions_count": summary.capital_at_risk.len(),
                    "drawdown": {
                        "current_drawdown": Self::decimal_string(drawdown_metrics.current_drawdown),
                        "current_drawdown_percentage": Self::decimal_string(drawdown_metrics.current_drawdown_percentage),
                        "max_drawdown": Self::decimal_string(drawdown_metrics.max_drawdown),
                        "max_drawdown_percentage": Self::decimal_string(drawdown_metrics.max_drawdown_percentage),
                        "max_drawdown_date": drawdown_metrics.max_drawdown_date.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string())
                    }
                },
                "concentration": {
                    "group_count": summary.concentration.len(),
                    "total_open_risk": Self::decimal_string(concentration_open_risk_total),
                    "top_groups_by_open_risk": Self::concentration_groups_json(&summary.concentration, concentration_open_risk_total)
                },
                "costs": {
                    "fee_open_total": Self::decimal_string(fee_open_total),
                    "fee_close_total": Self::decimal_string(fee_close_total),
                    "fees_total": Self::decimal_string(fees_total),
                    "fees_per_closed_trade": Self::decimal_string(fees_per_closed_trade)
                },
                "advanced_metrics": advanced,
                "rolling_metrics": rolling_json,
                "execution_quality": {
                    "fill_price_coverage_percentage": Self::decimal_string(execution.fill_price_coverage_percentage),
                    "average_entry_slippage_bps": execution.average_entry_slippage_bps.map(Self::decimal_string),
                    "average_abs_entry_slippage_bps": execution.average_abs_entry_slippage_bps.map(Self::decimal_string),
                    "median_entry_slippage_bps": execution.median_entry_slippage_bps.map(Self::decimal_string),
                    "median_abs_entry_slippage_bps": execution.median_abs_entry_slippage_bps.map(Self::decimal_string),
                    "p95_abs_entry_slippage_bps": execution.p95_abs_entry_slippage_bps.map(Self::decimal_string),
                    "stop_fill_price_coverage_percentage": Self::decimal_string(execution.stop_fill_price_coverage_percentage),
                    "average_stop_slippage_bps": execution.average_stop_slippage_bps.map(Self::decimal_string),
                    "average_abs_stop_slippage_bps": execution.average_abs_stop_slippage_bps.map(Self::decimal_string),
                    "target_fill_price_coverage_percentage": Self::decimal_string(execution.target_fill_price_coverage_percentage),
                    "average_target_slippage_bps": execution.average_target_slippage_bps.map(Self::decimal_string),
                    "average_abs_target_slippage_bps": execution.average_abs_target_slippage_bps.map(Self::decimal_string),
                    "average_setup_reward_to_risk": execution.average_setup_reward_to_risk.map(Self::decimal_string),
                    "average_holding_days": Self::decimal_string(execution.average_holding_days),
                    "profit_per_holding_day": execution.profit_per_holding_day.map(Self::decimal_string)
                },
                "journal_quality": {
                    "thesis_coverage_percentage": Self::decimal_string(journal.thesis_coverage_percentage),
                    "sector_coverage_percentage": Self::decimal_string(journal.sector_coverage_percentage),
                    "asset_class_coverage_percentage": Self::decimal_string(journal.asset_class_coverage_percentage),
                    "context_coverage_percentage": Self::decimal_string(journal.context_coverage_percentage),
                    "complete_journal_percentage": Self::decimal_string(journal.complete_journal_percentage)
                },
                "streaks_detail": {
                    "max_consecutive_wins": streaks_detail.max_consecutive_wins,
                    "max_consecutive_losses": streaks_detail.max_consecutive_losses,
                    "average_win_streak": streaks_detail.average_win_streak.map(Self::decimal_string),
                    "average_loss_streak": streaks_detail.average_loss_streak.map(Self::decimal_string),
                    "current_streak_type": streaks_detail.current_streak_type,
                    "current_streak_len": streaks_detail.current_streak_len
                },
                "exposure": {
                    "gross_exposure": Self::decimal_string(exposure.gross_exposure),
                    "long_exposure": Self::decimal_string(exposure.long_exposure),
                    "net_exposure": Self::decimal_string(exposure.net_exposure),
                    "short_exposure": Self::decimal_string(exposure.short_exposure),
                    "top_3_symbol_concentration_percentage": Self::decimal_string(exposure.top_3_symbol_concentration_percentage),
                    "top_sector_concentration_percentage": Self::decimal_string(exposure.top_sector_concentration_percentage)
                },
                "confidence_intervals": {
                    "expectancy_95": bootstrap.expectancy_95.map(|(low, high)| json!({
                        "low": Self::decimal_string(low),
                        "high": Self::decimal_string(high)
                    })),
                    "sharpe_95": bootstrap.sharpe_95.map(|(low, high)| json!({
                        "low": Self::decimal_string(low),
                        "high": Self::decimal_string(high)
                    }))
                },
                "agent_signals": agent_signals
            },
            "consistency": {
                "trade_count_balanced": performance.total_trades == performance.winning_trades.saturating_add(performance.losing_trades),
                "risk_is_sum_of_positions": total_risk == summary.capital_at_risk.iter().fold(dec!(0), |acc, p| acc.checked_add(p.capital_amount).unwrap_or(acc))
            }
        });
        Self::print_json(&payload)?;

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn metrics_report(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        use crate::views::AdvancedMetricsView;
        use core::calculators_advanced_metrics::AdvancedMetricsCalculator;
        use core::calculators_performance::PerformanceCalculator;
        use std::str::FromStr;

        // Get account ID if provided
        let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
            match Uuid::from_str(account_arg) {
                Ok(id) => Some(id),
                Err(_) => {
                    return Err(Self::report_error(
                        format,
                        "invalid_account_id",
                        "Invalid account ID format",
                    ));
                }
            }
        } else {
            None
        };

        // Get days filter if provided
        let days_filter = sub_matches.get_one::<u32>("days").copied();

        // Get all closed trades
        let mut all_trades = match self.trust.search_closed_trades(account_id) {
            Ok(trades) => trades,
            Err(_) => {
                return Err(Self::report_error(
                    format,
                    "trading_data_unavailable",
                    "Unable to retrieve trading data. Please check your account settings and try again.",
                ));
            }
        };

        // Apply days filter if specified
        if let Some(days) = days_filter {
            all_trades = PerformanceCalculator::filter_trades_by_days(&all_trades, days);
        }

        let transactions = Self::transactions_for_scope(self, account_id, days_filter);
        let drawdown_metrics = Self::drawdown_metrics_for_transactions(&transactions);
        let report_calmar =
            Self::report_calmar_ratio(&all_trades, &transactions, &drawdown_metrics);
        let profit_concentration =
            AdvancedMetricsCalculator::calculate_profit_concentration_metrics(&all_trades);

        if format == ReportOutputFormat::Text {
            AdvancedMetricsView::display(
                all_trades,
                Some(crate::views::AdvancedMetricsDisplayContext {
                    calmar_ratio: report_calmar,
                    risk_free_rate: Some(dec!(0.05)),
                }),
            );
            return Ok(());
        }

        let gross_profit = AdvancedMetricsCalculator::calculate_gross_profit(&all_trades);
        let gross_loss = AdvancedMetricsCalculator::calculate_gross_loss(&all_trades);
        let net_profit = AdvancedMetricsCalculator::calculate_net_profit(&all_trades);
        let average_trade_pnl = AdvancedMetricsCalculator::calculate_average_trade_pnl(&all_trades);
        let median_trade_pnl = AdvancedMetricsCalculator::calculate_median_trade_pnl(&all_trades);

        let profit_factor = AdvancedMetricsCalculator::calculate_profit_factor(&all_trades);
        let expectancy = AdvancedMetricsCalculator::calculate_expectancy(&all_trades);
        let win_rate = AdvancedMetricsCalculator::calculate_win_rate(&all_trades);
        let average_r = AdvancedMetricsCalculator::calculate_average_r_multiple(&all_trades);
        let payoff_ratio = AdvancedMetricsCalculator::calculate_payoff_ratio(&all_trades);
        let sharpe = AdvancedMetricsCalculator::calculate_sharpe_ratio(&all_trades, dec!(0.05));
        let adjusted_sharpe =
            AdvancedMetricsCalculator::calculate_adjusted_sharpe_ratio(&all_trades, dec!(0.05));
        let sortino = AdvancedMetricsCalculator::calculate_sortino_ratio(&all_trades, dec!(0.05));
        let adjusted_sortino =
            AdvancedMetricsCalculator::calculate_adjusted_sortino_ratio(&all_trades, dec!(0.05));
        let calmar = report_calmar;
        let var_95 = AdvancedMetricsCalculator::calculate_value_at_risk(&all_trades, dec!(0.95));
        let es_95 =
            AdvancedMetricsCalculator::calculate_expected_shortfall(&all_trades, dec!(0.95));
        let kelly = AdvancedMetricsCalculator::calculate_kelly_criterion(&all_trades);
        let max_losses = AdvancedMetricsCalculator::calculate_max_consecutive_losses(&all_trades);
        let max_wins = AdvancedMetricsCalculator::calculate_max_consecutive_wins(&all_trades);
        let ulcer = AdvancedMetricsCalculator::calculate_ulcer_index(&all_trades);
        let risk_of_ruin =
            AdvancedMetricsCalculator::calculate_risk_of_ruin_proxy(&all_trades, 100, 8);
        let rolling = AdvancedMetricsCalculator::calculate_rolling_metrics(
            &all_trades,
            &[30, 90, 252],
            dec!(0.05),
        );
        let rolling_json: Vec<Value> = rolling
            .iter()
            .map(|window| {
                json!({
                    "window_days": window.window_days,
                    "trade_count": window.trade_count,
                    "sharpe_ratio": window.sharpe_ratio.map(Self::decimal_string),
                    "sortino_ratio": window.sortino_ratio.map(Self::decimal_string),
                    "calmar_ratio": window.calmar_ratio.map(Self::decimal_string),
                    "expectancy": Self::decimal_string(window.expectancy),
                    "max_drawdown": Self::decimal_string(window.max_drawdown)
                })
            })
            .collect();
        let execution = AdvancedMetricsCalculator::calculate_execution_quality(&all_trades);
        let journal = AdvancedMetricsCalculator::calculate_journal_quality(&all_trades);
        let streaks_detail = AdvancedMetricsCalculator::calculate_streak_metrics(&all_trades);
        let open_trades_for_scope = Self::open_trades_for_scope(&mut self.trust, account_id);
        let exposure =
            AdvancedMetricsCalculator::calculate_exposure_metrics(&open_trades_for_scope);
        let bootstrap = AdvancedMetricsCalculator::calculate_bootstrap_confidence_intervals(
            &all_trades,
            200,
            dec!(0.05),
        );
        let agent_signals =
            Self::metrics_agent_signals(expectancy, win_rate, risk_of_ruin, max_losses);

        let (fee_open_total, fee_close_total, fees_total, fees_per_closed_trade) = {
            let mut fee_open = dec!(0);
            let mut fee_close = dec!(0);
            for tx in &transactions {
                match tx.category {
                    model::TransactionCategory::FeeOpen(_) => {
                        fee_open = fee_open.checked_add(tx.amount).unwrap_or(fee_open);
                    }
                    model::TransactionCategory::FeeClose(_) => {
                        fee_close = fee_close.checked_add(tx.amount).unwrap_or(fee_close);
                    }
                    _ => {}
                }
            }
            let total = fee_open.checked_add(fee_close).unwrap_or(fee_open);
            let per_trade = if all_trades.is_empty() {
                dec!(0)
            } else {
                total
                    .checked_div(Decimal::from(all_trades.len()))
                    .unwrap_or(dec!(0))
            };
            (fee_open, fee_close, total, per_trade)
        };

        let payload = json!({
            "report": "metrics",
            "format_version": 1,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "scope": { "account_id": account_id.map(|id| id.to_string()) },
            "filters": { "days": days_filter },
            "data": {
                "trade_count": all_trades.len(),
                "pnl": {
                    "gross_profit": Self::decimal_string(gross_profit),
                    "gross_loss": Self::decimal_string(gross_loss),
                    "net_profit": Self::decimal_string(net_profit),
                    "average_trade_pnl": Self::decimal_string(average_trade_pnl),
                    "median_trade_pnl": median_trade_pnl.map(Self::decimal_string)
                },
                "costs": {
                    "fee_open_total": Self::decimal_string(fee_open_total),
                    "fee_close_total": Self::decimal_string(fee_close_total),
                    "fees_total": Self::decimal_string(fees_total),
                    "fees_per_closed_trade": Self::decimal_string(fees_per_closed_trade)
                },
                "trade_quality": {
                    "profit_factor": profit_factor.map(Self::decimal_string),
                    "expectancy": Self::decimal_string(expectancy),
                    "win_rate_percentage": Self::decimal_string(win_rate),
                    "average_r_multiple": Self::decimal_string(average_r),
                    "payoff_ratio": payoff_ratio.map(Self::decimal_string)
                },
                "profit_concentration": {
                    "top_20pct_profit_share_percentage": Self::decimal_string(profit_concentration.top_20pct_profit_share_percentage),
                    "trade_share_to_reach_80pct_profit_percentage": Self::decimal_string(profit_concentration.trade_share_to_reach_80pct_profit_percentage)
                },
                "risk_adjusted_performance": {
                    "sharpe_ratio": sharpe.map(Self::decimal_string),
                    "adjusted_sharpe_ratio": adjusted_sharpe.map(Self::decimal_string),
                    "sortino_ratio": sortino.map(Self::decimal_string),
                    "adjusted_sortino_ratio": adjusted_sortino.map(Self::decimal_string),
                    "calmar_ratio": calmar.map(Self::decimal_string),
                    "ulcer_index": ulcer.map(Self::decimal_string)
                },
                "tail_and_position_sizing": {
                    "value_at_risk_95": var_95.map(Self::decimal_string),
                    "expected_shortfall_95": es_95.map(Self::decimal_string),
                    "kelly_criterion": kelly.map(Self::decimal_string)
                },
                "streaks": {
                    "max_consecutive_wins": max_wins,
                    "max_consecutive_losses": max_losses
                },
                "streaks_detail": {
                    "max_consecutive_wins": streaks_detail.max_consecutive_wins,
                    "max_consecutive_losses": streaks_detail.max_consecutive_losses,
                    "average_win_streak": streaks_detail.average_win_streak.map(Self::decimal_string),
                    "average_loss_streak": streaks_detail.average_loss_streak.map(Self::decimal_string),
                    "current_streak_type": streaks_detail.current_streak_type,
                    "current_streak_len": streaks_detail.current_streak_len
                },
                "risk_of_ruin_proxy_100t_8l": risk_of_ruin.map(Self::decimal_string),
                "rolling_metrics": rolling_json,
                "execution_quality": {
                    "fill_price_coverage_percentage": Self::decimal_string(execution.fill_price_coverage_percentage),
                    "average_entry_slippage_bps": execution.average_entry_slippage_bps.map(Self::decimal_string),
                    "average_abs_entry_slippage_bps": execution.average_abs_entry_slippage_bps.map(Self::decimal_string),
                    "median_entry_slippage_bps": execution.median_entry_slippage_bps.map(Self::decimal_string),
                    "median_abs_entry_slippage_bps": execution.median_abs_entry_slippage_bps.map(Self::decimal_string),
                    "p95_abs_entry_slippage_bps": execution.p95_abs_entry_slippage_bps.map(Self::decimal_string),
                    "stop_fill_price_coverage_percentage": Self::decimal_string(execution.stop_fill_price_coverage_percentage),
                    "average_stop_slippage_bps": execution.average_stop_slippage_bps.map(Self::decimal_string),
                    "average_abs_stop_slippage_bps": execution.average_abs_stop_slippage_bps.map(Self::decimal_string),
                    "target_fill_price_coverage_percentage": Self::decimal_string(execution.target_fill_price_coverage_percentage),
                    "average_target_slippage_bps": execution.average_target_slippage_bps.map(Self::decimal_string),
                    "average_abs_target_slippage_bps": execution.average_abs_target_slippage_bps.map(Self::decimal_string),
                    "average_setup_reward_to_risk": execution.average_setup_reward_to_risk.map(Self::decimal_string),
                    "average_holding_days": Self::decimal_string(execution.average_holding_days),
                    "profit_per_holding_day": execution.profit_per_holding_day.map(Self::decimal_string)
                },
                "journal_quality": {
                    "thesis_coverage_percentage": Self::decimal_string(journal.thesis_coverage_percentage),
                    "sector_coverage_percentage": Self::decimal_string(journal.sector_coverage_percentage),
                    "asset_class_coverage_percentage": Self::decimal_string(journal.asset_class_coverage_percentage),
                    "context_coverage_percentage": Self::decimal_string(journal.context_coverage_percentage),
                    "complete_journal_percentage": Self::decimal_string(journal.complete_journal_percentage)
                },
                "exposure": {
                    "gross_exposure": Self::decimal_string(exposure.gross_exposure),
                    "long_exposure": Self::decimal_string(exposure.long_exposure),
                    "net_exposure": Self::decimal_string(exposure.net_exposure),
                    "short_exposure": Self::decimal_string(exposure.short_exposure),
                    "top_3_symbol_concentration_percentage": Self::decimal_string(exposure.top_3_symbol_concentration_percentage),
                    "top_sector_concentration_percentage": Self::decimal_string(exposure.top_sector_concentration_percentage)
                },
                "confidence_intervals": {
                    "expectancy_95": bootstrap.expectancy_95.map(|(low, high)| json!({
                        "low": Self::decimal_string(low),
                        "high": Self::decimal_string(high)
                    })),
                    "sharpe_95": bootstrap.sharpe_95.map(|(low, high)| json!({
                        "low": Self::decimal_string(low),
                        "high": Self::decimal_string(high)
                    }))
                },
                "agent_signals": agent_signals
            }
        });
        Self::print_json(&payload)?;

        Ok(())
    }

    fn parse_required_date_range(
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(NaiveDate, NaiveDate), CliError> {
        let from_raw = sub_matches
            .get_one::<String>("from")
            .ok_or_else(|| Self::report_error(format, "missing_argument", "Missing --from"))?;
        let to_raw = sub_matches
            .get_one::<String>("to")
            .ok_or_else(|| Self::report_error(format, "missing_argument", "Missing --to"))?;
        let from = NaiveDate::parse_from_str(from_raw, "%Y-%m-%d")
            .map_err(|_| Self::report_error(format, "invalid_date", "Invalid --from date"))?;
        let to = NaiveDate::parse_from_str(to_raw, "%Y-%m-%d")
            .map_err(|_| Self::report_error(format, "invalid_date", "Invalid --to date"))?;
        if from > to {
            return Err(Self::report_error(
                format,
                "invalid_date_range",
                "--from must be less than or equal to --to",
            ));
        }
        Ok((from, to))
    }

    fn transaction_cashflow_delta(tx: &model::Transaction) -> Decimal {
        match tx.category {
            TransactionCategory::Deposit
            | TransactionCategory::CloseTarget(_)
            | TransactionCategory::CloseSafetyStop(_)
            | TransactionCategory::CloseSafetyStopSlippage(_) => tx.amount,
            TransactionCategory::Withdrawal
            | TransactionCategory::OpenTrade(_)
            | TransactionCategory::FeeOpen(_)
            | TransactionCategory::FeeClose(_)
            | TransactionCategory::WithdrawalTax
            | TransactionCategory::WithdrawalEarnings => {
                Decimal::ZERO.checked_sub(tx.amount).unwrap_or(dec!(0))
            }
            TransactionCategory::FundTrade(_)
            | TransactionCategory::PaymentFromTrade(_)
            | TransactionCategory::PaymentTax(_)
            | TransactionCategory::PaymentEarnings(_) => Decimal::ZERO,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn attribution_report(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_raw = sub_matches
            .get_one::<String>("account")
            .ok_or_else(|| Self::report_error(format, "missing_argument", "Missing --account"))?;
        let account = self.resolve_account_arg(account_raw, format)?;
        let by = sub_matches
            .get_one::<String>("by")
            .ok_or_else(|| Self::report_error(format, "missing_argument", "Missing --by"))?;
        if !matches!(by.as_str(), "symbol" | "sector" | "asset-class") {
            return Err(Self::report_error(
                format,
                "invalid_argument",
                "Invalid --by value",
            ));
        }
        let (from, to) = Self::parse_required_date_range(sub_matches, format)?;

        let trades = self
            .trust
            .search_closed_trades(Some(account.id))
            .map_err(|e| Self::report_error(format, "report_failed", e.to_string()))?;
        let in_window: Vec<Trade> = trades
            .into_iter()
            .filter(|trade| {
                let day = trade.updated_at.date();
                day >= from && day <= to
            })
            .collect();

        let mut groups: HashMap<String, (u64, Decimal, Decimal)> = HashMap::new();
        for trade in &in_window {
            let key = match by.as_str() {
                "symbol" => trade.trading_vehicle.symbol.clone(),
                "sector" => trade
                    .sector
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                "asset-class" => trade
                    .asset_class
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                _ => unreachable!("validated --by before iterating"),
            };
            let entry = groups
                .entry(key)
                .or_insert((0, Decimal::ZERO, Decimal::ZERO));
            entry.0 = entry.0.saturating_add(1);
            entry.1 = entry
                .1
                .checked_add(trade.balance.total_performance)
                .unwrap_or(entry.1);
            entry.2 = entry
                .2
                .checked_add(trade.balance.funding)
                .unwrap_or(entry.2);
        }

        let total_pnl = in_window.iter().fold(Decimal::ZERO, |acc, trade| {
            acc.checked_add(trade.balance.total_performance)
                .unwrap_or(acc)
        });
        let total_funding = in_window.iter().fold(Decimal::ZERO, |acc, trade| {
            acc.checked_add(trade.balance.funding).unwrap_or(acc)
        });
        let mut rows: Vec<(String, u64, Decimal, Decimal)> = groups
            .into_iter()
            .map(|(key, (count, pnl, funding))| (key, count, pnl, funding))
            .collect();
        rows.sort_by_key(|row| std::cmp::Reverse(row.2));

        match format {
            ReportOutputFormat::Text => {
                println!("Attribution report ({by})");
                println!("=======================");
                println!("account_id: {}", account.id);
                println!("from: {from}");
                println!("to: {to}");
                println!("closed_trades: {}", in_window.len());
                println!(
                    "total_pnl: {}",
                    crate::zen::amount_share(total_pnl, total_pnl)
                );
                for (key, count, pnl, funding) in &rows {
                    let share = if total_pnl == Decimal::ZERO {
                        Decimal::ZERO
                    } else {
                        pnl.checked_div(total_pnl)
                            .and_then(|v| v.checked_mul(dec!(100)))
                            .unwrap_or(Decimal::ZERO)
                    };
                    println!(
                        "{key}: trades={count} pnl={} funding={} share_pct={}",
                        crate::zen::amount_share(*pnl, total_pnl),
                        crate::zen::amount_share(*funding, total_funding),
                        Self::decimal_string(share),
                    );
                }
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "attribution",
                    "schema_version": 1,
                    "generated_at": Utc::now().to_rfc3339(),
                    "source": { "broker": "alpaca", "provider": "trust_core" },
                    "scope": {
                        "account_id": account.id.to_string(),
                        "from": from.to_string(),
                        "to": to.to_string(),
                        "by": by,
                    },
                    "data": {
                        "closed_trade_count": in_window.len(),
                        "total_pnl": Self::decimal_string(total_pnl),
                        "groups": rows.iter().map(|(key, count, pnl, funding)| {
                            let share = if total_pnl == Decimal::ZERO {
                                Decimal::ZERO
                            } else {
                                pnl.checked_div(total_pnl)
                                    .and_then(|v| v.checked_mul(dec!(100)))
                                    .unwrap_or(Decimal::ZERO)
                            };
                            json!({
                                "key": key,
                                "trade_count": count,
                                "pnl": Self::decimal_string(*pnl),
                                "funding": Self::decimal_string(*funding),
                                "share_pct": Self::decimal_string(share),
                            })
                        }).collect::<Vec<Value>>()
                    },
                    "consistency": {
                        "sum_group_pnl": Self::decimal_string(rows.iter().fold(Decimal::ZERO, |acc, row| acc.checked_add(row.2).unwrap_or(acc))),
                        "matches_total_pnl": rows.iter().fold(Decimal::ZERO, |acc, row| acc.checked_add(row.2).unwrap_or(acc)) == total_pnl
                    }
                });
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn benchmark_report(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_raw = sub_matches
            .get_one::<String>("account")
            .ok_or_else(|| Self::report_error(format, "missing_argument", "Missing --account"))?;
        let account = self.resolve_account_arg(account_raw, format)?;
        let benchmark_symbol = sub_matches
            .get_one::<String>("benchmark")
            .ok_or_else(|| Self::report_error(format, "missing_argument", "Missing --benchmark"))?
            .trim()
            .to_uppercase();
        let (from, to) = Self::parse_required_date_range(sub_matches, format)?;
        let start = DateTime::<Utc>::from_naive_utc_and_offset(
            from.and_hms_opt(0, 0, 0).unwrap_or_default(),
            Utc,
        );
        let end = DateTime::<Utc>::from_naive_utc_and_offset(
            to.and_hms_opt(23, 59, 59).unwrap_or_default(),
            Utc,
        );

        let closed = self
            .trust
            .search_closed_trades(Some(account.id))
            .map_err(|e| Self::report_error(format, "report_failed", e.to_string()))?;
        let scoped: Vec<Trade> = closed
            .into_iter()
            .filter(|trade| {
                let day = trade.updated_at.date();
                day >= from && day <= to
            })
            .collect();

        let funding = scoped.iter().fold(Decimal::ZERO, |acc, trade| {
            acc.checked_add(trade.balance.funding).unwrap_or(acc)
        });
        let pnl = scoped.iter().fold(Decimal::ZERO, |acc, trade| {
            acc.checked_add(trade.balance.total_performance)
                .unwrap_or(acc)
        });
        let strategy_return_pct = if funding == Decimal::ZERO {
            Decimal::ZERO
        } else {
            pnl.checked_div(funding)
                .and_then(|v| v.checked_mul(dec!(100)))
                .unwrap_or(Decimal::ZERO)
        };

        let mut bars = self
            .trust
            .market_bars(
                &account,
                &benchmark_symbol,
                start,
                end,
                model::BarTimeframe::OneDay,
            )
            .map_err(|e| {
                Self::report_error(
                    format,
                    "benchmark_data_failed",
                    format!("Unable to retrieve benchmark bars: {e}"),
                )
            })?;
        bars.sort_by_key(|bar| bar.time);
        let benchmark_return_pct = if let (Some(first), Some(last)) = (bars.first(), bars.last()) {
            if first.open == Decimal::ZERO {
                Decimal::ZERO
            } else {
                last.close
                    .checked_sub(first.open)
                    .and_then(|v| v.checked_div(first.open))
                    .and_then(|v| v.checked_mul(dec!(100)))
                    .unwrap_or(Decimal::ZERO)
            }
        } else {
            Decimal::ZERO
        };
        let alpha_pct = strategy_return_pct
            .checked_sub(benchmark_return_pct)
            .unwrap_or(Decimal::ZERO);

        match format {
            ReportOutputFormat::Text => {
                println!("Benchmark report");
                println!("================");
                println!("account_id: {}", account.id);
                println!("benchmark: {benchmark_symbol}");
                println!("from: {from}");
                println!("to: {to}");
                println!("closed_trades: {}", scoped.len());
                println!(
                    "strategy_return_pct: {}",
                    Self::decimal_string(strategy_return_pct)
                );
                println!(
                    "benchmark_return_pct: {}",
                    Self::decimal_string(benchmark_return_pct)
                );
                println!("alpha_pct: {}", Self::decimal_string(alpha_pct));
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "benchmark",
                    "schema_version": 1,
                    "generated_at": Utc::now().to_rfc3339(),
                    "source": { "broker": "alpaca", "provider": "trust_core" },
                    "scope": {
                        "account_id": account.id.to_string(),
                        "benchmark": benchmark_symbol,
                        "from": from.to_string(),
                        "to": to.to_string(),
                    },
                    "data": {
                        "closed_trade_count": scoped.len(),
                        "strategy": {
                            "funding": Self::decimal_string(funding),
                            "pnl": Self::decimal_string(pnl),
                            "return_pct": Self::decimal_string(strategy_return_pct),
                        },
                        "benchmark": {
                            "bars": bars.len(),
                            "return_pct": Self::decimal_string(benchmark_return_pct),
                        },
                        "alpha_pct": Self::decimal_string(alpha_pct),
                    }
                });
                Self::print_json(&payload)?;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn timeline_report(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_raw = sub_matches
            .get_one::<String>("account")
            .ok_or_else(|| Self::report_error(format, "missing_argument", "Missing --account"))?;
        let account = self.resolve_account_arg(account_raw, format)?;
        let granularity = sub_matches
            .get_one::<String>("granularity")
            .ok_or_else(|| {
                Self::report_error(format, "missing_argument", "Missing --granularity")
            })?;
        if !matches!(granularity.as_str(), "day" | "week" | "month") {
            return Err(Self::report_error(
                format,
                "invalid_argument",
                "Invalid --granularity value",
            ));
        }
        let (from, to) = Self::parse_required_date_range(sub_matches, format)?;

        let txs = self
            .trust
            .get_account_transactions(account.id)
            .map_err(|e| Self::report_error(format, "report_failed", e.to_string()))?;
        let scoped: Vec<model::Transaction> = txs
            .into_iter()
            .filter(|tx| {
                let day = tx.created_at.date();
                day >= from && day <= to
            })
            .collect();

        let mut buckets: BTreeMap<String, (u64, Decimal)> = BTreeMap::new();
        for tx in &scoped {
            let day = tx.created_at.date();
            let key = match granularity.as_str() {
                "day" => day.format("%Y-%m-%d").to_string(),
                "week" => {
                    let iso = day.iso_week();
                    format!("{}-W{:02}", iso.year(), iso.week())
                }
                "month" => day.format("%Y-%m").to_string(),
                _ => unreachable!("validated --granularity before iterating"),
            };
            let signed = Self::transaction_cashflow_delta(tx);
            let entry = buckets.entry(key).or_insert((0, Decimal::ZERO));
            entry.0 = entry.0.saturating_add(1);
            entry.1 = entry.1.checked_add(signed).unwrap_or(entry.1);
        }

        let total_net = buckets.values().fold(Decimal::ZERO, |acc, (_, net)| {
            acc.checked_add(*net).unwrap_or(acc)
        });

        match format {
            ReportOutputFormat::Text => {
                println!("Timeline report");
                println!("===============");
                println!("account_id: {}", account.id);
                println!("granularity: {granularity}");
                println!("from: {from}");
                println!("to: {to}");
                println!("bucket_count: {}", buckets.len());
                println!(
                    "net_cash_flow: {}",
                    crate::zen::amount_share(total_net, total_net)
                );
                for (bucket, (count, net)) in &buckets {
                    println!(
                        "{bucket}: events={count} net_cash_flow={}",
                        crate::zen::amount_share(*net, total_net)
                    );
                }
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "timeline",
                    "schema_version": 1,
                    "generated_at": Utc::now().to_rfc3339(),
                    "source": { "broker": "alpaca", "provider": "trust_core" },
                    "scope": {
                        "account_id": account.id.to_string(),
                        "granularity": granularity,
                        "from": from.to_string(),
                        "to": to.to_string(),
                    },
                    "data": {
                        "bucket_count": buckets.len(),
                        "net_cash_flow": Self::decimal_string(total_net),
                        "buckets": buckets.iter().map(|(bucket, (count, net))| json!({
                            "bucket": bucket,
                            "event_count": count,
                            "net_cash_flow": Self::decimal_string(*net),
                        })).collect::<Vec<Value>>()
                    },
                    "consistency": {
                        "sum_bucket_net": Self::decimal_string(total_net),
                        "bucket_total_matches": total_net == buckets
                            .values()
                            .fold(Decimal::ZERO, |acc, (_, net)| acc.checked_add(*net).unwrap_or(acc))
                    }
                });
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn bias_summary_report(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_raw = sub_matches
            .get_one::<String>("account")
            .ok_or_else(|| Self::report_error(format, "missing_argument", "Missing --account"))?;
        let account = self.resolve_account_arg(account_raw, format)?;
        let days = sub_matches.get_one::<i32>("days").copied().unwrap_or(7);
        let aggregation = self
            .trust
            .bias_aggregation_for_account(account.id, days)
            .map_err(|error| Self::report_error(format, "report_failed", error.to_string()))?;

        match format {
            ReportOutputFormat::Text => Self::print_bias_summary_text(&aggregation),
            ReportOutputFormat::Json => {
                let payload = Self::bias_aggregation_payload(account.id, &aggregation);
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

    fn print_bias_summary_text(aggregation: &BiasAggregation) {
        println!(
            "Bias Summary (last {} days, {} mistakes)",
            aggregation.window_days, aggregation.total_mistakes
        );
        println!("----------------------------------------");
        if let Some((tendency_number, count)) = aggregation.most_frequent_tendency {
            let label = Self::munger_tendency_label_by_number(tendency_number);
            let percentage = Self::bias_count_percentage(count, aggregation.total_mistakes);
            println!(
                "Most frequent:  #{} {} ({}/{} = {}%)",
                tendency_number, label, count, aggregation.total_mistakes, percentage
            );
            if aggregation.mechanical_antidote_needed {
                println!("                -> Consider mechanical antidote");
            }
        } else {
            println!("Most frequent:  n/a");
        }
        println!("Lollapaloozas:  {}", aggregation.lollapalooza_count);
        println!("Omissions:      {}", aggregation.omission_count);
        println!("Commissions:    {}", aggregation.commission_count);
        println!(
            "Standdown:      {}",
            if aggregation.standdown_recommended {
                "Yes"
            } else {
                "No"
            }
        );
        println!(
            "Mechanical antidote: {}",
            if aggregation.mechanical_antidote_needed {
                "Yes"
            } else {
                "No"
            }
        );
    }

    fn bias_aggregation_payload(account_id: Uuid, aggregation: &BiasAggregation) -> Value {
        json!({
            "report": "bias_summary",
            "schema_version": 1,
            "generated_at": Utc::now().to_rfc3339(),
            "account_id": account_id.to_string(),
            "window_days": aggregation.window_days,
            "total_mistakes": aggregation.total_mistakes,
            "most_frequent_tendency": Self::most_frequent_tendency_payload(aggregation),
            "lollapalooza_count": aggregation.lollapalooza_count,
            "omission_count": aggregation.omission_count,
            "commission_count": aggregation.commission_count,
            "standdown_recommended": aggregation.standdown_recommended,
            "mechanical_antidote_needed": aggregation.mechanical_antidote_needed,
        })
    }

    fn most_frequent_tendency_payload(aggregation: &BiasAggregation) -> Value {
        aggregation
            .most_frequent_tendency
            .map(|(tendency_number, count)| {
                json!({
                    "tendency_number": tendency_number,
                    "count": count,
                    "label": Self::munger_tendency_label_by_number(tendency_number),
                    "percentage": Self::bias_count_percentage(count, aggregation.total_mistakes),
                })
            })
            .unwrap_or(Value::Null)
    }

    fn bias_count_percentage(count: i32, total: i32) -> String {
        if total <= 0 {
            return "0".to_string();
        }

        let percentage = Decimal::from(count)
            .checked_mul(dec!(100))
            .and_then(|value| value.checked_div(Decimal::from(total)))
            .unwrap_or(Decimal::ZERO)
            .round_dp(0);
        Self::decimal_string(percentage)
    }

    #[allow(clippy::too_many_lines)]
    fn grade_show(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        use std::str::FromStr;

        let trade_id = sub_matches
            .get_one::<String>("trade_id")
            .ok_or_else(|| Self::report_error(format, "missing_trade_id", "Missing trade id"))?;
        let trade_id = Uuid::from_str(trade_id).map_err(|_| {
            Self::report_error(format, "invalid_trade_id", "Invalid trade ID format")
        })?;

        let regrade = sub_matches.get_flag("regrade");
        let requested_weights = if sub_matches.get_one::<String>("weights").is_some() {
            Some(Self::parse_grade_weights(sub_matches, format)?)
        } else {
            None
        };

        let existing = self.trust.latest_trade_grade(trade_id).map_err(|e| {
            Self::report_error(
                format,
                "grade_read_failed",
                format!("Failed to read existing grade: {e}"),
            )
        })?;

        let grade = if let Some(existing) = existing {
            if regrade {
                let weights =
                    requested_weights.unwrap_or(core::services::grading::GradingWeightsPermille {
                        process: existing.process_weight_permille,
                        risk: existing.risk_weight_permille,
                        execution: existing.execution_weight_permille,
                        documentation: existing.documentation_weight_permille,
                    });
                self.trust
                    .grade_trade(trade_id, weights)
                    .map(|d| d.grade)
                    .map_err(|e| {
                        Self::report_error(
                            format,
                            "grade_compute_failed",
                            format!("Failed to compute grade: {e}"),
                        )
                    })?
            } else {
                if let Some(weights) = requested_weights {
                    let same_weights = existing.process_weight_permille == weights.process
                        && existing.risk_weight_permille == weights.risk
                        && existing.execution_weight_permille == weights.execution
                        && existing.documentation_weight_permille == weights.documentation;
                    if !same_weights {
                        return Err(Self::report_error(
                            format,
                            "weights_mismatch",
                            "Existing grade uses different weights. Re-run with --regrade or omit --weights.",
                        ));
                    }
                }
                existing
            }
        } else {
            let weights = requested_weights
                .unwrap_or_else(core::services::grading::GradingWeightsPermille::default);
            self.trust
                .grade_trade(trade_id, weights)
                .map(|d| d.grade)
                .map_err(|e| {
                    Self::report_error(
                        format,
                        "grade_compute_failed",
                        format!("Failed to compute grade: {e}"),
                    )
                })?
        };

        if format == ReportOutputFormat::Json {
            let payload = json!({
                "report": "grade_show",
                "format_version": 1,
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "data": {
                    "trade_id": grade.trade_id.to_string(),
                    "overall": {
                        "grade": grade.overall_grade.to_string(),
                        "score": grade.overall_score,
                    },
                    "components": {
                        "process": grade.process_score,
                        "risk": grade.risk_score,
                        "execution": grade.execution_score,
                        "documentation": grade.documentation_score,
                    },
                    "weights_permille": {
                        "process": grade.process_weight_permille,
                        "risk": grade.risk_weight_permille,
                        "execution": grade.execution_weight_permille,
                        "documentation": grade.documentation_weight_permille,
                    },
                    "recommendations": grade.recommendations,
                    "graded_at": grade.graded_at.format("%Y-%m-%dT%H:%M:%S").to_string()
                }
            });
            Self::print_json(&payload)?;
            return Ok(());
        }

        println!("Trade Grade Report: {trade_id}");
        println!("=====================================");
        println!(
            "Overall Grade: {} ({}/100)",
            grade.overall_grade, grade.overall_score
        );
        println!();
        println!("Breakdown:");
        println!(
            "Process Adherence: {}/100 (weight {}‰)",
            grade.process_score, grade.process_weight_permille
        );
        println!(
            "Risk Management:   {}/100 (weight {}‰)",
            grade.risk_score, grade.risk_weight_permille
        );
        println!(
            "Execution Quality: {}/100 (weight {}‰)",
            grade.execution_score, grade.execution_weight_permille
        );
        println!(
            "Documentation:     {}/100 (weight {}‰)",
            grade.documentation_score, grade.documentation_weight_permille
        );
        println!();
        if grade.recommendations.is_empty() {
            println!("Recommendations: (none)");
        } else {
            println!("Recommendations:");
            for rec in &grade.recommendations {
                println!("• {rec}");
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn grade_summary(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        use core::calculators_performance::PerformanceCalculator;
        use std::collections::HashMap;
        use std::str::FromStr;

        let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
            Some(Uuid::from_str(account_arg).map_err(|_| {
                Self::report_error(format, "invalid_account_id", "Invalid account ID format")
            })?)
        } else {
            None
        };

        let days = sub_matches.get_one::<u32>("days").copied().unwrap_or(30);
        let weights = Self::parse_grade_weights(sub_matches, format)?;

        let mut closed_trades = self.trust.search_closed_trades(account_id).map_err(|e| {
            Self::report_error(
                format,
                "trading_data_unavailable",
                format!("Unable to retrieve closed trades: {e}"),
            )
        })?;

        closed_trades = PerformanceCalculator::filter_trades_by_days(&closed_trades, days);

        // Ensure every closed trade in window has a grade with the requested weights.
        for trade in &closed_trades {
            let latest = self.trust.latest_trade_grade(trade.id).unwrap_or(None);
            let needs = match latest {
                None => true,
                Some(g) => {
                    g.process_weight_permille != weights.process
                        || g.risk_weight_permille != weights.risk
                        || g.execution_weight_permille != weights.execution
                        || g.documentation_weight_permille != weights.documentation
                }
            };
            if needs {
                let _ = self.trust.grade_trade(trade.id, weights).map_err(|e| {
                    Self::report_error(
                        format,
                        "grade_compute_failed",
                        format!("Failed to compute grade for trade {}: {e}", trade.id),
                    )
                })?;
            }
        }

        let mut per_trade_latest: HashMap<Uuid, model::TradeGrade> = HashMap::new();
        for trade in &closed_trades {
            if let Ok(Some(g)) = self.trust.latest_trade_grade(trade.id) {
                per_trade_latest.insert(trade.id, g);
            }
        }

        let grades: Vec<model::TradeGrade> = per_trade_latest.into_values().collect();
        let count = u32::try_from(grades.len()).map_err(|_| {
            Self::report_error(
                format,
                "grade_count_overflow",
                "Too many grades to summarize safely",
            )
        })?;

        let avg = |xs: Vec<u8>| -> Decimal {
            if xs.is_empty() {
                return dec!(0);
            }
            let sum: u32 = xs.into_iter().map(u32::from).sum();
            Decimal::from(sum)
                .checked_div(Decimal::from(count))
                .unwrap_or(dec!(0))
        };

        let overall_avg = avg(grades.iter().map(|g| g.overall_score).collect());
        let process_avg = avg(grades.iter().map(|g| g.process_score).collect());
        let risk_avg = avg(grades.iter().map(|g| g.risk_score).collect());
        let execution_avg = avg(grades.iter().map(|g| g.execution_score).collect());
        let documentation_avg = avg(grades.iter().map(|g| g.documentation_score).collect());

        let mut dist: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
        for g in &grades {
            let entry = dist.entry(g.overall_grade.to_string()).or_insert(0);
            *entry = entry.saturating_add(1);
        }

        if format == ReportOutputFormat::Json {
            let payload = json!({
                "report": "grade_summary",
                "format_version": 1,
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "scope": { "account_id": account_id.map(|id| id.to_string()) },
                "filters": { "days": days },
                "data": {
                    "trade_count": count,
                    "average_overall_score": Self::decimal_string(overall_avg),
                    "component_averages": {
                        "process": Self::decimal_string(process_avg),
                        "risk": Self::decimal_string(risk_avg),
                        "execution": Self::decimal_string(execution_avg),
                        "documentation": Self::decimal_string(documentation_avg)
                    },
                    "distribution": dist,
                    "weights_permille": {
                        "process": weights.process,
                        "risk": weights.risk,
                        "execution": weights.execution,
                        "documentation": weights.documentation
                    }
                }
            });
            Self::print_json(&payload)?;
            return Ok(());
        }

        println!("Trade Grading Summary (Last {days} days)");
        println!("===================================");
        println!("Average Score: {overall_avg}/100");
        println!("Grade Distribution:");
        for bucket in ["A+", "A", "A-", "B+", "B", "B-", "C+", "C", "C-", "D", "F"] {
            let n = dist.get(bucket).copied().unwrap_or(0);
            if count == 0 {
                println!("{bucket}: {n}");
            } else {
                let pct = Decimal::from(n)
                    .checked_mul(dec!(100))
                    .and_then(|v| v.checked_div(Decimal::from(count)))
                    .unwrap_or(dec!(0));
                println!("{bucket}: {n} ({pct}%)");
            }
        }
        println!();
        println!("Component Averages:");
        println!("Process:        {process_avg}/100");
        println!("Risk:           {risk_avg}/100");
        println!("Execution:      {execution_avg}/100");
        println!("Documentation:  {documentation_avg}/100");

        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn parse_grade_weights(
        matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<core::services::grading::GradingWeightsPermille, CliError> {
        use core::services::grading::GradingWeightsPermille;

        let Some(raw) = matches.get_one::<String>("weights") else {
            return Ok(GradingWeightsPermille::default());
        };

        let parts: Vec<&str> = raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let [process_raw, risk_raw, execution_raw, documentation_raw] = parts.as_slice() else {
            return Err(Self::report_error(
                format,
                "invalid_weights",
                "Weights must have 4 comma-separated numbers",
            ));
        };

        let parse_u16 = |s: &str| -> Result<u16, CliError> {
            s.parse::<u16>().map_err(|_| {
                Self::report_error(format, "invalid_weights", "Weights must be valid integers")
            })
        };

        let a = parse_u16(process_raw)?;
        let b = parse_u16(risk_raw)?;
        let c = parse_u16(execution_raw)?;
        let d = parse_u16(documentation_raw)?;

        let sum = u32::from(a)
            .saturating_add(u32::from(b))
            .saturating_add(u32::from(c))
            .saturating_add(u32::from(d));

        let (process, risk, execution, documentation) = if sum == 100 {
            (
                a.saturating_mul(10),
                b.saturating_mul(10),
                c.saturating_mul(10),
                d.saturating_mul(10),
            )
        } else if sum == 1000 {
            (a, b, c, d)
        } else {
            return Err(Self::report_error(
                format,
                "invalid_weights",
                "Weights must sum to 100 (percent) or 1000 (permille)",
            ));
        };

        let weights = GradingWeightsPermille {
            process,
            risk,
            execution,
            documentation,
        };
        Ok(weights)
    }

    fn metrics_bond(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let format = Self::parse_report_format(sub_matches);
        let input = self.parse_bond_analytics_input(sub_matches, format)?;
        let analytics = self
            .trust
            .calculate_bond_analytics(input.clone())
            .map_err(|error| {
                Self::report_error(
                    format,
                    "bond_analytics_failed",
                    format!("Unable to calculate bond metrics: {error}"),
                )
            })?;

        match format {
            ReportOutputFormat::Text => {
                Self::print_bond_metrics_text(&input, &analytics);
                Ok(())
            }
            ReportOutputFormat::Json => {
                Self::print_json(&Self::bond_metrics_payload(&input, &analytics))
            }
        }
    }

    fn parse_bond_analytics_input(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<core::calculators_fixed_income::BondAnalyticsInput, CliError> {
        let quantity = sub_matches
            .get_one::<i64>("quantity")
            .copied()
            .ok_or_else(|| {
                Self::report_error(format, "missing_argument", "Missing argument --quantity")
            })?;

        let market_price = Self::parse_decimal_arg(sub_matches, "market-price", format)?;
        let symbol = sub_matches
            .get_one::<String>("symbol")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty());
        let broker = sub_matches
            .get_one::<String>("broker")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty());

        match (symbol, broker) {
            (Some(symbol), Some(broker)) => self.parse_bond_input_from_vehicle(
                sub_matches,
                symbol,
                broker,
                market_price,
                quantity,
                format,
            ),
            (Some(_), None) => Err(Self::report_error(
                format,
                "missing_argument",
                "Missing argument --broker for stored bond lookup",
            )),
            (None, Some(_)) => Err(Self::report_error(
                format,
                "missing_argument",
                "Missing argument --symbol for stored bond lookup",
            )),
            (None, None) => Ok(core::calculators_fixed_income::BondAnalyticsInput {
                face_value: Self::parse_decimal_arg(sub_matches, "face-value", format)?,
                market_price,
                annual_coupon_rate_pct: Self::parse_decimal_arg(
                    sub_matches,
                    "coupon-rate",
                    format,
                )?,
                quantity,
                years_to_maturity: Self::parse_decimal_arg(
                    sub_matches,
                    "years-to-maturity",
                    format,
                )?,
                accrued_interest: Self::parse_bond_accrued_interest_input(
                    sub_matches,
                    None,
                    format,
                )?,
            }),
        }
    }

    fn parse_bond_input_from_vehicle(
        &mut self,
        sub_matches: &ArgMatches,
        symbol: &str,
        broker: &str,
        market_price: Decimal,
        quantity: i64,
        format: ReportOutputFormat,
    ) -> Result<core::calculators_fixed_income::BondAnalyticsInput, CliError> {
        let symbol_norm = symbol.to_uppercase();
        let broker_norm = broker.to_lowercase();
        let vehicles = self.trust.search_trading_vehicles().map_err(|error| {
            Self::report_error(format, "bond_vehicle_lookup_failed", format!("{error}"))
        })?;
        let vehicle = vehicles
            .into_iter()
            .find(|vehicle| {
                vehicle.symbol == symbol_norm && vehicle.broker.eq_ignore_ascii_case(&broker_norm)
            })
            .ok_or_else(|| {
                Self::report_error(
                    format,
                    "bond_vehicle_not_found",
                    format!("No trading vehicle found for {broker}:{symbol}"),
                )
            })?;

        if vehicle.category != model::TradingVehicleCategory::Bond {
            return Err(Self::report_error(
                format,
                "bond_vehicle_wrong_category",
                format!(
                    "Trading vehicle {broker}:{symbol} is category {}, expected bond",
                    vehicle.category
                ),
            ));
        }

        let terms = vehicle.fixed_income.as_ref().ok_or_else(|| {
            Self::report_error(
                format,
                "bond_terms_missing",
                format!("Trading vehicle {broker}:{symbol} has no fixed-income terms"),
            )
        })?;

        Ok(core::calculators_fixed_income::BondAnalyticsInput {
            face_value: Self::decimal_arg_or_stored(
                sub_matches,
                "face-value",
                terms.face_value,
                "stored bond face value is missing; pass --face-value",
                format,
            )?,
            market_price,
            annual_coupon_rate_pct: Self::decimal_arg_or_stored(
                sub_matches,
                "coupon-rate",
                terms.annual_coupon_rate_pct,
                "stored bond coupon rate is missing; pass --coupon-rate",
                format,
            )?,
            quantity,
            years_to_maturity: Self::years_to_maturity_arg_or_stored(
                sub_matches,
                terms.maturity_date,
                format,
            )?,
            accrued_interest: Self::parse_bond_accrued_interest_input(
                sub_matches,
                terms.coupon_frequency_per_year,
                format,
            )?,
        })
    }

    fn parse_bond_accrued_interest_input(
        sub_matches: &ArgMatches,
        stored_coupon_frequency: Option<u16>,
        format: ReportOutputFormat,
    ) -> Result<Option<core::calculators_fixed_income::BondAccruedInterestInput>, CliError> {
        if !Self::bond_accrued_interest_args_present(sub_matches) {
            return Ok(None);
        }

        let coupon_frequency = sub_matches
            .get_one::<u16>("coupon-frequency")
            .copied()
            .or(stored_coupon_frequency)
            .ok_or_else(|| {
                Self::report_error(
                    format,
                    "missing_argument",
                    "Missing argument --coupon-frequency for accrued-interest calculations",
                )
            })?;
        if coupon_frequency == 0 {
            return Err(Self::report_error(
                format,
                "invalid_coupon_frequency",
                "--coupon-frequency must be greater than zero",
            ));
        }

        let day_count_basis = match sub_matches.get_one::<String>("day-count") {
            Some(raw) => {
                core::calculators_fixed_income::DayCountBasis::from_str(raw).map_err(|_| {
                    Self::report_error(
                        format,
                        "invalid_day_count",
                        format!("Invalid --day-count value: {raw}"),
                    )
                })?
            }
            None => core::calculators_fixed_income::DayCountBasis::ActualActual,
        };

        Ok(Some(
            core::calculators_fixed_income::BondAccruedInterestInput {
                settlement_date: Self::parse_required_bond_date_arg(
                    sub_matches,
                    "settlement-date",
                    format,
                )?,
                last_coupon_date: Self::parse_required_bond_date_arg(
                    sub_matches,
                    "last-coupon-date",
                    format,
                )?,
                next_coupon_date: Self::parse_required_bond_date_arg(
                    sub_matches,
                    "next-coupon-date",
                    format,
                )?,
                coupon_frequency_per_year: coupon_frequency,
                day_count_basis,
            },
        ))
    }

    fn bond_accrued_interest_args_present(sub_matches: &ArgMatches) -> bool {
        sub_matches.get_one::<u16>("coupon-frequency").is_some()
            || sub_matches.get_one::<String>("settlement-date").is_some()
            || sub_matches.get_one::<String>("last-coupon-date").is_some()
            || sub_matches.get_one::<String>("next-coupon-date").is_some()
            || sub_matches.get_one::<String>("day-count").is_some()
    }

    fn parse_required_bond_date_arg(
        sub_matches: &ArgMatches,
        key: &'static str,
        format: ReportOutputFormat,
    ) -> Result<NaiveDate, CliError> {
        let raw = sub_matches.get_one::<String>(key).ok_or_else(|| {
            Self::report_error(
                format,
                "missing_argument",
                format!("Missing argument --{key}"),
            )
        })?;
        NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| {
            Self::report_error(
                format,
                "invalid_date",
                format!("Invalid --{key} value (expected YYYY-MM-DD): {raw}"),
            )
        })
    }

    fn decimal_arg_or_stored(
        sub_matches: &ArgMatches,
        key: &'static str,
        stored: Option<Decimal>,
        missing_message: &'static str,
        format: ReportOutputFormat,
    ) -> Result<Decimal, CliError> {
        match Self::parse_optional_decimal_arg(sub_matches, key, format)? {
            Some(value) => Ok(value),
            None => stored
                .ok_or_else(|| Self::report_error(format, "bond_terms_missing", missing_message)),
        }
    }

    fn years_to_maturity_arg_or_stored(
        sub_matches: &ArgMatches,
        maturity_date: Option<NaiveDate>,
        format: ReportOutputFormat,
    ) -> Result<Decimal, CliError> {
        if let Some(value) =
            Self::parse_optional_decimal_arg(sub_matches, "years-to-maturity", format)?
        {
            return Ok(value);
        }

        let maturity_date = maturity_date.ok_or_else(|| {
            Self::report_error(
                format,
                "bond_terms_missing",
                "stored bond maturity date is missing; pass --years-to-maturity",
            )
        })?;
        Self::years_to_maturity(maturity_date, Utc::now().date_naive(), format)
    }

    fn years_to_maturity(
        maturity_date: NaiveDate,
        as_of_date: NaiveDate,
        _format: ReportOutputFormat,
    ) -> Result<Decimal, CliError> {
        let days = maturity_date.signed_duration_since(as_of_date).num_days();
        if days <= 0 {
            return Ok(Decimal::ZERO);
        }
        Ok(Decimal::from(days)
            .checked_div(dec!(365))
            .unwrap_or(dec!(0)))
    }

    fn print_bond_metrics_text(
        input: &core::calculators_fixed_income::BondAnalyticsInput,
        analytics: &core::calculators_fixed_income::BondAnalytics,
    ) {
        println!("Bond Metrics");
        println!("============");
        println!(
            "face_value: {}",
            crate::zen::amount_share(input.face_value, input.face_value)
        );
        println!(
            "market_price: {}",
            crate::zen::amount_share(input.market_price, input.face_value)
        );
        println!(
            "coupon_rate: {}%",
            Self::decimal_string(input.annual_coupon_rate_pct)
        );
        println!("quantity: {}", input.quantity);
        println!(
            "position_face_value: {}",
            crate::zen::amount_share(analytics.position_face_value, analytics.position_face_value)
        );
        println!(
            "position_market_value: {}",
            crate::zen::amount_share(
                analytics.position_market_value,
                analytics.position_face_value,
            )
        );
        println!(
            "accrued_interest: {}",
            crate::zen::amount_share(
                analytics.accrued_interest_total,
                analytics.position_face_value,
            )
        );
        println!(
            "dirty_price: {}",
            crate::zen::amount_share(analytics.dirty_price, input.face_value)
        );
        println!(
            "position_dirty_value: {}",
            crate::zen::amount_share(
                analytics.position_dirty_value,
                analytics.position_face_value
            )
        );
        println!(
            "annual_coupon_income: {}",
            crate::zen::amount_share(
                analytics.annual_coupon_income,
                analytics.position_face_value
            )
        );
        println!(
            "current_yield: {}%",
            Self::decimal_string(analytics.current_yield_pct)
        );
        println!(
            "approximate_ytm: {}",
            analytics
                .approximate_yield_to_maturity_pct
                .map(|value| format!("{}%", Self::decimal_string(value)))
                .unwrap_or_else(|| "n/a".to_string())
        );
        println!(
            "premium_discount: {} ({}%)",
            crate::zen::amount_share(analytics.price_premium_discount, input.face_value),
            Self::decimal_string(analytics.price_premium_discount_pct)
        );
    }

    fn bond_metrics_payload(
        input: &core::calculators_fixed_income::BondAnalyticsInput,
        analytics: &core::calculators_fixed_income::BondAnalytics,
    ) -> Value {
        json!({
            "report": "bond_metrics",
            "input": {
                "face_value": Self::decimal_string(input.face_value),
                "market_price": Self::decimal_string(input.market_price),
                "annual_coupon_rate_pct": Self::decimal_string(input.annual_coupon_rate_pct),
                "quantity": input.quantity,
                "years_to_maturity": Self::decimal_string(input.years_to_maturity),
                "accrued_interest": input.accrued_interest.as_ref().map(|accrual| json!({
                    "settlement_date": accrual.settlement_date.to_string(),
                    "last_coupon_date": accrual.last_coupon_date.to_string(),
                    "next_coupon_date": accrual.next_coupon_date.to_string(),
                    "coupon_frequency_per_year": accrual.coupon_frequency_per_year,
                    "day_count_basis": accrual.day_count_basis.to_string(),
                })),
            },
            "data": {
                "position_face_value": Self::decimal_string(analytics.position_face_value),
                "position_market_value": Self::decimal_string(analytics.position_market_value),
                "annual_coupon_per_unit": Self::decimal_string(analytics.annual_coupon_per_unit),
                "annual_coupon_income": Self::decimal_string(analytics.annual_coupon_income),
                "current_yield_pct": Self::decimal_string(analytics.current_yield_pct),
                "approximate_yield_to_maturity_pct": analytics.approximate_yield_to_maturity_pct.map(Self::decimal_string),
                "price_premium_discount": Self::decimal_string(analytics.price_premium_discount),
                "price_premium_discount_pct": Self::decimal_string(analytics.price_premium_discount_pct),
                "accrued_interest_per_unit": Self::decimal_string(analytics.accrued_interest_per_unit),
                "accrued_interest_total": Self::decimal_string(analytics.accrued_interest_total),
                "dirty_price": Self::decimal_string(analytics.dirty_price),
                "position_dirty_value": Self::decimal_string(analytics.position_dirty_value),
            }
        })
    }

    fn metrics_advanced(&mut self, sub_matches: &ArgMatches) {
        use crate::views::{AdvancedMetricsDisplayContext, AdvancedMetricsView};
        use core::calculators_performance::PerformanceCalculator;
        use rust_decimal::Decimal;
        use rust_decimal_macros::dec;
        use std::str::FromStr;
        use uuid::Uuid;

        // Get account ID if provided
        let account_id = if let Some(account_arg) = sub_matches.get_one::<String>("account") {
            match Uuid::from_str(account_arg) {
                Ok(id) => Some(id),
                Err(_) => {
                    eprintln!("Error: Invalid account ID format");
                    return;
                }
            }
        } else {
            None
        };

        // Get days filter if provided (default to 90)
        let days_filter = sub_matches.get_one::<u32>("days").copied().unwrap_or(90);

        // Get risk-free rate if provided (default to 5%)
        let _risk_free_rate = if let Some(rate) = sub_matches.get_one::<f64>("risk-free-rate") {
            Decimal::try_from(*rate).unwrap_or(dec!(0.05))
        } else {
            dec!(0.05)
        };

        // Display period information
        if days_filter > 0 {
            println!("Advanced Trading Metrics (Last {days_filter} days)");
        } else {
            println!("Advanced Trading Metrics (All time)");
        }
        println!("======================================");

        // Get all closed trades
        let mut all_trades = match self.trust.search_closed_trades(account_id) {
            Ok(trades) => trades,
            Err(_) => {
                eprintln!("Unable to retrieve trading data. Please check your account settings and try again.");
                return;
            }
        };

        // Apply days filter if specified
        if days_filter > 0 {
            all_trades = PerformanceCalculator::filter_trades_by_days(&all_trades, days_filter);
        }

        if all_trades.is_empty() {
            println!("No trades found for the specified criteria.");
            return;
        }

        // Handle export if requested
        if let Some(export_format) = sub_matches.get_one::<String>("export") {
            let output_file = sub_matches
                .get_one::<String>("output")
                .cloned()
                .unwrap_or_else(|| format!("metrics.{export_format}"));

            match self.export_metrics(&all_trades, export_format, &output_file, _risk_free_rate) {
                Ok(()) => {
                    println!("Metrics exported to: {output_file}");
                    return;
                }
                Err(e) => {
                    eprintln!("Export failed: {e}");
                    // Continue with normal display
                }
            }
        }

        AdvancedMetricsView::display(
            all_trades,
            Some(AdvancedMetricsDisplayContext {
                calmar_ratio: None,
                risk_free_rate: Some(_risk_free_rate),
            }),
        );
    }

    fn export_metrics(
        &self,
        trades: &[Trade],
        format: &str,
        output_file: &str,
        risk_free_rate: Decimal,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::exporters::MetricsExporter;
        use std::fs::File;
        use std::io::Write;

        let content = match format {
            "json" => {
                let json = MetricsExporter::to_json(trades, Some(risk_free_rate));
                serde_json::to_string_pretty(&json)?
            }
            "csv" => MetricsExporter::to_csv(trades, Some(risk_free_rate)),
            _ => return Err("Unsupported export format".into()),
        };

        let mut file = File::create(output_file)?;
        file.write_all(content.as_bytes())?;

        Ok(())
    }

    #[allow(clippy::too_many_lines, clippy::indexing_slicing)]
    fn metrics_compare(&mut self, sub_matches: &ArgMatches) {
        use crate::exporters::MetricsExporter;
        use core::calculators_advanced_metrics::AdvancedMetricsCalculator;
        use serde_json::json;
        use std::fs::File;
        use std::io::Write;

        let period1 = sub_matches.get_one::<String>("period1").unwrap();
        let period2 = sub_matches.get_one::<String>("period2").unwrap();
        let output_format = sub_matches
            .get_one::<String>("format")
            .map(String::as_str)
            .unwrap_or("text");

        let account_id = match sub_matches.get_one::<String>("account") {
            Some(id) => match Uuid::parse_str(id) {
                Ok(v) => Some(v),
                Err(_) => {
                    eprintln!("Error: Invalid account ID format");
                    return;
                }
            },
            None => None,
        };

        let now = Utc::now();
        let window1 = match Self::parse_period_window(period1, now) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("Error: {err}");
                return;
            }
        };
        let window2 = match Self::parse_period_window(period2, now) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("Error: {err}");
                return;
            }
        };

        let all_trades = match self.trust.search_closed_trades(account_id) {
            Ok(v) => v,
            Err(err) => {
                eprintln!("Error loading trades: {err}");
                return;
            }
        };

        let p1: Vec<Trade> = all_trades
            .iter()
            .filter(|t| Self::trade_in_window(t, window1.0, window1.1))
            .cloned()
            .collect();
        let p2: Vec<Trade> = all_trades
            .iter()
            .filter(|t| Self::trade_in_window(t, window2.0, window2.1))
            .cloned()
            .collect();

        let make_metrics = |trades: &[Trade]| {
            let total_return = AdvancedMetricsCalculator::calculate_net_profit(trades);
            let sharpe = AdvancedMetricsCalculator::calculate_sharpe_ratio(trades, dec!(0.05));
            let profit_factor = AdvancedMetricsCalculator::calculate_profit_factor(trades);
            let win_rate = AdvancedMetricsCalculator::calculate_win_rate(trades);
            let expectancy = AdvancedMetricsCalculator::calculate_expectancy(trades);
            let max_dd = Self::max_drawdown_amount(trades);
            json!({
                "trade_count": trades.len(),
                "total_return": Self::decimal_string(total_return),
                "sharpe_ratio": sharpe.map(Self::decimal_string),
                "profit_factor": profit_factor.map(Self::decimal_string),
                "win_rate": Self::decimal_string(win_rate),
                "expectancy": Self::decimal_string(expectancy),
                "max_drawdown": Self::decimal_string(max_dd),
            })
        };

        let p1m = make_metrics(&p1);
        let p2m = make_metrics(&p2);

        let to_decimal = |value: &Value| -> Decimal {
            value
                .as_str()
                .and_then(|s| Decimal::from_str_exact(s).ok())
                .unwrap_or(Decimal::ZERO)
        };
        let delta = |k: &str| -> String {
            let v1 = to_decimal(&p1m[k]);
            let v2 = to_decimal(&p2m[k]);
            Self::decimal_string(v1.checked_sub(v2).unwrap_or(Decimal::ZERO))
        };

        let payload = json!({
            "report": "metrics_compare",
            "period1": period1,
            "period2": period2,
            "window1": {
                "from": window1.0.map(|d| d.to_string()),
                "to": window1.1.map(|d| d.to_string()),
            },
            "window2": {
                "from": window2.0.map(|d| d.to_string()),
                "to": window2.1.map(|d| d.to_string()),
            },
            "period1_metrics": p1m,
            "period2_metrics": p2m,
            "delta": {
                "total_return": delta("total_return"),
                "win_rate": delta("win_rate"),
                "expectancy": delta("expectancy"),
                "max_drawdown": delta("max_drawdown"),
            }
        });

        if let Some(export_format) = sub_matches.get_one::<String>("export") {
            let output_file = sub_matches
                .get_one::<String>("output")
                .cloned()
                .unwrap_or_else(|| format!("metrics-compare.{export_format}"));
            let content = match export_format.as_str() {
                "json" => {
                    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
                }
                "csv" => {
                    let mut rows = vec![
                        "metric,period1,period2,delta".to_string(),
                        format!("trade_count,{},,", p1m["trade_count"].as_u64().unwrap_or(0)),
                    ];
                    for key in ["total_return", "win_rate", "expectancy", "max_drawdown"] {
                        rows.push(format!(
                            "{},{},{},{}",
                            key,
                            p1m[key].as_str().unwrap_or(""),
                            p2m[key].as_str().unwrap_or(""),
                            payload["delta"][key].as_str().unwrap_or("")
                        ));
                    }
                    rows.join("\n")
                }
                _ => MetricsExporter::to_csv(&[], None),
            };
            if let Ok(mut file) = File::create(&output_file) {
                if file.write_all(content.as_bytes()).is_ok() {
                    println!("Metrics comparison exported to: {output_file}");
                } else {
                    eprintln!("Failed to write export file: {output_file}");
                }
            } else {
                eprintln!("Failed to create export file: {output_file}");
            }
        }

        if output_format == "json" {
            println!("{payload}");
            return;
        }

        println!("Performance Comparison");
        println!("=====================");
        println!("Period 1 ({period1}) trades: {}", p1.len());
        println!("Period 2 ({period2}) trades: {}", p2.len());
        println!(
            "Total Return: {} vs {} (delta {})",
            p1m["total_return"].as_str().unwrap_or("0"),
            p2m["total_return"].as_str().unwrap_or("0"),
            payload["delta"]["total_return"].as_str().unwrap_or("0")
        );
        println!(
            "Win Rate: {} vs {} (delta {})",
            p1m["win_rate"].as_str().unwrap_or("0"),
            p2m["win_rate"].as_str().unwrap_or("0"),
            payload["delta"]["win_rate"].as_str().unwrap_or("0")
        );
        println!(
            "Expectancy: {} vs {} (delta {})",
            p1m["expectancy"].as_str().unwrap_or("0"),
            p2m["expectancy"].as_str().unwrap_or("0"),
            payload["delta"]["expectancy"].as_str().unwrap_or("0")
        );
        println!(
            "Max Drawdown: {} vs {} (delta {})",
            p1m["max_drawdown"].as_str().unwrap_or("0"),
            p2m["max_drawdown"].as_str().unwrap_or("0"),
            payload["delta"]["max_drawdown"].as_str().unwrap_or("0")
        );
    }

    fn parse_period_window(
        period: &str,
        now: DateTime<Utc>,
    ) -> Result<(Option<NaiveDate>, Option<NaiveDate>), String> {
        let normalized = period.trim().to_ascii_lowercase();
        if let Some(days) = normalized
            .strip_prefix("last-")
            .and_then(|s| s.strip_suffix("-days"))
            .and_then(|s| s.parse::<u64>().ok())
        {
            let end = now.date_naive();
            let start = end
                .checked_sub_days(Days::new(days.saturating_sub(1)))
                .ok_or("Invalid last-N-days window")?;
            return Ok((Some(start), Some(end)));
        }
        if let Some(days) = normalized
            .strip_prefix("previous-")
            .and_then(|s| s.strip_suffix("-days"))
            .and_then(|s| s.parse::<u64>().ok())
        {
            let end = now
                .date_naive()
                .checked_sub_days(Days::new(days))
                .ok_or("Invalid previous-N-days window")?;
            let start = end
                .checked_sub_days(Days::new(days.saturating_sub(1)))
                .ok_or("Invalid previous-N-days window")?;
            return Ok((Some(start), Some(end)));
        }
        if let Some((from, to)) = normalized.split_once("..") {
            let from = NaiveDate::parse_from_str(from, "%Y-%m-%d")
                .map_err(|_| "Invalid period start date (expected YYYY-MM-DD)")?;
            let to = NaiveDate::parse_from_str(to, "%Y-%m-%d")
                .map_err(|_| "Invalid period end date (expected YYYY-MM-DD)")?;
            if from > to {
                return Err("Invalid period range: start date is after end date".to_string());
            }
            return Ok((Some(from), Some(to)));
        }
        if normalized == "all" {
            return Ok((None, None));
        }
        Err("Invalid period format. Use last-N-days, previous-N-days, YYYY-MM-DD..YYYY-MM-DD, or all".to_string())
    }

    fn trade_in_window(trade: &Trade, from: Option<NaiveDate>, to: Option<NaiveDate>) -> bool {
        let day = trade.updated_at.date();
        if let Some(f) = from {
            if day < f {
                return false;
            }
        }
        if let Some(t) = to {
            if day > t {
                return false;
            }
        }
        true
    }

    fn max_drawdown_amount(trades: &[Trade]) -> Decimal {
        let mut running = Decimal::ZERO;
        let mut peak = Decimal::ZERO;
        let mut max_drawdown = Decimal::ZERO;

        for trade in trades {
            running = running
                .checked_add(trade.balance.total_performance)
                .unwrap_or(running);
            if running > peak {
                peak = running;
            }
            let drawdown = peak.checked_sub(running).unwrap_or(Decimal::ZERO);
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
            }
        }
        max_drawdown
    }

    fn session_open(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
            Self::report_error(
                format,
                "missing_argument",
                "Missing --account for session open",
            )
        })?;
        let account = self.resolve_account_arg(account_raw, format)?;
        if self
            .trust
            .open_session_for_account(account.id)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "session_lookup_failed",
                    format!("Failed to read open session: {error}"),
                )
            })?
            .is_some()
        {
            return Err(Self::report_error(
                format,
                "session_already_open",
                format!("Account '{}' already has an open session", account.name),
            ));
        }

        let suggested = self.suggest_session_regime(&account);
        let mut io = ConsoleDialogIo::default();
        let plan = Self::session_plan_from_dialog(&mut io, &account, suggested, format)?;
        let saved = self.trust.create_session_plan(plan).map_err(|error| {
            Self::report_error(
                format,
                "session_open_failed",
                format!("Failed to open session: {error}"),
            )
        })?;

        Self::print_session_open_summary(&saved, &account, format)
    }

    fn session_close(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let (account, plan) = self.open_session_target(sub_matches, format)?;
        let closed_at = Utc::now().naive_utc();
        let actual_regime = self.suggest_session_regime(&account);
        let trades = self
            .trust
            .trades_for_account_in_period(account.id, plan.opened_at, closed_at)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "session_trade_lookup_failed",
                    format!("Failed to load session trades: {error}"),
                )
            })?;
        let review_rows = Self::session_trade_review_rows(&plan, &trades);

        Self::print_session_close_comparison(&plan, &account, actual_regime, &review_rows, format)?;

        let mut io = ConsoleDialogIo::default();
        let grade = Self::select_session_grade(&mut io, format)?;
        let notes = Self::read_optional_session_text(&mut io, "Adherence notes", format)?;
        let close = SessionPlanClose {
            session_plan_id: plan.id,
            closed_at,
            session_grade: Some(grade),
            adherence_notes: notes,
        };
        let closed = self.trust.close_session_plan(close).map_err(|error| {
            Self::report_error(
                format,
                "session_close_failed",
                format!("Failed to close session: {error}"),
            )
        })?;

        Self::print_session_closed_summary(&closed, &account, &review_rows, format)
    }

    fn session_list(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
            Self::report_error(
                format,
                "missing_argument",
                "Missing --account for session list",
            )
        })?;
        let account = self.resolve_account_arg(account_raw, format)?;
        let sessions = self
            .trust
            .session_plans_for_account(
                account.id,
                chrono::NaiveDateTime::MIN,
                Utc::now().naive_utc(),
            )
            .map_err(|error| {
                Self::report_error(
                    format,
                    "session_list_failed",
                    format!("Failed to list sessions: {error}"),
                )
            })?;

        if format == ReportOutputFormat::Json {
            let payload = json!({
                "account": {
                    "id": account.id.to_string(),
                    "name": account.name,
                },
                "sessions": sessions.iter().map(Self::session_plan_payload).collect::<Vec<_>>(),
            });
            return Self::print_json(&payload);
        }

        println!("Session history for {} ({})", account.name, account.id);
        if sessions.is_empty() {
            println!("No sessions found.");
            return Ok(());
        }
        println!(
            "{:<19} {:<19} {:<9} {:<5} {:<8} setups",
            "opened_at", "closed_at", "regime", "max", "grade"
        );
        for session in sessions {
            println!(
                "{:<19} {:<19} {:<9} {:<5} {:<8} {}",
                session.opened_at,
                session
                    .closed_at
                    .map(|closed_at| closed_at.to_string())
                    .unwrap_or_else(|| "open".to_string()),
                session.regime,
                session.max_positions,
                session.session_grade.as_deref().unwrap_or("-"),
                session.permitted_setups.join(", ")
            );
        }
        Ok(())
    }

    fn open_session_target(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(Account, SessionPlan), CliError> {
        if let Some(account_raw) = sub_matches.get_one::<String>("account") {
            let account = self.resolve_account_arg(account_raw, format)?;
            let plan = self
                .trust
                .open_session_for_account(account.id)
                .map_err(|error| {
                    Self::report_error(
                        format,
                        "session_lookup_failed",
                        format!("Failed to read open session: {error}"),
                    )
                })?
                .ok_or_else(|| {
                    Self::report_error(
                        format,
                        "no_open_session",
                        format!("No open session for account '{}'", account.name),
                    )
                })?;
            return Ok((account, plan));
        }

        let accounts = self.trust.search_all_accounts().map_err(|error| {
            Self::report_error(
                format,
                "accounts_unavailable",
                format!("Failed to load accounts: {error}"),
            )
        })?;
        let mut open = Vec::new();
        for account in accounts {
            if let Some(plan) =
                self.trust
                    .open_session_for_account(account.id)
                    .map_err(|error| {
                        Self::report_error(
                            format,
                            "session_lookup_failed",
                            format!("Failed to read open session: {error}"),
                        )
                    })?
            {
                open.push((account, plan));
            }
        }

        match open.len() {
            0 => Err(Self::report_error(
                format,
                "no_open_session",
                "No open session found",
            )),
            1 => open.into_iter().next().ok_or_else(|| {
                Self::report_error(format, "no_open_session", "No open session found")
            }),
            _ => Err(Self::report_error(
                format,
                "ambiguous_open_session",
                "More than one session is open; pass --account",
            )),
        }
    }

    fn session_plan_from_dialog(
        io: &mut dyn DialogIo,
        account: &Account,
        suggested_regime: Option<SessionRegime>,
        format: ReportOutputFormat,
    ) -> Result<SessionPlan, CliError> {
        let regime = Self::select_session_regime(io, suggested_regime, format)?;
        let permitted_setups = Self::read_session_setups(io, format)?;
        let max_positions = Self::read_session_max_positions(io, format)?;
        let hypothesis = Self::read_required_session_text(io, "Hypothesis", 500, format)?;
        let success_criteria =
            Self::read_required_session_text(io, "Success criteria", 500, format)?;
        let failure_criteria =
            Self::read_required_session_text(io, "Failure criteria", 500, format)?;
        let now = Utc::now().naive_utc();

        Ok(SessionPlan {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: account.id,
            opened_at: now,
            closed_at: None,
            regime,
            permitted_setups,
            max_positions,
            hypothesis,
            success_criteria,
            failure_criteria,
            session_grade: None,
            adherence_notes: None,
        })
    }

    fn select_session_regime(
        io: &mut dyn DialogIo,
        suggested: Option<SessionRegime>,
        format: ReportOutputFormat,
    ) -> Result<SessionRegime, CliError> {
        let regimes = [
            SessionRegime::Calm,
            SessionRegime::Normal,
            SessionRegime::Elevated,
        ];
        let labels = regimes
            .iter()
            .map(|regime| {
                let label = Self::session_regime_label(*regime);
                if Some(*regime) == suggested {
                    format!("{label} (suggested)")
                } else {
                    label.to_string()
                }
            })
            .collect::<Vec<_>>();
        let default = regimes
            .iter()
            .position(|regime| Some(*regime) == suggested)
            .unwrap_or(1);
        let selected = io
            .select_index("Regime today?", &labels, default)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "session_dialog_failed",
                    format!("Failed to select regime: {error}"),
                )
            })?
            .ok_or_else(|| {
                Self::report_error(format, "session_dialog_cancelled", "Session open cancelled")
            })?;
        regimes.get(selected).copied().ok_or_else(|| {
            Self::report_error(format, "invalid_session_regime", "Invalid regime selection")
        })
    }

    fn read_session_setups(
        io: &mut dyn DialogIo,
        format: ReportOutputFormat,
    ) -> Result<Vec<String>, CliError> {
        let raw = Self::read_required_session_text(
            io,
            "Permitted setups (comma-separated)",
            500,
            format,
        )?;
        model::parse_session_setups(&raw).map_err(|error| {
            Self::report_error(
                format,
                "invalid_session_setups",
                format!("Invalid permitted setups: {error:?}"),
            )
        })
    }

    fn read_session_max_positions(
        io: &mut dyn DialogIo,
        format: ReportOutputFormat,
    ) -> Result<i32, CliError> {
        let raw = Self::read_required_session_text(io, "Max new positions", 32, format)?;
        let max_positions = raw.trim().parse::<i32>().map_err(|_| {
            Self::report_error(
                format,
                "invalid_session_max_positions",
                "Max new positions must be an integer",
            )
        })?;
        if max_positions < 0 {
            return Err(Self::report_error(
                format,
                "invalid_session_max_positions",
                "Max new positions cannot be negative",
            ));
        }
        Ok(max_positions)
    }

    fn select_session_grade(
        io: &mut dyn DialogIo,
        format: ReportOutputFormat,
    ) -> Result<String, CliError> {
        let labels = ["A", "B", "C", "D", "F"]
            .iter()
            .map(|grade| (*grade).to_string())
            .collect::<Vec<_>>();
        let selected = io
            .select_index("Session grade", &labels, 2)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "session_dialog_failed",
                    format!("Failed to select grade: {error}"),
                )
            })?
            .ok_or_else(|| {
                Self::report_error(
                    format,
                    "session_dialog_cancelled",
                    "Session close cancelled",
                )
            })?;
        labels.get(selected).cloned().ok_or_else(|| {
            Self::report_error(
                format,
                "invalid_session_grade",
                "Invalid session grade selection",
            )
        })
    }

    fn read_required_session_text(
        io: &mut dyn DialogIo,
        prompt: &str,
        max_chars: usize,
        format: ReportOutputFormat,
    ) -> Result<String, CliError> {
        let value = io.input_text(prompt, false).map_err(|error| {
            Self::report_error(
                format,
                "session_dialog_failed",
                format!("Failed to read {prompt}: {error}"),
            )
        })?;
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(Self::report_error(
                format,
                "invalid_session_text",
                format!("{prompt} cannot be blank"),
            ));
        }
        if trimmed.chars().count() > max_chars {
            return Err(Self::report_error(
                format,
                "invalid_session_text",
                format!("{prompt} cannot exceed {max_chars} characters"),
            ));
        }
        Ok(trimmed.to_string())
    }

    fn read_optional_session_text(
        io: &mut dyn DialogIo,
        prompt: &str,
        format: ReportOutputFormat,
    ) -> Result<Option<String>, CliError> {
        let value = io.input_text(prompt, true).map_err(|error| {
            Self::report_error(
                format,
                "session_dialog_failed",
                format!("Failed to read {prompt}: {error}"),
            )
        })?;
        let trimmed = value.trim();
        if trimmed.is_empty() {
            Ok(None)
        } else {
            Ok(Some(trimmed.to_string()))
        }
    }

    fn suggest_session_regime(&mut self, account: &Account) -> Option<SessionRegime> {
        self.trust
            .regime_advisory(&RegimeRequest::default(), account, RegimeConfig::default())
            .ok()
            .map(|snapshot| match snapshot.composite {
                advisor::CompositeRegime::CalmTrending => SessionRegime::Calm,
                advisor::CompositeRegime::NormalTrending => SessionRegime::Normal,
                advisor::CompositeRegime::ElevatedTrending
                | advisor::CompositeRegime::Choppy
                | advisor::CompositeRegime::ElevatedChoppy
                | advisor::CompositeRegime::NarrowBreadth => SessionRegime::Elevated,
            })
    }

    fn session_trade_review_rows(
        plan: &SessionPlan,
        trades: &[Trade],
    ) -> Vec<SessionTradeReviewRow> {
        trades
            .iter()
            .map(|trade| {
                let setup = Self::trade_setup_type(trade);
                let planned = setup
                    .as_deref()
                    .is_some_and(|value| Self::setup_is_permitted(&plan.permitted_setups, value));
                SessionTradeReviewRow {
                    trade_id: trade.id,
                    symbol: trade.trading_vehicle.symbol.clone(),
                    setup,
                    planned,
                }
            })
            .collect()
    }

    fn trade_setup_type(trade: &Trade) -> Option<String> {
        trade
            .context
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }

    fn setup_is_permitted(permitted_setups: &[String], setup: &str) -> bool {
        permitted_setups
            .iter()
            .any(|permitted| permitted.eq_ignore_ascii_case(setup.trim()))
    }

    fn session_regime_label(regime: SessionRegime) -> &'static str {
        match regime {
            SessionRegime::Calm => "Calm",
            SessionRegime::Normal => "Normal",
            SessionRegime::Elevated => "Elevated",
        }
    }

    fn print_session_open_summary(
        plan: &SessionPlan,
        account: &Account,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        if format == ReportOutputFormat::Json {
            return Self::print_json(&json!({
                "session": Self::session_plan_payload(plan),
                "account": {
                    "id": account.id.to_string(),
                    "name": account.name,
                },
            }));
        }

        println!("Session opened:");
        println!("  account: {} ({})", account.name, account.id);
        println!("  session_id: {}", plan.id);
        println!("  opened_at: {}", plan.opened_at);
        println!("  regime: {}", plan.regime);
        println!("  permitted_setups: {}", plan.permitted_setups.join(", "));
        println!("  max_new_positions: {}", plan.max_positions);
        Ok(())
    }

    fn print_session_close_comparison(
        plan: &SessionPlan,
        account: &Account,
        actual_regime: Option<SessionRegime>,
        review_rows: &[SessionTradeReviewRow],
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        if format == ReportOutputFormat::Json {
            return Self::print_json(&json!({
                "comparison": Self::session_comparison_payload(plan, account, actual_regime, review_rows),
            }));
        }

        println!("Session review:");
        println!("  account: {} ({})", account.name, account.id);
        println!("  opened_at: {}", plan.opened_at);
        println!(
            "  regime: planned {} | actual {}",
            plan.regime,
            actual_regime
                .map(|regime| regime.to_string())
                .unwrap_or_else(|| "unavailable".to_string())
        );
        println!(
            "  positions: planned max {} | actual {}",
            plan.max_positions,
            review_rows.len()
        );
        println!("Trades:");
        if review_rows.is_empty() {
            println!("  none");
        } else {
            for row in review_rows {
                let marker = if row.planned { "planned" } else { "UNPLANNED" };
                println!(
                    "  {} {} setup={} {}",
                    row.trade_id,
                    row.symbol,
                    row.setup.as_deref().unwrap_or("none"),
                    marker
                );
            }
        }
        Ok(())
    }

    fn print_session_closed_summary(
        plan: &SessionPlan,
        account: &Account,
        review_rows: &[SessionTradeReviewRow],
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        if format == ReportOutputFormat::Json {
            return Self::print_json(&json!({
                "closed_session": Self::session_plan_payload(plan),
                "comparison": Self::session_comparison_payload(plan, account, None, review_rows),
            }));
        }

        println!("Session closed:");
        println!("  session_id: {}", plan.id);
        println!("  closed_at: {}", plan.closed_at.unwrap_or(plan.updated_at));
        println!("  grade: {}", plan.session_grade.as_deref().unwrap_or("-"));
        println!(
            "  unplanned_trades: {}",
            review_rows.iter().filter(|row| !row.planned).count()
        );
        Ok(())
    }

    fn session_plan_payload(plan: &SessionPlan) -> Value {
        json!({
            "id": plan.id.to_string(),
            "created_at": plan.created_at.to_string(),
            "updated_at": plan.updated_at.to_string(),
            "account_id": plan.account_id.to_string(),
            "opened_at": plan.opened_at.to_string(),
            "closed_at": plan.closed_at.map(|value| value.to_string()),
            "regime": plan.regime.to_string(),
            "permitted_setups": &plan.permitted_setups,
            "max_positions": plan.max_positions,
            "hypothesis": &plan.hypothesis,
            "success_criteria": &plan.success_criteria,
            "failure_criteria": &plan.failure_criteria,
            "session_grade": &plan.session_grade,
            "adherence_notes": &plan.adherence_notes,
        })
    }

    fn session_comparison_payload(
        plan: &SessionPlan,
        account: &Account,
        actual_regime: Option<SessionRegime>,
        review_rows: &[SessionTradeReviewRow],
    ) -> Value {
        json!({
            "account": {
                "id": account.id.to_string(),
                "name": account.name,
            },
            "session_id": plan.id.to_string(),
            "planned_regime": plan.regime.to_string(),
            "actual_regime": actual_regime.map(|regime| regime.to_string()),
            "planned_max_positions": plan.max_positions,
            "actual_positions": review_rows.len(),
            "trades": review_rows.iter().map(|row| {
                json!({
                    "trade_id": row.trade_id.to_string(),
                    "symbol": &row.symbol,
                    "setup": &row.setup,
                    "planned": row.planned,
                    "unplanned": !row.planned,
                })
            }).collect::<Vec<_>>(),
        })
    }

    fn advisor_configure(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let provider_mode = advisor_provider_config_requested(sub_matches);
        let threshold_mode = advisor_threshold_config_requested(sub_matches);
        if provider_mode && threshold_mode {
            return Err(CliError::new(
                "advisor_configure_mode_conflict",
                "Advisor provider keys and concentration thresholds must be configured separately",
            ));
        }
        if provider_mode {
            return Self::advisor_provider_configure(sub_matches);
        }
        self.advisor_thresholds_configure(sub_matches)
    }

    fn advisor_thresholds_configure(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let account_id = Self::parse_uuid_arg(sub_matches, "account", ReportOutputFormat::Text)?;
        let sector_limit =
            Self::parse_decimal_arg(sub_matches, "sector-limit", ReportOutputFormat::Text)?;
        let asset_class_limit =
            Self::parse_decimal_arg(sub_matches, "asset-class-limit", ReportOutputFormat::Text)?;
        let single_position_limit = Self::parse_decimal_arg(
            sub_matches,
            "single-position-limit",
            ReportOutputFormat::Text,
        )?;
        self.ensure_protected_keyword(sub_matches, ReportOutputFormat::Text, "advisor configure")?;

        self.trust
            .configure_advisory_thresholds(
                account_id,
                AdvisoryThresholds {
                    sector_limit_pct: sector_limit,
                    asset_class_limit_pct: asset_class_limit,
                    single_position_limit_pct: single_position_limit,
                },
            )
            .map_err(|e| CliError::new("advisor_configure_failed", e.to_string()))?;
        println!("Advisor thresholds configured for account {account_id}");
        Ok(())
    }

    fn advisor_provider_configure(sub_matches: &ArgMatches) -> Result<(), CliError> {
        let update = advisor_config_update_from_matches(sub_matches)?;
        if !update.is_empty() {
            update
                .apply()
                .map_err(|e| CliError::new("advisor_configure_failed", e.to_string()))?;
        }
        if sub_matches.get_flag("show") || update.is_empty() {
            let config = AdvisorConfig::read()
                .map_err(|e| CliError::new("advisor_configure_failed", e.to_string()))?;
            print_advisor_provider_config(&config);
        } else {
            let config = AdvisorConfig::read()
                .map_err(|e| CliError::new("advisor_configure_failed", e.to_string()))?;
            println!("Advisor provider configuration updated.");
            print_advisor_provider_config(&config);
        }
        Ok(())
    }

    fn advisor_check(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let account_id = Uuid::parse_str(sub_matches.get_one::<String>("account").unwrap())
            .map_err(|_| CliError::new("invalid_uuid", "Invalid --account UUID"))?;
        let symbol = sub_matches.get_one::<String>("symbol").unwrap().to_string();
        let entry_price = Decimal::from_str_exact(sub_matches.get_one::<String>("entry").unwrap())
            .map_err(|_| CliError::new("invalid_decimal", "Invalid --entry"))?;
        let quantity = Decimal::from_str_exact(sub_matches.get_one::<String>("quantity").unwrap())
            .map_err(|_| CliError::new("invalid_decimal", "Invalid --quantity"))?;
        let sector = sub_matches.get_one::<String>("sector").cloned();
        let asset_class = sub_matches.get_one::<String>("asset-class").cloned();

        let result = self
            .trust
            .advisory_check_trade(TradeProposal {
                account_id,
                symbol,
                sector,
                asset_class,
                entry_price,
                quantity,
            })
            .map_err(|e| CliError::new("advisor_check_failed", e.to_string()))?;

        println!("Advisory level: {:?}", result.level);
        println!(
            "Projected concentrations: sector={} asset_class={} single_position={}",
            result.projected_sector_pct,
            result.projected_asset_class_pct,
            result.projected_single_position_pct
        );
        if !result.warnings.is_empty() {
            println!("Warnings:");
            for warning in result.warnings {
                println!("  - {warning}");
            }
        }
        if !result.recommendations.is_empty() {
            println!("Recommendations:");
            for recommendation in result.recommendations {
                println!("  - {recommendation}");
            }
        }
        Ok(())
    }

    fn advisor_status(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let account_id = Uuid::parse_str(sub_matches.get_one::<String>("account").unwrap())
            .map_err(|_| CliError::new("invalid_uuid", "Invalid --account UUID"))?;
        let status = self
            .trust
            .advisory_status_for_account(account_id)
            .map_err(|e| CliError::new("advisor_status_failed", e.to_string()))?;
        println!("Portfolio advisory level: {:?}", status.level);
        println!("Top sector concentration: {}", status.top_sector_pct);
        println!(
            "Top asset class concentration: {}",
            status.top_asset_class_pct
        );
        println!(
            "Top single position concentration: {}",
            status.top_position_pct
        );
        for warning in status.warnings {
            println!("  - {warning}");
        }
        Ok(())
    }

    fn advisor_history(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let account_id = Uuid::parse_str(sub_matches.get_one::<String>("account").unwrap())
            .map_err(|_| CliError::new("invalid_uuid", "Invalid --account UUID"))?;
        let days = sub_matches
            .get_one::<String>("days")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(30);
        let entries = self.trust.advisory_history_for_account(account_id, days);
        if entries.is_empty() {
            println!("No advisory history entries.");
            return Ok(());
        }
        for entry in entries {
            println!(
                "{} {:?} {} {}",
                entry.created_at, entry.level, entry.symbol, entry.summary
            );
        }
        Ok(())
    }

    fn db_export(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let output = sub_matches
            .get_one::<String>("output")
            .map(String::as_str)
            .unwrap_or("trust-backup.json");

        let db = SqliteDatabase::new(Self::database_url().as_str());
        db.export_backup_to_path(Path::new(output))
            .map_err(|error| {
                CliError::new(
                    "db_export_failed",
                    format!("Unable to export DB backup to {output}: {error}"),
                )
            })?;

        // Print the output path so scripts can capture it.
        println!("{output}");
        Ok(())
    }

    fn db_import(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        let input = sub_matches
            .get_one::<String>("input")
            .ok_or_else(|| CliError::new("missing_input", "Missing --input for db import"))?;

        // DB import mutates persisted state, so keep it protected.
        self.ensure_protected_keyword(sub_matches, ReportOutputFormat::Text, "database import")?;

        let mode = match sub_matches.get_one::<String>("mode").map(String::as_str) {
            Some("replace") => ImportMode::Replace,
            _ => ImportMode::Strict,
        };
        let dry_run = sub_matches.get_flag("dry-run");

        let mut db = SqliteDatabase::new(Self::database_url().as_str());
        let report = db
            .import_backup_from_path(Path::new(input), ImportOptions { mode, dry_run })
            .map_err(|error| {
                CliError::new(
                    "db_import_failed",
                    format!("Unable to import DB backup from {input}: {error}"),
                )
            })?;

        println!(
            "cleared_rows={} inserted_rows={}",
            report.cleared_rows, report.inserted_rows
        );
        Ok(())
    }
}

impl ArgDispatcher {
    fn transfer_accounts(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        let from_id_str = matches.get_one::<String>("from").unwrap();
        let to_id_str = matches.get_one::<String>("to").unwrap();
        let amount_str = matches.get_one::<String>("amount").unwrap();
        let reason = matches.get_one::<String>("reason").unwrap();

        let from_id = Uuid::parse_str(from_id_str)
            .map_err(|_| CliError::new("invalid_uuid", "Invalid --from UUID"))?;
        let to_id = Uuid::parse_str(to_id_str)
            .map_err(|_| CliError::new("invalid_uuid", "Invalid --to UUID"))?;
        let amount = Decimal::from_str_exact(amount_str)
            .map_err(|_| CliError::new("invalid_decimal", "Invalid --amount decimal"))?;

        let currency = Currency::USD;
        let (withdrawal_id, deposit_id) = self
            .trust
            .transfer_between_accounts(from_id, to_id, amount, currency, reason)
            .map_err(|e| CliError::new("transfer_failed", e.to_string()))?;

        println!("Transfer completed:");
        println!("  amount: {}", crate::zen::amount(amount));
        println!("  from:   {from_id}");
        println!("  to:     {to_id}");
        println!("  reason: {reason}");
        println!("  withdrawal_tx_id: {withdrawal_id}");
        println!("  deposit_tx_id:    {deposit_id}");
        Ok(())
    }
}

// Distribution Operations
impl ArgDispatcher {
    fn configure_distribution(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        let account_id_str = matches.get_one::<String>("account-id").unwrap();
        let earnings_str = matches.get_one::<String>("earnings").unwrap();
        let tax_str = matches.get_one::<String>("tax").unwrap();
        let reinvestment_str = matches.get_one::<String>("reinvestment").unwrap();
        let threshold_str = matches.get_one::<String>("threshold").unwrap();
        let password = if let Some(p) = matches.get_one::<String>("password") {
            p.to_string()
        } else {
            Password::new()
                .with_prompt("Distribution configuration password")
                .with_confirmation("Confirm password", "Passwords do not match")
                .interact()
                .map_err(|e| CliError::new("password_prompt_failed", e.to_string()))?
        };

        let account_id = Uuid::parse_str(account_id_str)
            .map_err(|_| CliError::new("invalid_uuid", "Invalid --account-id UUID"))?;

        let earnings_pct = Decimal::from_str_exact(earnings_str)
            .map_err(|_| CliError::new("invalid_decimal", "Invalid --earnings decimal"))?
            .checked_div(Decimal::new(100, 0))
            .ok_or_else(|| CliError::new("invalid_decimal", "Invalid --earnings percentage"))?;
        let tax_pct = Decimal::from_str_exact(tax_str)
            .map_err(|_| CliError::new("invalid_decimal", "Invalid --tax decimal"))?
            .checked_div(Decimal::new(100, 0))
            .ok_or_else(|| CliError::new("invalid_decimal", "Invalid --tax percentage"))?;
        let reinvestment_pct = Decimal::from_str_exact(reinvestment_str)
            .map_err(|_| CliError::new("invalid_decimal", "Invalid --reinvestment decimal"))?
            .checked_div(Decimal::new(100, 0))
            .ok_or_else(|| CliError::new("invalid_decimal", "Invalid --reinvestment percentage"))?;
        let threshold = Decimal::from_str_exact(threshold_str)
            .map_err(|_| CliError::new("invalid_decimal", "Invalid --threshold decimal"))?;

        self.trust
            .configure_distribution(
                account_id,
                earnings_pct,
                tax_pct,
                reinvestment_pct,
                threshold,
                &password,
            )
            .map_err(|e| CliError::new("configure_distribution_failed", e.to_string()))?;

        println!("Distribution rules configured:");
        println!("  account_id: {account_id}");
        println!("  earnings: {earnings_pct}");
        println!("  tax: {tax_pct}");
        println!("  reinvestment: {reinvestment_pct}");
        println!("  minimum_threshold: {}", crate::zen::amount(threshold));
        Ok(())
    }

    fn execute_distribution(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        let account_id_str = matches.get_one::<String>("account-id").unwrap();
        let amount_str = matches.get_one::<String>("amount").unwrap();

        let account_id = Uuid::parse_str(account_id_str)
            .map_err(|_| CliError::new("invalid_uuid", "Invalid --account-id UUID"))?;
        let amount = Decimal::from_str_exact(amount_str)
            .map_err(|_| CliError::new("invalid_decimal", "Invalid --amount decimal"))?;

        let currency = Currency::USD;
        let result = self
            .trust
            .execute_distribution(account_id, amount, currency)
            .map_err(|e| CliError::new("execute_distribution_failed", e.to_string()))?;

        println!("Distribution executed:");
        println!("  source_account_id: {}", result.source_account_id);
        println!(
            "  original_amount: {}",
            crate::zen::amount_share(result.original_amount, result.original_amount)
        );
        println!(
            "  earnings_amount: {}",
            crate::zen::amount_share(
                result.earnings_amount.unwrap_or_default(),
                result.original_amount,
            )
        );
        println!(
            "  tax_amount: {}",
            crate::zen::amount_share(
                result.tax_amount.unwrap_or_default(),
                result.original_amount
            )
        );
        println!(
            "  reinvestment_amount: {}",
            crate::zen::amount_share(
                result.reinvestment_amount.unwrap_or_default(),
                result.original_amount,
            )
        );
        println!(
            "  transactions_created: {}",
            result.transactions_created.len()
        );
        Ok(())
    }

    fn distribution_history(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        let account_id_str = matches.get_one::<String>("account-id").unwrap();
        let limit = matches
            .get_one::<String>("limit")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(20);

        let account_id = Uuid::parse_str(account_id_str)
            .map_err(|_| CliError::new("invalid_uuid", "Invalid --account-id UUID"))?;

        let mut entries = self
            .trust
            .distribution_history(account_id)
            .map_err(|e| CliError::new("distribution_history_failed", e.to_string()))?;

        if entries.len() > limit {
            entries.truncate(limit);
        }

        println!("Distribution History (most recent first):");
        for entry in entries {
            let basis = entry.original_amount;
            println!(
                "- at={} trade_id={} amount={} earnings={} tax={} reinvestment={}",
                entry.distribution_date,
                entry
                    .trade_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "N/A".to_string()),
                crate::zen::amount_share(entry.original_amount, basis),
                entry
                    .earnings_amount
                    .map(|v| crate::zen::amount_share(v, basis))
                    .unwrap_or_else(|| crate::zen::amount_share(Decimal::ZERO, basis)),
                entry
                    .tax_amount
                    .map(|v| crate::zen::amount_share(v, basis))
                    .unwrap_or_else(|| crate::zen::amount_share(Decimal::ZERO, basis)),
                entry
                    .reinvestment_amount
                    .map(|v| crate::zen::amount_share(v, basis))
                    .unwrap_or_else(|| crate::zen::amount_share(Decimal::ZERO, basis)),
            );
        }

        Ok(())
    }

    fn distribution_rules(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        let account_id_str = matches.get_one::<String>("account-id").unwrap();
        let account_id = Uuid::parse_str(account_id_str)
            .map_err(|_| CliError::new("invalid_uuid", "Invalid --account-id UUID"))?;

        let rules = self
            .trust
            .distribution_rules_for_account(account_id)
            .map_err(|e| CliError::new("distribution_rules_failed", e.to_string()))?;

        println!("Distribution rules:");
        println!("  account_id: {}", rules.account_id);
        println!("  earnings_percent: {}", rules.earnings_percent);
        println!("  tax_percent: {}", rules.tax_percent);
        println!("  reinvestment_percent: {}", rules.reinvestment_percent);
        println!(
            "  minimum_threshold: {}",
            crate::zen::amount(rules.minimum_threshold)
        );
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
mod tests {
    #![allow(
        clippy::too_many_lines,
        clippy::cognitive_complexity,
        clippy::indexing_slicing
    )]

    use super::ArgDispatcher;
    use super::BondInventoryStats;
    use super::CliError;
    use super::ReportOutputFormat;
    use super::TradeAdvisorCheck;
    use super::TradeHypothesisArgs;
    use super::TradeHypothesisReport;
    use super::TradePreviewArgs;
    use super::TradingVehicleSearchFilters;
    use crate::commands::{
        AccountCommandBuilder, AdvisorCommandBuilder, DbCommandBuilder, DistributionCommandBuilder,
        GradeCommandBuilder, KeysCommandBuilder, LevelCommandBuilder, MarketDataCommandBuilder,
        MetricsCommandBuilder, OnboardingCommandBuilder, PolicyCommandBuilder,
        ReportCommandBuilder, RuleCommandBuilder, SessionCommandBuilder, TradeCommandBuilder,
        TradingVehicleCommandBuilder, TransactionCommandBuilder,
    };
    use alpaca_broker::AlpacaBroker;
    use chrono::{Days, NaiveDate};
    use chrono::{TimeZone, Utc};
    use clap::{Arg, Command};
    use core::calculators_concentration::{ConcentrationGroup, ConcentrationWarning, WarningLevel};
    use core::services::leveling::{
        LevelCriterionProgress, LevelDecision, LevelEvaluationOutcome, LevelPathProgress,
        LevelPerformanceSnapshot, LevelProgressReport,
    };
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{
        database::TradingVehicleUpsert, AccountType, Broker, BrokerKind, BrokerLog, Currency,
        Environment, FixedIncomeTerms, Level, LevelAdjustmentRules, LevelDirection, LevelStatus,
        LevelTrigger, MarketBar, MarketDataChannel, MarketDataStreamEvent, MarketQuote,
        MarketSnapshotSource, MarketSnapshotV2, MarketTradeTick, Mistake, MistakeErrorType,
        MungerTendency, OrderCategory, OrderIds, SessionRegime, Status, Trade,
        TradingVehicleCategory, TransactionCategory,
    };
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use serde_json::json;
    use std::error::Error;
    use uuid::Uuid;

    #[derive(Clone)]
    struct StubBroker {
        bars: Vec<MarketBar>,
        quote: Option<MarketQuote>,
        trade: Option<MarketTradeTick>,
        events: Vec<MarketDataStreamEvent>,
        fail_bars: bool,
        fail_stream: bool,
        submit_succeeds: bool,
        sync_fills_entry: bool,
        sync_returns_empty: bool,
    }

    impl StubBroker {
        fn quote_trade_fixture() -> Self {
            Self {
                bars: vec![MarketBar {
                    time: Utc
                        .with_ymd_and_hms(2026, 2, 24, 10, 0, 0)
                        .single()
                        .expect("valid fixture timestamp"),
                    open: dec!(198),
                    high: dec!(205),
                    low: dec!(197),
                    close: dec!(201),
                    volume: 12_345,
                }],
                quote: Some(MarketQuote {
                    symbol: "AAPL".to_string(),
                    as_of: Utc
                        .with_ymd_and_hms(2026, 2, 24, 10, 0, 0)
                        .single()
                        .expect("valid fixture timestamp"),
                    bid_price: dec!(200.95),
                    bid_size: 100,
                    ask_price: dec!(201.05),
                    ask_size: 120,
                }),
                trade: Some(MarketTradeTick {
                    symbol: "AAPL".to_string(),
                    as_of: Utc
                        .with_ymd_and_hms(2026, 2, 24, 10, 0, 0)
                        .single()
                        .expect("valid fixture timestamp"),
                    price: dec!(201),
                    size: 50,
                }),
                events: vec![
                    MarketDataStreamEvent {
                        channel: MarketDataChannel::Quotes,
                        symbol: "AAPL".to_string(),
                        as_of: Utc
                            .with_ymd_and_hms(2026, 2, 24, 10, 0, 1)
                            .single()
                            .expect("valid fixture timestamp"),
                        price: dec!(201),
                        size: 20,
                    },
                    MarketDataStreamEvent {
                        channel: MarketDataChannel::Trades,
                        symbol: "AAPL".to_string(),
                        as_of: Utc
                            .with_ymd_and_hms(2026, 2, 24, 10, 0, 2)
                            .single()
                            .expect("valid fixture timestamp"),
                        price: dec!(201.1),
                        size: 10,
                    },
                ],
                fail_bars: false,
                fail_stream: false,
                submit_succeeds: false,
                sync_fills_entry: false,
                sync_returns_empty: false,
            }
        }

        fn submitted_fill_fixture() -> Self {
            let mut broker = Self::quote_trade_fixture();
            broker.submit_succeeds = true;
            broker.sync_fills_entry = true;
            broker
        }

        fn submitted_empty_sync_fixture() -> Self {
            let mut broker = Self::submitted_fill_fixture();
            broker.sync_returns_empty = true;
            broker
        }
    }

    impl Broker for StubBroker {
        fn kind(&self) -> BrokerKind {
            BrokerKind::Alpaca
        }

        fn submit_trade(
            &self,
            trade: &Trade,
            _account: &model::Account,
        ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
            if !self.submit_succeeds {
                return Err("not implemented in test stub".into());
            }
            Ok((
                BrokerLog {
                    trade_id: trade.id,
                    log: "submitted by test stub".to_string(),
                    ..BrokerLog::default()
                },
                OrderIds {
                    stop: format!("stop-{}", trade.id),
                    entry: format!("entry-{}", trade.id),
                    target: format!("target-{}", trade.id),
                },
            ))
        }

        fn sync_trade(
            &self,
            trade: &Trade,
            _account: &model::Account,
        ) -> Result<(Status, Vec<model::Order>, BrokerLog), Box<dyn Error>> {
            if !self.sync_fills_entry {
                return Err("not implemented in test stub".into());
            }

            if self.sync_returns_empty {
                return Ok((
                    Status::Submitted,
                    vec![],
                    BrokerLog {
                        trade_id: trade.id,
                        log: "empty sync by test stub".to_string(),
                        ..BrokerLog::default()
                    },
                ));
            }

            let mut entry = trade.entry.clone();
            entry.status = model::OrderStatus::Filled;
            entry.filled_quantity = entry.quantity;
            entry.average_filled_price = Some(entry.unit_price);
            entry.filled_at = Some(Utc::now().naive_utc());
            Ok((
                Status::Filled,
                vec![entry],
                BrokerLog {
                    trade_id: trade.id,
                    log: "synced by test stub".to_string(),
                    ..BrokerLog::default()
                },
            ))
        }

        fn close_trade(
            &self,
            _trade: &Trade,
            _account: &model::Account,
        ) -> Result<(model::Order, BrokerLog), Box<dyn Error>> {
            Err("not implemented in test stub".into())
        }

        fn cancel_trade(
            &self,
            _trade: &Trade,
            _account: &model::Account,
        ) -> Result<(), Box<dyn Error>> {
            Err("not implemented in test stub".into())
        }

        fn modify_stop(
            &self,
            _trade: &Trade,
            _account: &model::Account,
            _new_stop_price: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Err("not implemented in test stub".into())
        }

        fn modify_target(
            &self,
            _trade: &Trade,
            _account: &model::Account,
            _new_price: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Err("not implemented in test stub".into())
        }

        fn get_bars(
            &self,
            _symbol: &str,
            _start: chrono::DateTime<Utc>,
            _end: chrono::DateTime<Utc>,
            _timeframe: model::BarTimeframe,
            _account: &model::Account,
        ) -> Result<Vec<MarketBar>, Box<dyn Error>> {
            if self.fail_bars {
                return Err("bars unavailable".into());
            }
            Ok(self.bars.clone())
        }

        fn get_latest_quote(
            &self,
            _symbol: &str,
            _account: &model::Account,
        ) -> Result<MarketQuote, Box<dyn Error>> {
            self.quote.clone().ok_or_else(|| "quote unavailable".into())
        }

        fn get_latest_trade(
            &self,
            _symbol: &str,
            _account: &model::Account,
        ) -> Result<MarketTradeTick, Box<dyn Error>> {
            self.trade.clone().ok_or_else(|| "trade unavailable".into())
        }

        fn stream_market_data(
            &self,
            _symbols: &[String],
            _channels: &[MarketDataChannel],
            _max_events: usize,
            _timeout_seconds: u64,
            _account: &model::Account,
        ) -> Result<Vec<MarketDataStreamEvent>, Box<dyn Error>> {
            if self.fail_stream {
                return Err("stream unavailable".into());
            }
            Ok(self.events.clone())
        }
    }

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        crate::protected_keyword::test_env_guard()
    }

    fn test_dispatcher() -> ArgDispatcher {
        let trust = TrustFacade::new(
            Box::new(SqliteDatabase::new_in_memory()),
            Box::<AlpacaBroker>::default(),
        );
        ArgDispatcher { trust }
    }

    fn test_dispatcher_with_broker(broker: Box<dyn Broker>) -> ArgDispatcher {
        let trust = TrustFacade::new(Box::new(SqliteDatabase::new_in_memory()), broker);
        ArgDispatcher { trust }
    }

    fn test_dispatcher_with_read_failures(
        factory: crate::test_support::ReadFailureFactory,
    ) -> ArgDispatcher {
        let trust = TrustFacade::new(Box::new(factory), Box::<AlpacaBroker>::default());
        ArgDispatcher { trust }
    }

    fn account_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("confirm-protected").long("confirm-protected"))
            .arg(Arg::new("name").long("name"))
            .arg(Arg::new("description").long("description"))
            .arg(Arg::new("environment").long("environment"))
            .arg(Arg::new("taxes").long("taxes"))
            .arg(Arg::new("earnings").long("earnings"))
            .arg(Arg::new("type").long("type"))
            .arg(Arg::new("broker").long("broker"))
            .arg(Arg::new("broker-account-id").long("broker-account-id"))
            .arg(Arg::new("parent").long("parent"))
            .arg(Arg::new("account").long("account"))
            .arg(
                Arg::new("force")
                    .long("force")
                    .action(clap::ArgAction::SetTrue),
            )
            .get_matches_from(argv)
    }

    fn trade_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("symbol").long("symbol"))
            .arg(Arg::new("category").long("category"))
            .arg(Arg::new("entry").long("entry"))
            .arg(Arg::new("stop").long("stop"))
            .arg(Arg::new("safety-order-type").long("safety-order-type"))
            .arg(Arg::new("target").long("target"))
            .arg(Arg::new("quantity").long("quantity"))
            .arg(Arg::new("currency").long("currency"))
            .arg(
                Arg::new("auto-fund")
                    .long("auto-fund")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                Arg::new("auto-submit")
                    .long("auto-submit")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(Arg::new("thesis").long("thesis"))
            .arg(Arg::new("sector").long("sector"))
            .arg(Arg::new("asset-class").long("asset-class"))
            .arg(Arg::new("context").long("context"))
            .get_matches_from(argv)
    }

    fn trade_event_add_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("trade-id").long("trade-id"))
            .arg(Arg::new("type").long("type"))
            .arg(Arg::new("date").long("date"))
            .arg(Arg::new("severity").long("severity"))
            .arg(Arg::new("notes").long("notes"))
            .get_matches_from(argv)
    }

    fn trade_event_list_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("trade-id").long("trade-id"))
            .arg(Arg::new("format").long("format"))
            .get_matches_from(argv)
    }

    fn trade_autopsy_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("trade-id").long("trade-id"))
            .get_matches_from(argv)
    }

    fn size_preview_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("entry").long("entry"))
            .arg(Arg::new("stop").long("stop"))
            .arg(Arg::new("currency").long("currency"))
            .get_matches_from(argv)
    }

    fn hypothesis_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("entry").long("entry"))
            .arg(Arg::new("stop").long("stop"))
            .arg(Arg::new("target").long("target"))
            .arg(Arg::new("quantity").long("quantity"))
            .arg(Arg::new("currency").long("currency"))
            .get_matches_from(argv)
    }

    fn market_stream_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("symbols").long("symbols"))
            .arg(Arg::new("channels").long("channels"))
            .arg(Arg::new("max-events").long("max-events"))
            .arg(Arg::new("timeout-seconds").long("timeout-seconds"))
            .get_matches_from(argv)
    }

    fn market_data_command_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["market-data"];
        argv.extend_from_slice(args);
        MarketDataCommandBuilder::new()
            .snapshot()
            .bars()
            .stream()
            .quote()
            .trade()
            .session()
            .build()
            .get_matches_from(argv)
    }

    fn report_args_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("account").long("account"))
            .arg(
                Arg::new("days")
                    .long("days")
                    .value_parser(clap::value_parser!(u32)),
            )
            .arg(
                Arg::new("open-only")
                    .long("open-only")
                    .action(clap::ArgAction::SetTrue),
            )
            .get_matches_from(argv)
    }

    fn advisor_configure_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("confirm-protected").long("confirm-protected"))
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("sector-limit").long("sector-limit"))
            .arg(Arg::new("asset-class-limit").long("asset-class-limit"))
            .arg(Arg::new("single-position-limit").long("single-position-limit"))
            .arg(Arg::new("calendar-provider").long("calendar-provider"))
            .arg(Arg::new("calendar-api-key").long("calendar-api-key"))
            .arg(Arg::new("claude-api-key").long("claude-api-key"))
            .arg(
                Arg::new("show")
                    .long("show")
                    .action(clap::ArgAction::SetTrue),
            )
            .get_matches_from(argv)
    }

    fn advisor_check_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("symbol").long("symbol"))
            .arg(Arg::new("entry").long("entry"))
            .arg(Arg::new("quantity").long("quantity"))
            .arg(Arg::new("sector").long("sector"))
            .arg(Arg::new("asset-class").long("asset-class"))
            .get_matches_from(argv)
    }

    fn advisor_history_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("days").long("days"))
            .get_matches_from(argv)
    }

    fn session_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("format").long("format"))
            .get_matches_from(argv)
    }

    fn distribution_config_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("account-id").long("account-id"))
            .arg(Arg::new("earnings").long("earnings"))
            .arg(Arg::new("tax").long("tax"))
            .arg(Arg::new("reinvestment").long("reinvestment"))
            .arg(Arg::new("threshold").long("threshold"))
            .arg(Arg::new("password").long("password"))
            .get_matches_from(argv)
    }

    fn distribution_execute_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("account-id").long("account-id"))
            .arg(Arg::new("amount").long("amount"))
            .get_matches_from(argv)
    }

    fn distribution_history_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("account-id").long("account-id"))
            .arg(Arg::new("limit").long("limit"))
            .get_matches_from(argv)
    }

    fn distribution_rules_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("account-id").long("account-id"))
            .get_matches_from(argv)
    }

    fn transfer_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("from").long("from"))
            .arg(Arg::new("to").long("to"))
            .arg(Arg::new("amount").long("amount"))
            .arg(Arg::new("reason").long("reason"))
            .get_matches_from(argv)
    }

    fn root_command() -> Command {
        Command::new("trust")
            .subcommand_required(true)
            .arg_required_else_help(true)
            .allow_external_subcommands(true)
            .subcommand(DbCommandBuilder::new().export().import().build())
            .subcommand(
                KeysCommandBuilder::new()
                    .create_keys()
                    .read_environment()
                    .delete_environment()
                    .protected_set()
                    .protected_show()
                    .protected_delete()
                    .build(),
            )
            .subcommand(
                AccountCommandBuilder::new()
                    .create_account()
                    .read_account()
                    .list_accounts()
                    .balance_accounts()
                    .transfer_account()
                    .delete_account()
                    .build(),
            )
            .subcommand(
                TransactionCommandBuilder::new()
                    .deposit()
                    .withdraw()
                    .build(),
            )
            .subcommand(
                RuleCommandBuilder::new()
                    .create_rule()
                    .remove_rule()
                    .build(),
            )
            .subcommand(
                TradingVehicleCommandBuilder::new()
                    .create_trading_vehicle()
                    .update_bond_terms()
                    .search_trading_vehicle()
                    .build(),
            )
            .subcommand(
                TradeCommandBuilder::new()
                    .create_trade()
                    .search_trade()
                    .list_open()
                    .reconcile()
                    .fund_trade()
                    .cancel_trade()
                    .submit_trade()
                    .sync_trade()
                    .watch_trade()
                    .manually_fill()
                    .manually_stop()
                    .manually_target()
                    .manually_close()
                    .modify_stop()
                    .modify_target()
                    .size_preview()
                    .build(),
            )
            .subcommand(
                DistributionCommandBuilder::new()
                    .configure_distribution()
                    .execute_distribution()
                    .history()
                    .show_rules()
                    .build(),
            )
            .subcommand(
                ReportCommandBuilder::new()
                    .performance()
                    .drawdown()
                    .risk()
                    .concentration()
                    .summary()
                    .metrics()
                    .attribution()
                    .benchmark()
                    .timeline()
                    .bias_summary()
                    .build(),
            )
            .subcommand(
                MarketDataCommandBuilder::new()
                    .snapshot()
                    .bars()
                    .stream()
                    .quote()
                    .trade()
                    .session()
                    .build(),
            )
            .subcommand(GradeCommandBuilder::new().show().summary().build())
            .subcommand(
                LevelCommandBuilder::new()
                    .status()
                    .triggers()
                    .history()
                    .change()
                    .evaluate()
                    .progress()
                    .rules()
                    .build(),
            )
            .subcommand(
                MetricsCommandBuilder::new()
                    .advanced()
                    .compare()
                    .bond()
                    .build(),
            )
            .subcommand(
                AdvisorCommandBuilder::new()
                    .configure()
                    .check()
                    .status()
                    .history()
                    .build(),
            )
            .subcommand(SessionCommandBuilder::new().open().close().list().build())
            .subcommand(OnboardingCommandBuilder::new().init().status().build())
            .subcommand(PolicyCommandBuilder::new().build())
    }

    fn seed_account_and_vehicle(
        dispatcher: &mut ArgDispatcher,
    ) -> (model::Account, model::TradingVehicle) {
        let account = dispatcher
            .trust
            .create_account("disp-acct", "test", Environment::Paper, dec!(20), dec!(10))
            .expect("account");
        dispatcher
            .trust
            .create_transaction(
                &account,
                &model::TransactionCategory::Deposit,
                dec!(10_000),
                &Currency::USD,
            )
            .expect("deposit");
        let vehicle = dispatcher
            .trust
            .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
            .expect("vehicle");
        (account, vehicle)
    }

    fn seed_new_trade(dispatcher: &mut ArgDispatcher) -> (model::Account, Trade) {
        let (account, vehicle) = seed_account_and_vehicle(dispatcher);
        let trade = dispatcher
            .trust
            .create_trade(
                model::DraftTrade {
                    account: account.clone(),
                    trading_vehicle: vehicle,
                    quantity: 2,
                    category: model::TradeCategory::Long,
                    currency: Currency::USD,
                    thesis: Some("breakout".to_string()),
                    sector: Some("technology".to_string()),
                    asset_class: Some("equity".to_string()),
                    context: None,
                },
                dec!(95),
                dec!(100),
                dec!(110),
            )
            .expect("seed trade");
        (account, trade)
    }

    fn seed_filled_trade(dispatcher: &mut ArgDispatcher) -> (model::Account, Trade) {
        let (account, trade) = seed_new_trade(dispatcher);
        let (funded, _, _, _) = dispatcher.trust.fund_trade(&trade).expect("fund trade");
        let (submitted, _) = dispatcher
            .trust
            .submit_trade(&funded)
            .expect("submit trade");
        let (status, _, _) = dispatcher
            .trust
            .sync_trade(&submitted, &account)
            .expect("sync trade to filled");
        assert_eq!(status, Status::Filled);
        let filled = dispatcher
            .trust
            .search_trades(account.id, Status::Filled)
            .expect("load filled trades")
            .into_iter()
            .find(|candidate| candidate.id == trade.id)
            .expect("filled trade should be persisted");
        (account, filled)
    }

    fn seed_closed_target_trade(dispatcher: &mut ArgDispatcher) -> (model::Account, Trade) {
        let (account, filled) = seed_filled_trade(dispatcher);
        let mut target_ready = filled.clone();
        target_ready.target.status = model::OrderStatus::Filled;
        target_ready.target.filled_quantity = target_ready.target.quantity;
        target_ready.target.average_filled_price = Some(dec!(110));
        target_ready.target.filled_at = Some(Utc::now().naive_utc());
        dispatcher
            .trust
            .target_acquired(&target_ready, Decimal::ZERO)
            .expect("seed closed target trade");
        let closed = dispatcher
            .trust
            .search_trades(account.id, Status::ClosedTarget)
            .expect("load closed target trades")
            .into_iter()
            .find(|candidate| candidate.id == filled.id)
            .expect("closed trade should be persisted");
        (account, closed)
    }

    fn seed_mistake(
        dispatcher: &mut ArgDispatcher,
        trade_id: Uuid,
        bias_tags: Vec<MungerTendency>,
        lollapalooza: bool,
        error_type: MistakeErrorType,
    ) -> Mistake {
        let now = Utc::now().naive_utc();
        dispatcher
            .trust
            .create_mistake(Mistake {
                id: Uuid::new_v4(),
                created_at: now,
                updated_at: now,
                deleted_at: None,
                trade_id,
                bias_tags,
                lollapalooza,
                error_type,
                rule_violated: None,
                counterfactual_r: Decimal::ZERO,
                lesson: "Review the process rule before taking the next setup.".to_string(),
            })
            .expect("mistake should persist")
    }

    #[test]
    fn test_protected_keyword_validator() {
        assert!(ArgDispatcher::is_valid_protected_keyword("abc", "abc"));
        assert!(!ArgDispatcher::is_valid_protected_keyword("abc", "abcd"));
        assert!(!ArgDispatcher::is_valid_protected_keyword("abc", "ABC"));
    }

    #[test]
    fn test_parse_account_type_accepts_aliases() {
        assert_eq!(
            ArgDispatcher::parse_account_type("primary").unwrap(),
            AccountType::Primary
        );
        assert_eq!(
            ArgDispatcher::parse_account_type("earnings").unwrap(),
            AccountType::Earnings
        );
        assert_eq!(
            ArgDispatcher::parse_account_type("tax-reserve").unwrap(),
            AccountType::TaxReserve
        );
        assert_eq!(
            ArgDispatcher::parse_account_type("tax_reserve").unwrap(),
            AccountType::TaxReserve
        );
        assert_eq!(
            ArgDispatcher::parse_account_type("reinvestment").unwrap(),
            AccountType::Reinvestment
        );
    }

    #[test]
    fn test_parse_environment_accepts_aliases() {
        assert_eq!(
            ArgDispatcher::parse_environment("paper").unwrap(),
            Environment::Paper
        );
        assert_eq!(
            ArgDispatcher::parse_environment("sandbox").unwrap(),
            Environment::Paper
        );
        assert_eq!(
            ArgDispatcher::parse_environment("live").unwrap(),
            Environment::Live
        );
        assert_eq!(
            ArgDispatcher::parse_environment("production").unwrap(),
            Environment::Live
        );
    }

    #[test]
    fn test_parse_environment_rejects_invalid_value() {
        let error = ArgDispatcher::parse_environment("demo").unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid_environment: Invalid --environment value (expected paper|live)"
        );
    }

    #[test]
    fn test_parse_account_type_rejects_invalid_value() {
        let error = ArgDispatcher::parse_account_type("savings").unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid_account_type: Invalid --type value (expected primary|earnings|tax-reserve|reinvestment)"
        );
    }

    #[test]
    fn test_parse_broker_kind_accepts_supported_values() {
        assert_eq!(
            ArgDispatcher::parse_broker_kind("alpaca").unwrap(),
            BrokerKind::Alpaca
        );
        assert_eq!(
            ArgDispatcher::parse_broker_kind("IBKR").unwrap(),
            BrokerKind::Ibkr
        );
    }

    #[test]
    fn test_parse_broker_kind_rejects_invalid_value() {
        let error = ArgDispatcher::parse_broker_kind("binance").unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid_broker_kind: Invalid --broker value (expected alpaca|ibkr)"
        );
    }

    #[test]
    fn test_parse_period_window_last_days() {
        let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
        let (from, to) = ArgDispatcher::parse_period_window("last-7-days", now).unwrap();
        assert_eq!(to.unwrap().to_string(), "2026-02-18");
        assert_eq!(from.unwrap().to_string(), "2026-02-12");
    }

    #[test]
    fn test_parse_period_window_previous_days() {
        let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
        let (from, to) = ArgDispatcher::parse_period_window("previous-7-days", now).unwrap();
        assert_eq!(to.unwrap().to_string(), "2026-02-11");
        assert_eq!(from.unwrap().to_string(), "2026-02-05");
    }

    #[test]
    fn test_parse_period_window_range() {
        let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
        let (from, to) = ArgDispatcher::parse_period_window("2026-01-01..2026-01-31", now).unwrap();
        assert_eq!(from.unwrap().to_string(), "2026-01-01");
        assert_eq!(to.unwrap().to_string(), "2026-01-31");
    }

    #[test]
    fn test_parse_period_window_all() {
        let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
        let (from, to) = ArgDispatcher::parse_period_window("all", now).unwrap();
        assert!(from.is_none());
        assert!(to.is_none());
    }

    #[test]
    fn test_parse_period_window_invalid_range_order() {
        let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
        let error = ArgDispatcher::parse_period_window("2026-02-10..2026-01-01", now).unwrap_err();
        assert_eq!(error, "Invalid period range: start date is after end date");
    }

    #[test]
    fn test_parse_period_window_invalid_format() {
        let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
        let error = ArgDispatcher::parse_period_window("last-seven-days", now).unwrap_err();
        assert!(
            error.contains("Invalid period format"),
            "unexpected error message: {error}"
        );
    }

    #[test]
    fn test_parse_period_window_invalid_date() {
        let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
        let error = ArgDispatcher::parse_period_window("2026-01-40..2026-01-31", now).unwrap_err();
        assert_eq!(error, "Invalid period start date (expected YYYY-MM-DD)");
    }

    #[test]
    fn test_metrics_compare_executes_text_json_validation_and_exports() {
        let mut dispatcher = test_dispatcher();
        let account = dispatcher
            .trust
            .create_account(
                "metrics-compare-account",
                "metrics compare",
                Environment::Paper,
                dec!(10),
                dec!(5),
            )
            .expect("seed account");
        let text = MetricsCommandBuilder::new()
            .compare()
            .build()
            .get_matches_from([
                "metrics",
                "compare",
                "--period1",
                "all",
                "--period2",
                "last-7-days",
            ]);
        dispatcher.metrics_compare(text.subcommand_matches("compare").unwrap());

        let valid_account = MetricsCommandBuilder::new()
            .compare()
            .build()
            .get_matches_from([
                "metrics",
                "compare",
                "--period1",
                "all",
                "--period2",
                "last-7-days",
                "--account",
                &account.id.to_string(),
            ]);
        dispatcher.metrics_compare(valid_account.subcommand_matches("compare").unwrap());

        let json = MetricsCommandBuilder::new()
            .compare()
            .build()
            .get_matches_from([
                "metrics",
                "compare",
                "--period1",
                "previous-7-days",
                "--period2",
                "last-7-days",
                "--format",
                "json",
            ]);
        dispatcher.metrics_compare(json.subcommand_matches("compare").unwrap());

        let invalid_account = MetricsCommandBuilder::new()
            .compare()
            .build()
            .get_matches_from([
                "metrics",
                "compare",
                "--period1",
                "all",
                "--period2",
                "last-7-days",
                "--account",
                "not-a-uuid",
            ]);
        dispatcher.metrics_compare(invalid_account.subcommand_matches("compare").unwrap());

        let invalid_second_period = MetricsCommandBuilder::new()
            .compare()
            .build()
            .get_matches_from([
                "metrics",
                "compare",
                "--period1",
                "all",
                "--period2",
                "not-a-period",
            ]);
        dispatcher.metrics_compare(invalid_second_period.subcommand_matches("compare").unwrap());

        let csv_output =
            std::env::temp_dir().join(format!("trust-metrics-compare-{}.csv", Uuid::new_v4()));
        let csv_output_arg = csv_output.to_string_lossy().to_string();
        let csv_export = MetricsCommandBuilder::new()
            .compare()
            .build()
            .get_matches_from([
                "metrics",
                "compare",
                "--period1",
                "all",
                "--period2",
                "last-7-days",
                "--export",
                "csv",
                "--output",
                &csv_output_arg,
            ]);
        dispatcher.metrics_compare(csv_export.subcommand_matches("compare").unwrap());
        assert!(csv_output.exists());
        let _ = std::fs::remove_file(&csv_output);

        let json_output =
            std::env::temp_dir().join(format!("trust-metrics-compare-{}.json", Uuid::new_v4()));
        let json_output_arg = json_output.to_string_lossy().to_string();
        let json_export = MetricsCommandBuilder::new()
            .compare()
            .build()
            .get_matches_from([
                "metrics",
                "compare",
                "--period1",
                "all",
                "--period2",
                "last-7-days",
                "--format",
                "json",
                "--export",
                "json",
                "--output",
                &json_output_arg,
            ]);
        dispatcher.metrics_compare(json_export.subcommand_matches("compare").unwrap());
        assert!(json_output.exists());
        let _ = std::fs::remove_file(&json_output);

        let failed_export = MetricsCommandBuilder::new()
            .compare()
            .build()
            .get_matches_from([
                "metrics",
                "compare",
                "--period1",
                "all",
                "--period2",
                "last-7-days",
                "--export",
                "csv",
                "--output",
                "/tmp",
            ]);
        dispatcher.metrics_compare(failed_export.subcommand_matches("compare").unwrap());

        let unsupported_export = Command::new("compare")
            .arg(Arg::new("period1").long("period1").required(true))
            .arg(Arg::new("period2").long("period2").required(true))
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("format").long("format"))
            .arg(Arg::new("export").long("export"))
            .arg(Arg::new("output").long("output"))
            .get_matches_from([
                "compare",
                "--period1",
                "all",
                "--period2",
                "last-7-days",
                "--export",
                "xml",
                "--output",
                "/tmp",
            ]);
        dispatcher.metrics_compare(&unsupported_export);
    }

    #[test]
    fn test_export_metrics_writes_json_csv_and_rejects_unknown_format() {
        let dispatcher = test_dispatcher();
        let mut trade = Trade {
            status: Status::ClosedTarget,
            ..Trade::default()
        };
        trade.balance.total_performance = dec!(125);
        let trades = vec![trade];

        let json_output =
            std::env::temp_dir().join(format!("trust-advanced-metrics-{}.json", Uuid::new_v4()));
        let json_output_arg = json_output.to_string_lossy().to_string();
        dispatcher
            .export_metrics(&trades, "json", &json_output_arg, dec!(0.05))
            .expect("json export should succeed");
        assert!(json_output.exists());
        let _ = std::fs::remove_file(&json_output);

        let csv_output =
            std::env::temp_dir().join(format!("trust-advanced-metrics-{}.csv", Uuid::new_v4()));
        let csv_output_arg = csv_output.to_string_lossy().to_string();
        dispatcher
            .export_metrics(&trades, "csv", &csv_output_arg, dec!(0.05))
            .expect("csv export should succeed");
        assert!(csv_output.exists());
        let _ = std::fs::remove_file(&csv_output);

        let unsupported = dispatcher
            .export_metrics(&trades, "xml", "/tmp/unused-trust-metrics.xml", dec!(0.05))
            .expect_err("unsupported export formats should fail");
        assert!(unsupported
            .to_string()
            .contains("Unsupported export format"));
    }

    #[test]
    fn test_metrics_advanced_executes_non_empty_display_and_export_paths() {
        let mut empty_dispatcher = test_dispatcher();
        let empty_display = MetricsCommandBuilder::new()
            .advanced()
            .build()
            .get_matches_from(["metrics", "advanced"]);
        empty_dispatcher.metrics_advanced(empty_display.subcommand_matches("advanced").unwrap());
        let invalid_account = MetricsCommandBuilder::new()
            .advanced()
            .build()
            .get_matches_from(["metrics", "advanced", "--account", "not-a-uuid"]);
        empty_dispatcher.metrics_advanced(invalid_account.subcommand_matches("advanced").unwrap());

        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (account, _closed) = seed_closed_target_trade(&mut dispatcher);

        let display = MetricsCommandBuilder::new()
            .advanced()
            .build()
            .get_matches_from([
                "metrics",
                "advanced",
                "--account",
                &account.id.to_string(),
                "--days",
                "0",
                "--risk-free-rate",
                "0.03",
            ]);
        dispatcher.metrics_advanced(display.subcommand_matches("advanced").unwrap());

        let json_output =
            std::env::temp_dir().join(format!("trust-metrics-advanced-{}.json", Uuid::new_v4()));
        let json_output_arg = json_output.to_string_lossy().to_string();
        let json_export = MetricsCommandBuilder::new()
            .advanced()
            .build()
            .get_matches_from([
                "metrics",
                "advanced",
                "--account",
                &account.id.to_string(),
                "--days",
                "0",
                "--export",
                "json",
                "--output",
                &json_output_arg,
            ]);
        dispatcher.metrics_advanced(json_export.subcommand_matches("advanced").unwrap());
        assert!(json_output.exists());
        let _ = std::fs::remove_file(&json_output);

        let csv_output =
            std::env::temp_dir().join(format!("trust-metrics-advanced-{}.csv", Uuid::new_v4()));
        let csv_output_arg = csv_output.to_string_lossy().to_string();
        let csv_export = MetricsCommandBuilder::new()
            .advanced()
            .build()
            .get_matches_from([
                "metrics",
                "advanced",
                "--account",
                &account.id.to_string(),
                "--days",
                "0",
                "--export",
                "csv",
                "--output",
                &csv_output_arg,
            ]);
        dispatcher.metrics_advanced(csv_export.subcommand_matches("advanced").unwrap());
        assert!(csv_output.exists());
        let _ = std::fs::remove_file(&csv_output);

        let failed_export = MetricsCommandBuilder::new()
            .advanced()
            .build()
            .get_matches_from([
                "metrics",
                "advanced",
                "--account",
                &account.id.to_string(),
                "--days",
                "0",
                "--export",
                "json",
                "--output",
                "/tmp",
            ]);
        dispatcher.metrics_advanced(failed_export.subcommand_matches("advanced").unwrap());
    }

    #[test]
    fn test_grade_show_and_summary_cover_text_json_regrade_and_empty_paths() {
        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (account, closed) = seed_closed_target_trade(&mut dispatcher);
        let trade_id = closed.id.to_string();

        let show_matches = GradeCommandBuilder::new()
            .show()
            .build()
            .get_matches_from(["grade", "show", &trade_id]);
        let show = show_matches
            .subcommand_matches("show")
            .expect("show subcommand");
        dispatcher
            .grade_show(show, ReportOutputFormat::Text)
            .expect("grade show text should compute a grade");
        dispatcher
            .grade_show(show, ReportOutputFormat::Json)
            .expect("grade show json should reuse the existing grade");

        let mismatched_weights = GradeCommandBuilder::new().show().build().get_matches_from([
            "grade",
            "show",
            &trade_id,
            "--weights",
            "25,25,25,25",
        ]);
        let mismatch_error = dispatcher
            .grade_show(
                mismatched_weights
                    .subcommand_matches("show")
                    .expect("show subcommand"),
                ReportOutputFormat::Json,
            )
            .expect_err("mismatched weights should require --regrade");
        assert!(mismatch_error.to_string().contains("weights_mismatch"));

        let regrade_matches = GradeCommandBuilder::new().show().build().get_matches_from([
            "grade",
            "show",
            &trade_id,
            "--weights",
            "25,25,25,25",
            "--regrade",
        ]);
        dispatcher
            .grade_show(
                regrade_matches
                    .subcommand_matches("show")
                    .expect("show subcommand"),
                ReportOutputFormat::Text,
            )
            .expect("regrade with explicit weights should succeed");

        let stored_grade_matches = GradeCommandBuilder::new().show().build().get_matches_from([
            "grade",
            "show",
            &Uuid::new_v4().to_string(),
        ]);
        let mut stored_grade_dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::existing_empty_trade_grade(),
        );
        stored_grade_dispatcher
            .grade_show(
                stored_grade_matches
                    .subcommand_matches("show")
                    .expect("show subcommand"),
                ReportOutputFormat::Text,
            )
            .expect("stored grade with no recommendations should render");

        let stored_grade_matching_weights =
            GradeCommandBuilder::new().show().build().get_matches_from([
                "grade",
                "show",
                &Uuid::new_v4().to_string(),
                "--weights",
                "40,30,20,10",
            ]);
        stored_grade_dispatcher
            .grade_show(
                stored_grade_matching_weights
                    .subcommand_matches("show")
                    .expect("show subcommand"),
                ReportOutputFormat::Json,
            )
            .expect("stored grade should accept matching explicit weights");

        let regrade_failure_matches = GradeCommandBuilder::new().show().build().get_matches_from([
            "grade",
            "show",
            &Uuid::new_v4().to_string(),
            "--regrade",
        ]);
        let mut regrade_failure_dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::existing_trade_grade_then_trade_failure(),
        );
        let error = regrade_failure_dispatcher
            .grade_show(
                regrade_failure_matches
                    .subcommand_matches("show")
                    .expect("show subcommand"),
                ReportOutputFormat::Json,
            )
            .expect_err("regrade should surface grade computation failures");
        assert_eq!(error.code, "grade_compute_failed");

        let summary_matches = GradeCommandBuilder::new()
            .summary()
            .build()
            .get_matches_from([
                "grade",
                "summary",
                "--account",
                &account.id.to_string(),
                "--days",
                "365",
            ]);
        let summary = summary_matches
            .subcommand_matches("summary")
            .expect("summary subcommand");
        dispatcher
            .grade_summary(summary, ReportOutputFormat::Text)
            .expect("grade summary text should include non-empty distribution");
        dispatcher
            .grade_summary(summary, ReportOutputFormat::Json)
            .expect("grade summary json should include non-empty distribution");

        let mut ungraded_dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (ungraded_account, _) = seed_closed_target_trade(&mut ungraded_dispatcher);
        let ungraded_summary_matches = GradeCommandBuilder::new()
            .summary()
            .build()
            .get_matches_from([
                "grade",
                "summary",
                "--account",
                &ungraded_account.id.to_string(),
                "--days",
                "365",
            ]);
        ungraded_dispatcher
            .grade_summary(
                ungraded_summary_matches
                    .subcommand_matches("summary")
                    .expect("summary subcommand"),
                ReportOutputFormat::Json,
            )
            .expect("grade summary should compute missing grades");

        let mut grade_failure_dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::closed_trade_without_grade_then_trade_read_failure(),
        );
        let grade_failure_account = Uuid::new_v4().to_string();
        let grade_failure_summary = GradeCommandBuilder::new()
            .summary()
            .build()
            .get_matches_from([
                "grade",
                "summary",
                "--account",
                &grade_failure_account,
                "--days",
                "365",
            ]);
        let error = grade_failure_dispatcher
            .grade_summary(
                grade_failure_summary
                    .subcommand_matches("summary")
                    .expect("summary subcommand"),
                ReportOutputFormat::Json,
            )
            .expect_err("grade summary should surface grade computation failures");
        assert_eq!(error.code, "grade_compute_failed");

        let mut empty_dispatcher = test_dispatcher();
        let empty_summary_matches = GradeCommandBuilder::new()
            .summary()
            .build()
            .get_matches_from(["grade", "summary", "--days", "30"]);
        empty_dispatcher
            .grade_summary(
                empty_summary_matches
                    .subcommand_matches("summary")
                    .expect("summary subcommand"),
                ReportOutputFormat::Text,
            )
            .expect("empty grade summary text should succeed");
    }

    #[test]
    fn test_trade_in_window() {
        let trade = Trade {
            updated_at: Utc
                .with_ymd_and_hms(2026, 2, 10, 12, 0, 0)
                .unwrap()
                .naive_utc(),
            ..Trade::default()
        };
        let from = Some(
            Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0)
                .unwrap()
                .date_naive(),
        );
        let to = Some(
            Utc.with_ymd_and_hms(2026, 2, 20, 0, 0, 0)
                .unwrap()
                .date_naive(),
        );
        assert!(ArgDispatcher::trade_in_window(&trade, from, to));
    }

    #[test]
    fn test_trade_in_window_rejects_before_from_and_after_to() {
        let trade = Trade {
            updated_at: Utc
                .with_ymd_and_hms(2026, 2, 10, 12, 0, 0)
                .unwrap()
                .naive_utc(),
            ..Trade::default()
        };
        let after_trade = Some(
            Utc.with_ymd_and_hms(2026, 2, 11, 0, 0, 0)
                .unwrap()
                .date_naive(),
        );
        let before_trade = Some(
            Utc.with_ymd_and_hms(2026, 2, 9, 0, 0, 0)
                .unwrap()
                .date_naive(),
        );
        assert!(!ArgDispatcher::trade_in_window(&trade, after_trade, None));
        assert!(!ArgDispatcher::trade_in_window(&trade, None, before_trade));
        assert!(!ArgDispatcher::trade_in_window(
            &trade,
            Some(NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()),
            before_trade
        ));
    }

    #[test]
    fn test_max_drawdown_amount() {
        let mut t1 = Trade::default();
        t1.balance.total_performance = dec!(100);
        let mut t2 = Trade::default();
        t2.balance.total_performance = dec!(-40);
        let mut t3 = Trade::default();
        t3.balance.total_performance = dec!(-30);
        let dd = ArgDispatcher::max_drawdown_amount(&[t1, t2, t3]);
        assert_eq!(dd, dec!(70));
    }

    #[test]
    fn test_max_drawdown_amount_no_drawdown_case() {
        let mut t1 = Trade::default();
        t1.balance.total_performance = dec!(10);
        let mut t2 = Trade::default();
        t2.balance.total_performance = dec!(15);
        let mut t3 = Trade::default();
        t3.balance.total_performance = dec!(5);
        let dd = ArgDispatcher::max_drawdown_amount(&[t1, t2, t3]);
        assert_eq!(dd, dec!(0));
    }

    #[test]
    fn test_report_calmar_ratio_uses_realized_transaction_baseline() {
        let account_id = Uuid::new_v4();
        let trade_id = Uuid::new_v4();
        let base = Utc
            .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .expect("valid timestamp")
            .naive_utc();

        let mut deposit = model::transaction::Transaction::new(
            account_id,
            TransactionCategory::Deposit,
            &Currency::USD,
            dec!(1000),
        );
        deposit.created_at = base;
        deposit.updated_at = base;

        let mut payout = model::transaction::Transaction::new(
            account_id,
            TransactionCategory::PaymentFromTrade(trade_id),
            &Currency::USD,
            dec!(120),
        );
        payout.created_at = base + chrono::Duration::days(30);
        payout.updated_at = payout.created_at;

        let transactions = vec![deposit, payout];
        let drawdown_metrics = ArgDispatcher::drawdown_metrics_for_transactions(&transactions);

        let mut trade = Trade {
            created_at: base,
            updated_at: base + chrono::Duration::days(30),
            status: Status::ClosedTarget,
            ..Trade::default()
        };
        trade.balance.total_performance = dec!(120);

        let calmar = ArgDispatcher::report_calmar_ratio(&[trade], &transactions, &drawdown_metrics);
        assert_eq!(calmar, None);

        let calmar = ArgDispatcher::report_calmar_ratio(
            &[Trade {
                balance: model::trade::TradeBalance {
                    total_performance: dec!(120),
                    ..Trade::default().balance
                },
                created_at: base,
                updated_at: base + chrono::Duration::days(30),
                status: Status::ClosedTarget,
                ..Trade::default()
            }],
            &transactions,
            &core::calculators_drawdown::DrawdownMetrics {
                current_equity: dec!(1120),
                peak_equity: dec!(1120),
                current_drawdown: dec!(0),
                current_drawdown_percentage: dec!(0),
                max_drawdown: dec!(80),
                max_drawdown_percentage: dec!(8),
                max_drawdown_date: None,
                recovery_from_max: dec!(0),
                recovery_percentage: dec!(0),
                days_since_peak: 0,
                days_in_drawdown: 0,
            },
        )
        .expect("calmar ratio");

        assert!((calmar - dec!(18.25)).abs() < dec!(0.0001));
    }

    #[test]
    fn test_report_calmar_ratio_returns_none_without_realized_equity_baseline() {
        let drawdown_metrics = core::calculators_drawdown::DrawdownMetrics {
            current_equity: dec!(0),
            peak_equity: dec!(0),
            current_drawdown: dec!(0),
            current_drawdown_percentage: dec!(0),
            max_drawdown: dec!(0),
            max_drawdown_percentage: dec!(0),
            max_drawdown_date: None,
            recovery_from_max: dec!(0),
            recovery_percentage: dec!(0),
            days_since_peak: 0,
            days_in_drawdown: 0,
        };
        let mut trade = Trade {
            status: Status::ClosedTarget,
            ..Trade::default()
        };
        trade.balance.total_performance = dec!(100);

        let calmar = ArgDispatcher::report_calmar_ratio(&[trade], &[], &drawdown_metrics);

        assert_eq!(calmar, None);
    }

    #[test]
    fn test_parse_bar_timeframe_valid_values() {
        assert!(matches!(
            ArgDispatcher::parse_bar_timeframe("1m", ReportOutputFormat::Json).unwrap(),
            model::BarTimeframe::OneMinute
        ));
        assert!(matches!(
            ArgDispatcher::parse_bar_timeframe("1h", ReportOutputFormat::Json).unwrap(),
            model::BarTimeframe::OneHour
        ));
        assert!(matches!(
            ArgDispatcher::parse_bar_timeframe("1d", ReportOutputFormat::Json).unwrap(),
            model::BarTimeframe::OneDay
        ));
    }

    #[test]
    fn test_parse_bar_timeframe_invalid_value() {
        let error = ArgDispatcher::parse_bar_timeframe("5m", ReportOutputFormat::Json).unwrap_err();
        assert!(error.to_string().contains("invalid_timeframe"));
    }

    #[test]
    fn test_parse_market_data_channels_valid_and_invalid_values() {
        let channels = ArgDispatcher::parse_market_data_channels(
            "quotes,trades,bars,quotes",
            ReportOutputFormat::Json,
        )
        .expect("channels should parse");
        assert_eq!(channels.len(), 3);
        assert!(channels.contains(&model::MarketDataChannel::Quotes));
        assert!(channels.contains(&model::MarketDataChannel::Trades));
        assert!(channels.contains(&model::MarketDataChannel::Bars));

        let error = ArgDispatcher::parse_market_data_channels("foo", ReportOutputFormat::Json)
            .expect_err("invalid channel should fail");
        assert!(error.to_string().contains("invalid_channel"));
    }

    #[test]
    fn test_parse_market_data_stream_request_happy_path() {
        let matches = market_stream_matches(&[
            "--symbols",
            " aapl, msft ",
            "--channels",
            "quotes,trades,quotes",
            "--max-events",
            "12",
            "--timeout-seconds",
            "7",
        ]);

        let request =
            ArgDispatcher::parse_market_data_stream_request(&matches, ReportOutputFormat::Json)
                .expect("request should parse");

        assert_eq!(
            request.symbols,
            vec!["AAPL".to_string(), "MSFT".to_string()]
        );
        assert_eq!(request.max_events, 12);
        assert_eq!(request.timeout_seconds, 7);
        assert_eq!(
            request.channels,
            vec![MarketDataChannel::Quotes, MarketDataChannel::Trades]
        );
        assert_eq!(request.channels_raw, "quotes,trades,quotes");
    }

    #[test]
    fn test_parse_market_data_stream_request_rejects_missing_or_invalid_arguments() {
        let missing_symbols = market_stream_matches(&[
            "--channels",
            "quotes",
            "--max-events",
            "12",
            "--timeout-seconds",
            "7",
        ]);
        let error = ArgDispatcher::parse_market_data_stream_request(
            &missing_symbols,
            ReportOutputFormat::Json,
        )
        .expect_err("missing symbols should fail");
        assert!(error.to_string().contains("missing_argument"));

        let missing_channels = market_stream_matches(&[
            "--symbols",
            "AAPL",
            "--max-events",
            "12",
            "--timeout-seconds",
            "7",
        ]);
        let error = ArgDispatcher::parse_market_data_stream_request(
            &missing_channels,
            ReportOutputFormat::Json,
        )
        .expect_err("missing channels should fail");
        assert!(error.to_string().contains("missing_argument"));

        let missing_max_events = market_stream_matches(&[
            "--symbols",
            "AAPL",
            "--channels",
            "quotes",
            "--timeout-seconds",
            "7",
        ]);
        let error = ArgDispatcher::parse_market_data_stream_request(
            &missing_max_events,
            ReportOutputFormat::Json,
        )
        .expect_err("missing max-events should fail");
        assert!(error.to_string().contains("missing_argument"));

        let missing_timeout = market_stream_matches(&[
            "--symbols",
            "AAPL",
            "--channels",
            "quotes",
            "--max-events",
            "12",
        ]);
        let error = ArgDispatcher::parse_market_data_stream_request(
            &missing_timeout,
            ReportOutputFormat::Json,
        )
        .expect_err("missing timeout should fail");
        assert!(error.to_string().contains("missing_argument"));

        let invalid_max = market_stream_matches(&[
            "--symbols",
            "AAPL",
            "--channels",
            "quotes",
            "--max-events",
            "not-int",
            "--timeout-seconds",
            "7",
        ]);
        let error =
            ArgDispatcher::parse_market_data_stream_request(&invalid_max, ReportOutputFormat::Json)
                .expect_err("invalid max-events should fail");
        assert!(error.to_string().contains("invalid_argument"));

        let invalid_timeout = market_stream_matches(&[
            "--symbols",
            "AAPL",
            "--channels",
            "quotes",
            "--max-events",
            "10",
            "--timeout-seconds",
            "soon",
        ]);
        let error = ArgDispatcher::parse_market_data_stream_request(
            &invalid_timeout,
            ReportOutputFormat::Json,
        )
        .expect_err("invalid timeout should fail");
        assert!(error.to_string().contains("invalid_argument"));

        let invalid_symbols = market_stream_matches(&[
            "--symbols",
            ", ,",
            "--channels",
            "quotes",
            "--max-events",
            "10",
            "--timeout-seconds",
            "7",
        ]);
        let error = ArgDispatcher::parse_market_data_stream_request(
            &invalid_symbols,
            ReportOutputFormat::Json,
        )
        .expect_err("empty symbols should fail");
        assert!(error
            .to_string()
            .contains("At least one symbol is required in --symbols"));
    }

    #[test]
    fn test_market_data_handler_factory_routes_to_expected_handler_behavior() {
        let mut dispatcher = test_dispatcher();

        let snapshot = market_data_command_matches(&[
            "snapshot",
            "--account",
            "not-a-uuid",
            "--symbol",
            "AAPL",
        ]);
        let (handler, sub) = ArgDispatcher::market_data_handler_for(
            crate::command_routing::parse_market_data_subcommand(&snapshot),
        );
        let error = handler
            .execute(&mut dispatcher, sub, ReportOutputFormat::Json)
            .expect_err("invalid account should fail");
        assert!(error.to_string().contains("account_not_found"));

        let bars = market_data_command_matches(&[
            "bars",
            "--account",
            "not-a-uuid",
            "--symbol",
            "AAPL",
            "--timeframe",
            "1m",
            "--start",
            "2026-02-24T10:00:00Z",
            "--end",
            "2026-02-24T11:00:00Z",
        ]);
        let (handler, sub) = ArgDispatcher::market_data_handler_for(
            crate::command_routing::parse_market_data_subcommand(&bars),
        );
        let error = handler
            .execute(&mut dispatcher, sub, ReportOutputFormat::Json)
            .expect_err("invalid account should fail");
        assert!(error.to_string().contains("account_not_found"));

        let stream = market_data_command_matches(&[
            "stream",
            "--account",
            "not-a-uuid",
            "--symbols",
            "AAPL",
            "--channels",
            "quotes",
            "--max-events",
            "1",
            "--timeout-seconds",
            "1",
        ]);
        let (handler, sub) = ArgDispatcher::market_data_handler_for(
            crate::command_routing::parse_market_data_subcommand(&stream),
        );
        let error = handler
            .execute(&mut dispatcher, sub, ReportOutputFormat::Json)
            .expect_err("invalid account should fail");
        assert!(error.to_string().contains("account_not_found"));

        let quote =
            market_data_command_matches(&["quote", "--account", "not-a-uuid", "--symbol", "AAPL"]);
        let (handler, sub) = ArgDispatcher::market_data_handler_for(
            crate::command_routing::parse_market_data_subcommand(&quote),
        );
        let error = handler
            .execute(&mut dispatcher, sub, ReportOutputFormat::Json)
            .expect_err("invalid account should fail");
        assert!(error.to_string().contains("account_not_found"));

        let trade =
            market_data_command_matches(&["trade", "--account", "not-a-uuid", "--symbol", "AAPL"]);
        let (handler, sub) = ArgDispatcher::market_data_handler_for(
            crate::command_routing::parse_market_data_subcommand(&trade),
        );
        let error = handler
            .execute(&mut dispatcher, sub, ReportOutputFormat::Json)
            .expect_err("invalid account should fail");
        assert!(error.to_string().contains("account_not_found"));

        let session = market_data_command_matches(&[
            "session",
            "--account",
            "not-a-uuid",
            "--symbol",
            "AAPL",
        ]);
        let (handler, sub) = ArgDispatcher::market_data_handler_for(
            crate::command_routing::parse_market_data_subcommand(&session),
        );
        let error = handler
            .execute(&mut dispatcher, sub, ReportOutputFormat::Json)
            .expect_err("invalid account should fail");
        assert!(error.to_string().contains("account_not_found"));
    }

    #[test]
    fn test_market_data_dispatch_validates_missing_required_arguments() {
        let mut dispatcher = test_dispatcher();
        let market_matches = |args: &[&str]| {
            let mut argv = vec!["test"];
            argv.extend_from_slice(args);
            Command::new("test")
                .arg(Arg::new("account").long("account"))
                .arg(Arg::new("symbol").long("symbol"))
                .arg(Arg::new("symbols").long("symbols"))
                .arg(Arg::new("channels").long("channels"))
                .arg(Arg::new("max-events").long("max-events"))
                .arg(Arg::new("timeout-seconds").long("timeout-seconds"))
                .arg(Arg::new("timeframe").long("timeframe"))
                .arg(Arg::new("start").long("start"))
                .arg(Arg::new("end").long("end"))
                .get_matches_from(argv)
        };

        let snapshot_missing_account = market_matches(&["--symbol", "AAPL"]);
        let error = dispatcher
            .market_data_snapshot(&snapshot_missing_account, ReportOutputFormat::Json)
            .expect_err("snapshot without account should fail");
        assert!(error.to_string().contains("missing_argument"));

        let snapshot_missing_symbol = market_matches(&["--account", "paper-main"]);
        let error = dispatcher
            .market_data_snapshot(&snapshot_missing_symbol, ReportOutputFormat::Json)
            .expect_err("snapshot without symbol should fail");
        assert!(error.to_string().contains("missing_argument"));

        let quote_missing_account = market_matches(&["--symbol", "AAPL"]);
        let error = dispatcher
            .market_data_quote(&quote_missing_account, ReportOutputFormat::Json)
            .expect_err("quote without account should fail");
        assert!(error.to_string().contains("missing_argument"));

        let trade_missing_symbol = market_matches(&["--account", "paper-main"]);
        let error = dispatcher
            .market_data_trade(&trade_missing_symbol, ReportOutputFormat::Json)
            .expect_err("trade without symbol should fail");
        assert!(error.to_string().contains("missing_argument"));

        let stream_missing_account = market_matches(&[
            "--symbols",
            "AAPL",
            "--channels",
            "quotes",
            "--max-events",
            "1",
            "--timeout-seconds",
            "1",
        ]);
        let error = dispatcher
            .market_data_stream(&stream_missing_account, ReportOutputFormat::Json)
            .expect_err("stream without account should fail");
        assert!(error.to_string().contains("missing_argument"));

        let bars_missing_account = market_matches(&[
            "--symbol",
            "AAPL",
            "--timeframe",
            "1m",
            "--start",
            "2026-02-24T09:00:00Z",
            "--end",
            "2026-02-24T10:00:00Z",
        ]);
        let error = dispatcher
            .market_data_bars(&bars_missing_account, ReportOutputFormat::Json)
            .expect_err("bars without account should fail");
        assert!(error.to_string().contains("missing_argument"));

        let bars_missing_symbol = market_matches(&[
            "--account",
            "paper-main",
            "--timeframe",
            "1m",
            "--start",
            "2026-02-24T09:00:00Z",
            "--end",
            "2026-02-24T10:00:00Z",
        ]);
        let error = dispatcher
            .market_data_bars(&bars_missing_symbol, ReportOutputFormat::Json)
            .expect_err("bars without symbol should fail");
        assert!(error.to_string().contains("missing_argument"));

        let bars_missing_timeframe = market_matches(&[
            "--account",
            "paper-main",
            "--symbol",
            "AAPL",
            "--start",
            "2026-02-24T09:00:00Z",
            "--end",
            "2026-02-24T10:00:00Z",
        ]);
        let error = dispatcher
            .market_data_bars(&bars_missing_timeframe, ReportOutputFormat::Json)
            .expect_err("bars without timeframe should fail");
        assert!(error.to_string().contains("missing_argument"));

        let bars_reversed_range = market_matches(&[
            "--account",
            "paper-main",
            "--symbol",
            "AAPL",
            "--timeframe",
            "1m",
            "--start",
            "2026-02-24T10:00:00Z",
            "--end",
            "2026-02-24T09:00:00Z",
        ]);
        let error = dispatcher
            .market_data_bars(&bars_reversed_range, ReportOutputFormat::Json)
            .expect_err("reversed bars range should fail before account lookup");
        assert!(error.to_string().contains("invalid_time_range"));
    }

    #[test]
    fn test_report_handler_factory_routes_to_expected_handler_behavior() {
        let mut dispatcher = test_dispatcher();
        let invalid_account = "not-a-uuid";
        let commands: Vec<Vec<&str>> = vec![
            vec!["report", "performance", "--account", invalid_account],
            vec!["report", "drawdown", "--account", invalid_account],
            vec!["report", "risk", "--account", invalid_account],
            vec!["report", "concentration", "--account", invalid_account],
            vec!["report", "summary", "--account", invalid_account],
            vec!["report", "metrics", "--account", invalid_account],
            vec![
                "report",
                "attribution",
                "--account",
                invalid_account,
                "--by",
                "symbol",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ],
            vec![
                "report",
                "benchmark",
                "--account",
                invalid_account,
                "--benchmark",
                "SPY",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ],
            vec![
                "report",
                "timeline",
                "--account",
                invalid_account,
                "--granularity",
                "day",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ],
            vec![
                "report",
                "bias-summary",
                "--account",
                invalid_account,
                "--days",
                "7",
            ],
        ];

        for argv in commands {
            let matches = ReportCommandBuilder::new()
                .performance()
                .drawdown()
                .risk()
                .concentration()
                .summary()
                .metrics()
                .attribution()
                .benchmark()
                .timeline()
                .bias_summary()
                .build()
                .get_matches_from(argv);

            let (handler, sub) = ArgDispatcher::report_handler_for(
                crate::command_routing::parse_report_subcommand(&matches),
            );
            let error = handler
                .execute(&mut dispatcher, sub, ReportOutputFormat::Json)
                .expect_err("invalid account should fail");
            assert!(
                error.to_string().contains("invalid_account_id")
                    || error.to_string().contains("account_not_found")
            );
        }
    }

    #[test]
    fn test_market_snapshot_payload_includes_quote_trade_and_freshness() {
        let requested_at = Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 30).unwrap();
        let snapshot_as_of = Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 0).unwrap();
        let snapshot = MarketSnapshotV2 {
            symbol: "AAPL".to_string(),
            as_of: snapshot_as_of,
            last_price: dec!(201.12),
            volume: 123,
            open: dec!(200),
            high: dec!(202),
            low: dec!(199.5),
            quote: Some(MarketQuote {
                symbol: "AAPL".to_string(),
                as_of: snapshot_as_of,
                bid_price: dec!(201.1),
                bid_size: 4,
                ask_price: dec!(201.2),
                ask_size: 5,
            }),
            trade: Some(MarketTradeTick {
                symbol: "AAPL".to_string(),
                as_of: snapshot_as_of,
                price: dec!(201.12),
                size: 10,
            }),
            source: MarketSnapshotSource::QuoteTrade,
        };

        let payload = ArgDispatcher::market_snapshot_payload(Uuid::nil(), requested_at, &snapshot);
        assert_eq!(payload["report"], "market_data_snapshot");
        assert_eq!(payload["scope"]["symbol"], "AAPL");
        assert_eq!(payload["provenance"]["source_kind"], "quote_trade");
        assert_eq!(payload["provenance"]["fallback_used"], false);
        assert_eq!(payload["freshness"]["lag_seconds"], 30);
        assert_eq!(payload["data"]["quote"]["bid_price"], "201.1");
        assert_eq!(payload["data"]["trade"]["size"], 10);
    }

    #[test]
    fn test_market_data_stream_payload_normalizes_channels_and_counts() {
        let requested_at = Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 0).unwrap();
        let request = super::MarketDataStreamRequest {
            symbols: vec!["AAPL".to_string()],
            channels_raw: "quotes,trades".to_string(),
            channels: vec![MarketDataChannel::Quotes, MarketDataChannel::Trades],
            max_events: 2,
            timeout_seconds: 3,
        };
        let events = vec![
            MarketDataStreamEvent {
                channel: MarketDataChannel::Quotes,
                symbol: "AAPL".to_string(),
                as_of: Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 1).unwrap(),
                price: dec!(201.05),
                size: 2,
            },
            MarketDataStreamEvent {
                channel: MarketDataChannel::Trades,
                symbol: "AAPL".to_string(),
                as_of: Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 2).unwrap(),
                price: dec!(201.1),
                size: 6,
            },
        ];

        let payload =
            ArgDispatcher::market_data_stream_payload(Uuid::nil(), requested_at, &request, &events);
        assert_eq!(payload["report"], "market_data_stream");
        assert_eq!(payload["scope"]["symbols"][0], "AAPL");
        assert_eq!(payload["request"]["max_events"], 2);
        assert_eq!(payload["data"]["count"], 2);
        assert_eq!(payload["data"]["events"][0]["channel"], "quotes");
        assert_eq!(payload["data"]["events"][1]["channel"], "trades");
    }

    #[test]
    fn test_market_data_label_helpers_cover_all_sources_and_channels() {
        assert_eq!(
            ArgDispatcher::market_snapshot_source_label(&MarketSnapshotSource::QuoteTrade),
            "quote_trade"
        );
        assert_eq!(
            ArgDispatcher::market_snapshot_source_label(&MarketSnapshotSource::BarsFallback),
            "bars_fallback"
        );
        assert_eq!(
            ArgDispatcher::market_data_channel_text_label(MarketDataChannel::Bars),
            "bar"
        );
        assert_eq!(
            ArgDispatcher::market_data_channel_text_label(MarketDataChannel::Quotes),
            "quote"
        );
        assert_eq!(
            ArgDispatcher::market_data_channel_text_label(MarketDataChannel::Trades),
            "trade"
        );
        assert_eq!(
            ArgDispatcher::market_data_channel_json_label(MarketDataChannel::Bars),
            "bars"
        );
        assert_eq!(
            ArgDispatcher::market_data_channel_json_label(MarketDataChannel::Quotes),
            "quotes"
        );
        assert_eq!(
            ArgDispatcher::market_data_channel_json_label(MarketDataChannel::Trades),
            "trades"
        );
    }

    #[test]
    fn test_stub_broker_trade_operations_return_explicit_errors() {
        let broker = StubBroker::quote_trade_fixture();
        let trade = Trade::default();
        let account = model::Account::default();

        assert!(broker.submit_trade(&trade, &account).is_err());
        assert!(broker.sync_trade(&trade, &account).is_err());
        assert!(broker.close_trade(&trade, &account).is_err());
        assert!(broker.cancel_trade(&trade, &account).is_err());
        assert!(broker.modify_stop(&trade, &account, dec!(95)).is_err());
        assert!(broker.modify_target(&trade, &account, dec!(125)).is_err());
    }

    #[test]
    fn test_market_data_bars_payload_includes_request_and_ohlcv() {
        let requested_at = Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 0).unwrap();
        let start = Utc.with_ymd_and_hms(2026, 2, 27, 9, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 0).unwrap();
        let bars = vec![MarketBar {
            time: start,
            open: dec!(200),
            high: dec!(202),
            low: dec!(199),
            close: dec!(201),
            volume: 1000,
        }];

        let payload = ArgDispatcher::market_data_bars_payload(
            Uuid::nil(),
            requested_at,
            "AAPL",
            "1m",
            start,
            end,
            &bars,
        );
        assert_eq!(payload["report"], "market_data_bars");
        assert_eq!(payload["scope"]["symbol"], "AAPL");
        assert_eq!(payload["request"]["timeframe"], "1m");
        assert_eq!(payload["data"]["count"], 1);
        assert_eq!(payload["data"]["bars"][0]["close"], "201");
        assert_eq!(payload["data"]["bars"][0]["volume"], 1000);
    }

    #[test]
    fn test_parse_rfc3339_timestamp() {
        let matches = Command::new("test")
            .arg(Arg::new("start").long("start").required(true))
            .get_matches_from(["test", "--start", "2026-02-24T10:00:00Z"]);
        let parsed =
            ArgDispatcher::parse_rfc3339_timestamp(&matches, "start", ReportOutputFormat::Json)
                .unwrap();
        assert_eq!(parsed, Utc.with_ymd_and_hms(2026, 2, 24, 10, 0, 0).unwrap());
    }

    #[test]
    fn test_parse_rfc3339_timestamp_invalid() {
        let matches = Command::new("test")
            .arg(Arg::new("start").long("start").required(true))
            .get_matches_from(["test", "--start", "not-a-time"]);
        let error =
            ArgDispatcher::parse_rfc3339_timestamp(&matches, "start", ReportOutputFormat::Json)
                .unwrap_err();
        assert!(error.to_string().contains("invalid_timestamp"));
    }

    #[test]
    fn test_parse_rfc3339_timestamp_missing_argument() {
        let matches = Command::new("test")
            .arg(Arg::new("start").long("start"))
            .get_matches_from(["test"]);
        let error =
            ArgDispatcher::parse_rfc3339_timestamp(&matches, "start", ReportOutputFormat::Json)
                .expect_err("missing argument should fail");
        assert!(error.to_string().contains("missing_argument"));
        assert!(error.to_string().contains("--start"));
        assert!(error.already_printed());
    }

    #[test]
    fn test_parse_required_date_range_validates_format_and_order() {
        let valid = Command::new("test")
            .arg(Arg::new("from").long("from").required(true))
            .arg(Arg::new("to").long("to").required(true))
            .get_matches_from(["test", "--from", "2026-01-01", "--to", "2026-01-31"]);
        let (from, to) = ArgDispatcher::parse_required_date_range(&valid, ReportOutputFormat::Json)
            .expect("valid range should parse");
        assert_eq!(
            from,
            NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid literal date")
        );
        assert_eq!(
            to,
            NaiveDate::from_ymd_opt(2026, 1, 31).expect("valid literal date")
        );

        let invalid = Command::new("test")
            .arg(Arg::new("from").long("from").required(true))
            .arg(Arg::new("to").long("to").required(true))
            .get_matches_from(["test", "--from", "bad", "--to", "2026-01-31"]);
        let err = ArgDispatcher::parse_required_date_range(&invalid, ReportOutputFormat::Json)
            .expect_err("invalid date should fail");
        assert!(err.to_string().contains("invalid_date"));

        let reversed = Command::new("test")
            .arg(Arg::new("from").long("from").required(true))
            .arg(Arg::new("to").long("to").required(true))
            .get_matches_from(["test", "--from", "2026-02-01", "--to", "2026-01-31"]);
        let err = ArgDispatcher::parse_required_date_range(&reversed, ReportOutputFormat::Json)
            .expect_err("reversed date range should fail");
        assert!(err.to_string().contains("invalid_date_range"));
    }

    #[test]
    fn test_transaction_cashflow_delta_sign_rules() {
        let account_id = Uuid::new_v4();
        let trade_id = Uuid::new_v4();
        let amount = dec!(10);
        let mk = |category: model::TransactionCategory| {
            model::Transaction::new(account_id, category, &Currency::USD, amount)
        };

        assert_eq!(
            ArgDispatcher::transaction_cashflow_delta(&mk(model::TransactionCategory::Deposit)),
            dec!(10)
        );
        assert_eq!(
            ArgDispatcher::transaction_cashflow_delta(&mk(
                model::TransactionCategory::CloseTarget(trade_id)
            )),
            dec!(10)
        );
        assert_eq!(
            ArgDispatcher::transaction_cashflow_delta(&mk(model::TransactionCategory::Withdrawal)),
            dec!(-10)
        );
        assert_eq!(
            ArgDispatcher::transaction_cashflow_delta(&mk(model::TransactionCategory::FeeClose(
                trade_id
            ))),
            dec!(-10)
        );
        assert_eq!(
            ArgDispatcher::transaction_cashflow_delta(&mk(model::TransactionCategory::FundTrade(
                trade_id
            ))),
            dec!(0)
        );
        assert_eq!(
            ArgDispatcher::transaction_cashflow_delta(&mk(
                model::TransactionCategory::PaymentFromTrade(trade_id)
            )),
            dec!(0)
        );
    }

    #[test]
    fn test_new_report_handlers_cover_json_and_validation_paths() {
        let mut dispatcher = test_dispatcher();
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);
        let account_id = account.id.to_string();

        let attribution = Command::new("test")
            .arg(Arg::new("account").long("account").required(true))
            .arg(Arg::new("by").long("by").required(true))
            .arg(Arg::new("from").long("from").required(true))
            .arg(Arg::new("to").long("to").required(true))
            .get_matches_from([
                "test",
                "--account",
                &account_id,
                "--by",
                "symbol",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ]);
        dispatcher
            .attribution_report(&attribution, ReportOutputFormat::Json)
            .expect("attribution report should succeed");

        let attribution_bad_by = Command::new("test")
            .arg(Arg::new("account").long("account").required(true))
            .arg(Arg::new("by").long("by").required(true))
            .arg(Arg::new("from").long("from").required(true))
            .arg(Arg::new("to").long("to").required(true))
            .get_matches_from([
                "test",
                "--account",
                &account_id,
                "--by",
                "bad",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ]);
        let err = dispatcher
            .attribution_report(&attribution_bad_by, ReportOutputFormat::Json)
            .expect_err("invalid attribution dimension should fail");
        assert!(err.to_string().contains("invalid_argument"));

        let timeline = Command::new("test")
            .arg(Arg::new("account").long("account").required(true))
            .arg(Arg::new("granularity").long("granularity").required(true))
            .arg(Arg::new("from").long("from").required(true))
            .arg(Arg::new("to").long("to").required(true))
            .get_matches_from([
                "test",
                "--account",
                &account_id,
                "--granularity",
                "day",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ]);
        dispatcher
            .timeline_report(&timeline, ReportOutputFormat::Json)
            .expect("timeline report should succeed");

        let timeline_bad = Command::new("test")
            .arg(Arg::new("account").long("account").required(true))
            .arg(Arg::new("granularity").long("granularity").required(true))
            .arg(Arg::new("from").long("from").required(true))
            .arg(Arg::new("to").long("to").required(true))
            .get_matches_from([
                "test",
                "--account",
                &account_id,
                "--granularity",
                "bad",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ]);
        let err = dispatcher
            .timeline_report(&timeline_bad, ReportOutputFormat::Json)
            .expect_err("invalid timeline granularity should fail");
        assert!(err.to_string().contains("invalid_argument"));

        let benchmark = Command::new("test")
            .arg(Arg::new("account").long("account").required(true))
            .arg(Arg::new("benchmark").long("benchmark").required(true))
            .arg(Arg::new("from").long("from").required(true))
            .arg(Arg::new("to").long("to").required(true))
            .get_matches_from([
                "test",
                "--account",
                &account_id,
                "--benchmark",
                "SPY",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ]);
        let err = dispatcher
            .benchmark_report(&benchmark, ReportOutputFormat::Json)
            .expect_err("benchmark fetch should fail with default test broker");
        assert!(err.to_string().contains("benchmark_data_failed"));
    }

    #[test]
    fn test_parse_market_data_channels_deduplicates_and_rejects_empty_input() {
        let parsed = ArgDispatcher::parse_market_data_channels(
            " quotes, bars,quotes ,trades,bars ",
            ReportOutputFormat::Json,
        )
        .expect("channels parse");
        assert_eq!(
            parsed,
            vec![
                MarketDataChannel::Quotes,
                MarketDataChannel::Bars,
                MarketDataChannel::Trades
            ]
        );

        let error = ArgDispatcher::parse_market_data_channels(" , , ", ReportOutputFormat::Json)
            .expect_err("empty channels must fail");
        assert!(error.to_string().contains("invalid_channel"));
        assert!(error
            .to_string()
            .contains("At least one channel is required"));
    }

    #[test]
    fn test_market_data_dispatch_paths_succeed_with_stub_broker() {
        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::quote_trade_fixture()));
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);
        let account_id = account.id.to_string();

        let snapshot = market_data_command_matches(&[
            "snapshot",
            "--account",
            &account_id,
            "--symbol",
            "aapl",
            "--format",
            "json",
        ]);
        dispatcher
            .dispatch_market_data(&snapshot)
            .expect("snapshot dispatch should succeed");
        let snapshot_text = market_data_command_matches(&[
            "snapshot",
            "--account",
            &account_id,
            "--symbol",
            "aapl",
        ]);
        dispatcher
            .dispatch_market_data(&snapshot_text)
            .expect("text snapshot dispatch should succeed");

        let quote = market_data_command_matches(&[
            "quote",
            "--account",
            &account_id,
            "--symbol",
            "AAPL",
            "--format",
            "json",
        ]);
        dispatcher
            .dispatch_market_data(&quote)
            .expect("quote dispatch should succeed");
        let quote_text =
            market_data_command_matches(&["quote", "--account", &account_id, "--symbol", "AAPL"]);
        dispatcher
            .dispatch_market_data(&quote_text)
            .expect("text quote dispatch should succeed");

        let trade = market_data_command_matches(&[
            "trade",
            "--account",
            &account_id,
            "--symbol",
            "AAPL",
            "--format",
            "json",
        ]);
        dispatcher
            .dispatch_market_data(&trade)
            .expect("trade dispatch should succeed");
        let trade_text =
            market_data_command_matches(&["trade", "--account", &account_id, "--symbol", "AAPL"]);
        dispatcher
            .dispatch_market_data(&trade_text)
            .expect("text trade dispatch should succeed");

        let session = market_data_command_matches(&[
            "session",
            "--account",
            &account_id,
            "--symbol",
            "AAPL",
            "--format",
            "json",
        ]);
        dispatcher
            .dispatch_market_data(&session)
            .expect("session dispatch should succeed");
        let session_text =
            market_data_command_matches(&["session", "--account", &account_id, "--symbol", "AAPL"]);
        dispatcher
            .dispatch_market_data(&session_text)
            .expect("text session dispatch should succeed");

        let stream = market_data_command_matches(&[
            "stream",
            "--account",
            &account_id,
            "--symbols",
            "AAPL,MSFT",
            "--channels",
            "quotes,trades",
            "--max-events",
            "5",
            "--timeout-seconds",
            "2",
            "--format",
            "json",
        ]);
        dispatcher
            .dispatch_market_data(&stream)
            .expect("stream dispatch should succeed");
        let stream_text = market_data_command_matches(&[
            "stream",
            "--account",
            &account_id,
            "--symbols",
            "AAPL,MSFT",
            "--channels",
            "quotes,trades",
            "--max-events",
            "5",
            "--timeout-seconds",
            "2",
        ]);
        dispatcher
            .dispatch_market_data(&stream_text)
            .expect("text stream dispatch should succeed");

        let bars = market_data_command_matches(&[
            "bars",
            "--account",
            &account_id,
            "--symbol",
            "AAPL",
            "--timeframe",
            "1m",
            "--start",
            "2026-02-24T09:00:00Z",
            "--end",
            "2026-02-24T11:00:00Z",
            "--format",
            "json",
        ]);
        dispatcher
            .dispatch_market_data(&bars)
            .expect("bars dispatch should succeed");
        let bars_text = market_data_command_matches(&[
            "bars",
            "--account",
            &account_id,
            "--symbol",
            "AAPL",
            "--timeframe",
            "1m",
            "--start",
            "2026-02-24T09:00:00Z",
            "--end",
            "2026-02-24T11:00:00Z",
        ]);
        dispatcher
            .dispatch_market_data(&bars_text)
            .expect("text bars dispatch should succeed");
    }

    #[test]
    fn test_market_data_failure_paths_and_long_bars_are_reported() {
        let mut snapshot_failure_broker = StubBroker::quote_trade_fixture();
        snapshot_failure_broker.quote = None;
        snapshot_failure_broker.trade = None;
        snapshot_failure_broker.fail_bars = true;
        let mut dispatcher = test_dispatcher_with_broker(Box::new(snapshot_failure_broker));
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);
        let account_id = account.id.to_string();
        let quote = market_data_command_matches(&[
            "quote",
            "--account",
            &account_id,
            "--symbol",
            "AAPL",
            "--format",
            "json",
        ]);
        let error = dispatcher
            .dispatch_market_data(&quote)
            .expect_err("snapshot failure should fail quote handler");
        assert!(error.to_string().contains("market_data_snapshot_failed"));

        let mut stream_failure_broker = StubBroker::quote_trade_fixture();
        stream_failure_broker.fail_stream = true;
        let mut dispatcher = test_dispatcher_with_broker(Box::new(stream_failure_broker));
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);
        let account_id = account.id.to_string();
        let stream = market_data_command_matches(&[
            "stream",
            "--account",
            &account_id,
            "--symbols",
            "AAPL",
            "--channels",
            "quotes",
            "--max-events",
            "1",
            "--timeout-seconds",
            "1",
            "--format",
            "json",
        ]);
        let error = dispatcher
            .dispatch_market_data(&stream)
            .expect_err("stream broker failure should fail");
        assert!(error.to_string().contains("market_data_stream_failed"));

        let mut bars_failure_broker = StubBroker::quote_trade_fixture();
        bars_failure_broker.fail_bars = true;
        let mut dispatcher = test_dispatcher_with_broker(Box::new(bars_failure_broker));
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);
        let account_id = account.id.to_string();
        let bars = market_data_command_matches(&[
            "bars",
            "--account",
            &account_id,
            "--symbol",
            "AAPL",
            "--timeframe",
            "1m",
            "--start",
            "2026-02-24T09:00:00Z",
            "--end",
            "2026-02-24T11:00:00Z",
            "--format",
            "json",
        ]);
        let error = dispatcher
            .dispatch_market_data(&bars)
            .expect_err("bars broker failure should fail");
        assert!(error.to_string().contains("market_data_bars_failed"));

        let mut long_bars_broker = StubBroker::quote_trade_fixture();
        let start = Utc
            .with_ymd_and_hms(2026, 2, 24, 9, 0, 0)
            .single()
            .expect("valid fixture timestamp");
        long_bars_broker.bars = (0_u32..11)
            .map(|offset| MarketBar {
                time: start + chrono::Duration::minutes(i64::from(offset)),
                open: dec!(200),
                high: dec!(202),
                low: dec!(199),
                close: dec!(201),
                volume: 1_000 + u64::from(offset),
            })
            .collect();
        let mut dispatcher = test_dispatcher_with_broker(Box::new(long_bars_broker));
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);
        let account_id = account.id.to_string();
        let bars_text = market_data_command_matches(&[
            "bars",
            "--account",
            &account_id,
            "--symbol",
            "AAPL",
            "--timeframe",
            "1m",
            "--start",
            "2026-02-24T09:00:00Z",
            "--end",
            "2026-02-24T11:00:00Z",
        ]);
        dispatcher
            .dispatch_market_data(&bars_text)
            .expect("long text bars should be truncated after preview");
    }

    #[test]
    fn test_market_data_quote_and_trade_error_when_snapshot_lacks_fields() {
        let mut stub = StubBroker::quote_trade_fixture();
        stub.quote = None;
        stub.trade = None;
        let mut dispatcher = test_dispatcher_with_broker(Box::new(stub));
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);
        let account_id = account.id.to_string();

        let quote = market_data_command_matches(&[
            "quote",
            "--account",
            &account_id,
            "--symbol",
            "AAPL",
            "--format",
            "json",
        ]);
        let quote_error = dispatcher
            .dispatch_market_data(&quote)
            .expect_err("quote should fail when snapshot lacks quote");
        assert!(quote_error
            .to_string()
            .contains("market_data_quote_unavailable"));

        let trade = market_data_command_matches(&[
            "trade",
            "--account",
            &account_id,
            "--symbol",
            "AAPL",
            "--format",
            "json",
        ]);
        let trade_error = dispatcher
            .dispatch_market_data(&trade)
            .expect_err("trade should fail when snapshot lacks trade");
        assert!(trade_error
            .to_string()
            .contains("market_data_trade_unavailable"));
    }

    #[test]
    fn test_apply_level_rule_update_validates_inputs_and_updates_fields() {
        let mut rules = LevelAdjustmentRules::default();
        ArgDispatcher::apply_level_rule_update(
            &mut rules,
            "upgrade_win_rate_pct",
            "72.5",
            ReportOutputFormat::Json,
        )
        .expect("decimal update should work");
        assert_eq!(rules.upgrade_win_rate_pct, dec!(72.5));

        ArgDispatcher::apply_level_rule_update(
            &mut rules,
            "upgrade_consecutive_wins",
            "4",
            ReportOutputFormat::Json,
        )
        .expect("u32 update should work");
        assert_eq!(rules.upgrade_consecutive_wins, 4);

        ArgDispatcher::apply_level_rule_update(
            &mut rules,
            "min_trades_at_level_for_upgrade",
            "9",
            ReportOutputFormat::Json,
        )
        .expect("minimum trade count update should work");
        assert_eq!(rules.min_trades_at_level_for_upgrade, 9);

        let invalid_key = ArgDispatcher::apply_level_rule_update(
            &mut rules,
            "not-a-real-key",
            "1",
            ReportOutputFormat::Json,
        )
        .expect_err("unknown key should fail");
        assert!(invalid_key.to_string().contains("invalid_rule_key"));

        let invalid_decimal = ArgDispatcher::apply_level_rule_update(
            &mut rules,
            "upgrade_win_rate_pct",
            "abc",
            ReportOutputFormat::Json,
        )
        .expect_err("invalid decimal should fail");
        assert!(invalid_decimal.to_string().contains("invalid_rule_value"));

        let invalid_integer = ArgDispatcher::apply_level_rule_update(
            &mut rules,
            "max_changes_in_30_days",
            "-1",
            ReportOutputFormat::Json,
        )
        .expect_err("invalid integer should fail");
        assert!(invalid_integer.to_string().contains("invalid_rule_value"));
    }

    #[test]
    fn test_level_rules_payload_contains_all_keys() {
        let rules = LevelAdjustmentRules {
            monthly_loss_downgrade_pct: dec!(-6.0),
            single_loss_downgrade_pct: dec!(-2.5),
            upgrade_profitable_trades: 11,
            upgrade_win_rate_pct: dec!(71.0),
            upgrade_consecutive_wins: 4,
            cooldown_profitable_trades: 21,
            cooldown_win_rate_pct: dec!(86.5),
            cooldown_consecutive_wins: 7,
            recovery_profitable_trades: 6,
            recovery_win_rate_pct: dec!(66.0),
            recovery_consecutive_wins: 3,
            min_trades_at_level_for_upgrade: 8,
            max_changes_in_30_days: 5,
        };
        let payload = ArgDispatcher::level_rules_payload(&rules);
        assert_eq!(payload["monthly_loss_downgrade_pct"], "-6");
        assert_eq!(payload["single_loss_downgrade_pct"], "-2.5");
        assert_eq!(payload["upgrade_profitable_trades"], 11);
        assert_eq!(payload["upgrade_win_rate_pct"], "71");
        assert_eq!(payload["upgrade_consecutive_wins"], 4);
        assert_eq!(payload["cooldown_profitable_trades"], 21);
        assert_eq!(payload["cooldown_win_rate_pct"], "86.5");
        assert_eq!(payload["cooldown_consecutive_wins"], 7);
        assert_eq!(payload["recovery_profitable_trades"], 6);
        assert_eq!(payload["recovery_win_rate_pct"], "66");
        assert_eq!(payload["recovery_consecutive_wins"], 3);
        assert_eq!(payload["min_trades_at_level_for_upgrade"], 8);
        assert_eq!(payload["max_changes_in_30_days"], 5);
    }

    #[test]
    fn test_level_dispatch_text_success_paths_cover_read_and_mutation_handlers() {
        let _guard = env_guard();
        std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        crate::protected_keyword::delete().expect("reset protected keyword");

        let mut dispatcher = test_dispatcher();
        let account = dispatcher
            .trust
            .create_account(
                "level-text",
                "level text account",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("create account");
        let account_id = account.id.to_string();

        let triggers = LevelCommandBuilder::new()
            .triggers()
            .build()
            .get_matches_from(["level", "triggers"]);
        dispatcher
            .dispatch_level(&triggers)
            .expect("triggers text should succeed");

        let status = LevelCommandBuilder::new()
            .status()
            .build()
            .get_matches_from(["level", "status", "--account", &account_id]);
        dispatcher
            .dispatch_level(&status)
            .expect("status text should succeed");

        let rules_show = LevelCommandBuilder::new()
            .rules()
            .build()
            .get_matches_from(["level", "rules", "show", "--account", &account_id]);
        dispatcher
            .dispatch_level(&rules_show)
            .expect("rules show text should succeed");

        let rules_set = LevelCommandBuilder::new()
            .rules()
            .build()
            .get_matches_from([
                "level",
                "rules",
                "set",
                "--account",
                &account_id,
                "--rule",
                "max_changes_in_30_days",
                "--value",
                "4",
                "--confirm-protected",
                "secret",
            ]);
        dispatcher
            .dispatch_level(&rules_set)
            .expect("rules set text should succeed");
        let updated_rules = dispatcher
            .trust
            .level_adjustment_rules_for_account(account.id)
            .expect("updated rules should load");
        assert_eq!(updated_rules.max_changes_in_30_days, 4);

        let change = LevelCommandBuilder::new()
            .change()
            .build()
            .get_matches_from([
                "level",
                "change",
                "--account",
                &account_id,
                "--to",
                "2",
                "--reason",
                "manual test change",
                "--trigger",
                "manual_review",
                "--confirm-protected",
                "secret",
            ]);
        dispatcher
            .dispatch_level(&change)
            .expect("level change text should succeed");

        let evaluate = LevelCommandBuilder::new()
            .evaluate()
            .build()
            .get_matches_from([
                "level",
                "evaluate",
                "--account",
                &account_id,
                "--profitable-trades",
                "8",
                "--win-rate",
                "80",
                "--monthly-loss",
                "0",
                "--largest-loss",
                "0",
                "--consecutive-wins",
                "4",
                "--apply",
                "--confirm-protected",
                "secret",
            ]);
        dispatcher
            .dispatch_level(&evaluate)
            .expect("level evaluate text should succeed");

        let progress = LevelCommandBuilder::new()
            .progress()
            .build()
            .get_matches_from([
                "level",
                "progress",
                "--account",
                &account_id,
                "--profitable-trades",
                "2",
                "--win-rate",
                "50",
                "--monthly-loss",
                "1",
                "--largest-loss",
                "1",
                "--consecutive-wins",
                "1",
            ]);
        dispatcher
            .dispatch_level(&progress)
            .expect("level progress text should succeed");

        crate::protected_keyword::delete().expect("cleanup protected keyword");
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
        std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
    }

    #[test]
    fn test_level_handlers_surface_core_errors_and_required_field_validation() {
        let _guard = env_guard();
        std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        crate::protected_keyword::delete().expect("reset protected keyword");

        let mut dispatcher = test_dispatcher();
        let account = dispatcher
            .trust
            .create_account(
                "level-errors",
                "level error account",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("create account");
        let account_id = account.id.to_string();
        let missing_account_id = Uuid::new_v4().to_string();

        let status = LevelCommandBuilder::new()
            .status()
            .build()
            .get_matches_from(["level", "status", "--account", &missing_account_id]);
        let error = dispatcher
            .dispatch_level(&status)
            .expect_err("unknown account level status should fail");
        assert!(error.to_string().contains("level_status_failed"));

        let rules_show = LevelCommandBuilder::new()
            .rules()
            .build()
            .get_matches_from(["level", "rules", "show", "--account", &missing_account_id]);
        let error = dispatcher
            .dispatch_level(&rules_show)
            .expect_err("unknown account rules show should fail");
        assert!(error.to_string().contains("level_rules_show_failed"));

        let rules_set = LevelCommandBuilder::new()
            .rules()
            .build()
            .get_matches_from([
                "level",
                "rules",
                "set",
                "--account",
                &missing_account_id,
                "--rule",
                "monthly_loss_downgrade_pct",
                "--value",
                "8",
                "--confirm-protected",
                "secret",
            ]);
        let error = dispatcher
            .dispatch_level(&rules_set)
            .expect_err("unknown account rules set should fail while reading current rules");
        assert!(error.to_string().contains("level_rules_read_failed"));

        let progress = LevelCommandBuilder::new()
            .progress()
            .build()
            .get_matches_from([
                "level",
                "progress",
                "--account",
                &missing_account_id,
                "--profitable-trades",
                "1",
                "--win-rate",
                "50",
                "--monthly-loss",
                "0",
                "--largest-loss",
                "0",
            ]);
        let error = dispatcher
            .dispatch_level(&progress)
            .expect_err("unknown account progress should fail");
        assert!(error.to_string().contains("level_progress_failed"));

        let evaluate = LevelCommandBuilder::new()
            .evaluate()
            .build()
            .get_matches_from([
                "level",
                "evaluate",
                "--account",
                &missing_account_id,
                "--profitable-trades",
                "1",
                "--win-rate",
                "50",
                "--monthly-loss",
                "0",
                "--largest-loss",
                "0",
            ]);
        let error = dispatcher
            .dispatch_level(&evaluate)
            .expect_err("unknown account evaluate should fail");
        assert!(error.to_string().contains("level_evaluation_failed"));

        let level_change_matches = |args: &[&str]| {
            let mut argv = vec!["level-change"];
            argv.extend_from_slice(args);
            Command::new("level-change")
                .arg(Arg::new("account").long("account"))
                .arg(
                    Arg::new("to")
                        .long("to")
                        .value_parser(clap::value_parser!(u8)),
                )
                .arg(Arg::new("reason").long("reason"))
                .arg(Arg::new("trigger").long("trigger"))
                .arg(Arg::new("confirm-protected").long("confirm-protected"))
                .get_matches_from(argv)
        };

        let missing_target = level_change_matches(&[
            "--account",
            &account_id,
            "--reason",
            "missing target",
            "--trigger",
            "manual_review",
            "--confirm-protected",
            "secret",
        ]);
        let error = dispatcher
            .level_change(&missing_target, ReportOutputFormat::Json)
            .expect_err("missing target level should fail");
        assert!(error.to_string().contains("missing_level"));

        let invalid_trigger = level_change_matches(&[
            "--account",
            &account_id,
            "--to",
            "2",
            "--reason",
            "blank trigger",
            "--trigger",
            "   ",
            "--confirm-protected",
            "secret",
        ]);
        let error = dispatcher
            .level_change(&invalid_trigger, ReportOutputFormat::Json)
            .expect_err("blank trigger should fail");
        assert!(error.to_string().contains("invalid_trigger"));

        let invalid_level = level_change_matches(&[
            "--account",
            &account_id,
            "--to",
            "9",
            "--reason",
            "bad target",
            "--trigger",
            "manual_review",
            "--confirm-protected",
            "secret",
        ]);
        let error = dispatcher
            .level_change(&invalid_level, ReportOutputFormat::Json)
            .expect_err("out of range target should fail in core");
        assert!(error.to_string().contains("level_change_failed"));

        crate::protected_keyword::delete().expect("cleanup protected keyword");
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
        std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
    }

    #[test]
    fn test_create_trading_vehicle_from_broker_validates_required_args_and_account_lookup() {
        let mut dispatcher = test_dispatcher();
        let missing_account = Command::new("test")
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("symbol").long("symbol"))
            .get_matches_from(["test", "--symbol", "AAPL"]);
        let error = dispatcher
            .create_trading_vehicle_from_broker(&missing_account, BrokerKind::Alpaca)
            .expect_err("missing account should fail");
        assert!(error.to_string().contains("broker_import_invalid_args"));
        assert!(error.to_string().contains("--account is required"));

        let missing_symbol = Command::new("test")
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("symbol").long("symbol"))
            .get_matches_from(["test", "--account", "paper-main"]);
        let error = dispatcher
            .create_trading_vehicle_from_broker(&missing_symbol, BrokerKind::Alpaca)
            .expect_err("missing symbol should fail");
        assert!(error.to_string().contains("broker_import_invalid_args"));
        assert!(error.to_string().contains("--symbol is required"));

        let unknown_account = Command::new("test")
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("symbol").long("symbol"))
            .get_matches_from(["test", "--account", "paper-main", "--symbol", "AAPL"]);
        let error = dispatcher
            .create_trading_vehicle_from_broker(&unknown_account, BrokerKind::Alpaca)
            .expect_err("unknown account should fail");
        assert!(error
            .to_string()
            .contains("broker_import_account_not_found"));

        let account = dispatcher
            .trust
            .create_account(
                "broker-import-account",
                "broker import",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("create account");
        let invalid_category = Command::new("test")
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("symbol").long("symbol"))
            .arg(Arg::new("category").long("category"))
            .get_matches_from([
                "test",
                "--account",
                &account.name,
                "--symbol",
                "AAPL",
                "--category",
                "option",
            ]);
        let error = dispatcher
            .create_trading_vehicle_from_broker(&invalid_category, BrokerKind::Alpaca)
            .expect_err("invalid category hint should fail before broker import");
        assert!(error
            .to_string()
            .contains("invalid_trading_vehicle_category"));
    }

    #[test]
    fn test_create_trading_vehicle_dispatch_covers_interactive_and_invalid_broker_paths() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        crate::dialogs::scripted_reset();
        let mut dispatcher = test_dispatcher();

        let create_matches = |args: &[&str]| {
            let mut argv = vec!["trading-vehicle-create"];
            argv.extend_from_slice(args);
            Command::new("trading-vehicle-create")
                .arg(Arg::new("confirm-protected").long("confirm-protected"))
                .arg(
                    Arg::new("from-alpaca")
                        .long("from-alpaca")
                        .action(clap::ArgAction::SetTrue),
                )
                .arg(Arg::new("from-broker").long("from-broker"))
                .arg(Arg::new("account").long("account"))
                .arg(Arg::new("symbol").long("symbol"))
                .arg(Arg::new("category").long("category"))
                .get_matches_from(argv)
        };

        let invalid_broker =
            create_matches(&["--from-broker", "bad", "--confirm-protected", "secret"]);
        let error = dispatcher
            .create_trading_vehicle(&invalid_broker)
            .expect_err("unsupported broker import should fail");
        assert!(error.to_string().contains("invalid_broker_kind"));

        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_input(Ok("DTVI".to_string()));
        crate::dialogs::scripted_push_input(Ok("paper-broker".to_string()));
        crate::dialogs::scripted_push_input(Ok("US0000000001".to_string()));
        let interactive = create_matches(&["--confirm-protected", "secret"]);
        dispatcher
            .create_trading_vehicle(&interactive)
            .expect("scripted interactive trading vehicle creation should succeed");
        let created = dispatcher
            .trust
            .search_trading_vehicles()
            .expect("created vehicle should be searchable");
        assert!(created.iter().any(|vehicle| vehicle.symbol == "DTVI"));

        crate::dialogs::scripted_reset();
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_parse_fixed_income_terms_accepts_bond_metadata_only_for_bonds() {
        let cmd = TradingVehicleCommandBuilder::new()
            .create_trading_vehicle()
            .build();
        let matches = cmd
            .try_get_matches_from([
                "trading-vehicle",
                "create",
                "--from-broker",
                "ibkr",
                "--account",
                "paper-main",
                "--symbol",
                "9128285M8",
                "--category",
                "bond",
                "--face-value",
                "1000",
                "--coupon-rate",
                "4.625",
                "--maturity-date",
                "2034-05-15",
                "--coupon-frequency",
                "2",
            ])
            .unwrap();
        let create = matches.subcommand_matches("create").unwrap();

        let terms =
            ArgDispatcher::parse_fixed_income_terms(create, TradingVehicleCategory::Bond).unwrap();
        let terms = terms.unwrap();
        assert_eq!(terms.face_value, Some(dec!(1000)));
        assert_eq!(terms.annual_coupon_rate_pct, Some(dec!(4.625)));
        assert_eq!(
            terms.maturity_date,
            Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap())
        );
        assert_eq!(terms.coupon_frequency_per_year, Some(2));

        let error = ArgDispatcher::parse_fixed_income_terms(create, TradingVehicleCategory::Etf)
            .unwrap_err();
        assert!(error.to_string().contains("invalid_fixed_income_terms"));
    }

    #[test]
    fn test_store_imported_trading_vehicle_applies_bond_terms_and_surfaces_store_errors() {
        let mut dispatcher = test_dispatcher();
        let cmd = TradingVehicleCommandBuilder::new()
            .create_trading_vehicle()
            .build();
        let matches = cmd
            .try_get_matches_from([
                "trading-vehicle",
                "create",
                "--from-broker",
                "ibkr",
                "--account",
                "paper-main",
                "--symbol",
                "9128285M8",
                "--category",
                "bond",
                "--face-value",
                "1000",
                "--coupon-rate",
                "4.625",
                "--maturity-date",
                "2034-05-15",
                "--coupon-frequency",
                "2",
            ])
            .expect("broker import matches should parse");
        let create = matches.subcommand_matches("create").unwrap();

        let imported = crate::trading_vehicle_import::ImportedTradingVehicle {
            upsert: TradingVehicleUpsert {
                symbol: "9128285M8".to_string(),
                isin: None,
                category: TradingVehicleCategory::Bond,
                broker: "ibkr".to_string(),
                broker_asset_id: Some("bond-1".to_string()),
                exchange: Some("SMART".to_string()),
                broker_asset_class: Some("bond".to_string()),
                broker_asset_status: None,
                tradable: None,
                marginable: None,
                shortable: None,
                easy_to_borrow: None,
                fractionable: None,
                fixed_income: None,
            },
            summary: "Imported from test".to_string(),
        };

        dispatcher
            .store_imported_trading_vehicle(create, imported)
            .expect("imported bond should be stored");
        let stored = dispatcher
            .trust
            .search_trading_vehicles()
            .expect("stored vehicle should be searchable")
            .into_iter()
            .find(|vehicle| vehicle.symbol == "9128285M8")
            .expect("stored imported bond should exist");
        let terms = stored.fixed_income.expect("bond terms should be attached");
        assert_eq!(terms.face_value, Some(dec!(1000)));
        assert_eq!(terms.annual_coupon_rate_pct, Some(dec!(4.625)));

        let mut failing_dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::trading_vehicle_write(),
        );
        let failing_import = crate::trading_vehicle_import::ImportedTradingVehicle {
            upsert: TradingVehicleUpsert {
                symbol: "FAILBOND".to_string(),
                isin: None,
                category: TradingVehicleCategory::Bond,
                broker: "ibkr".to_string(),
                broker_asset_id: Some("fail-bond".to_string()),
                exchange: Some("SMART".to_string()),
                broker_asset_class: Some("bond".to_string()),
                broker_asset_status: None,
                tradable: None,
                marginable: None,
                shortable: None,
                easy_to_borrow: None,
                fractionable: None,
                fixed_income: None,
            },
            summary: "Imported from failing writer".to_string(),
        };
        let error = failing_dispatcher
            .store_imported_trading_vehicle(create, failing_import)
            .expect_err("writer failure should surface as broker import store failure");
        assert_eq!(error.code, "broker_import_store_failed");
    }

    #[test]
    fn test_parse_fixed_income_terms_rejects_invalid_bond_metadata_inputs() {
        let parse_create = |args: &[&str]| {
            let mut argv = vec!["trading-vehicle", "create", "--category", "bond"];
            argv.extend_from_slice(args);
            TradingVehicleCommandBuilder::new()
                .create_trading_vehicle()
                .build()
                .try_get_matches_from(argv)
                .expect("bond metadata args should parse")
        };

        let invalid_decimal = parse_create(&["--coupon-rate", "not-a-decimal"]);
        let error = ArgDispatcher::parse_fixed_income_terms(
            invalid_decimal.subcommand_matches("create").unwrap(),
            TradingVehicleCategory::Bond,
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid_decimal"));
        assert!(error.to_string().contains("--coupon-rate"));

        let invalid_face_value = parse_create(&["--face-value", "not-a-decimal"]);
        let error = ArgDispatcher::parse_fixed_income_terms(
            invalid_face_value.subcommand_matches("create").unwrap(),
            TradingVehicleCategory::Bond,
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid_decimal"));
        assert!(error.to_string().contains("--face-value"));

        let invalid_maturity = parse_create(&["--maturity-date", "05/15/2034"]);
        let error = ArgDispatcher::parse_fixed_income_terms(
            invalid_maturity.subcommand_matches("create").unwrap(),
            TradingVehicleCategory::Bond,
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid_maturity_date"));

        let zero_frequency = parse_create(&["--coupon-frequency", "0"]);
        let error = ArgDispatcher::parse_fixed_income_terms(
            zero_frequency.subcommand_matches("create").unwrap(),
            TradingVehicleCategory::Bond,
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid_coupon_frequency"));

        let no_terms = parse_create(&[]);
        let parsed = ArgDispatcher::parse_fixed_income_terms(
            no_terms.subcommand_matches("create").unwrap(),
            TradingVehicleCategory::Bond,
        )
        .unwrap();
        assert!(parsed.is_none());
    }

    #[test]
    fn test_parse_trading_vehicle_category_rejects_unknown_values() {
        assert_eq!(
            ArgDispatcher::parse_trading_vehicle_category("bond").unwrap(),
            TradingVehicleCategory::Bond
        );
        let error = ArgDispatcher::parse_trading_vehicle_category("option").unwrap_err();
        assert!(error
            .to_string()
            .contains("invalid_trading_vehicle_category"));
    }

    #[test]
    fn test_update_bond_terms_preserves_existing_metadata_and_terms() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();
        dispatcher
            .trust
            .upsert_trading_vehicle(TradingVehicleUpsert {
                symbol: "9128285M8".to_string(),
                isin: Some("US9128285M81".to_string()),
                category: TradingVehicleCategory::Bond,
                broker: "ibkr".to_string(),
                broker_asset_id: Some("123456".to_string()),
                exchange: Some("SMART".to_string()),
                broker_asset_class: Some("bond".to_string()),
                broker_asset_status: Some("active".to_string()),
                tradable: Some(true),
                marginable: None,
                shortable: None,
                easy_to_borrow: None,
                fractionable: None,
                fixed_income: Some(FixedIncomeTerms {
                    face_value: Some(dec!(1000)),
                    annual_coupon_rate_pct: Some(dec!(4.0)),
                    maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                    coupon_frequency_per_year: Some(2),
                }),
            })
            .unwrap();

        let matches = TradingVehicleCommandBuilder::new()
            .update_bond_terms()
            .build()
            .get_matches_from([
                "trading-vehicle",
                "update-bond-terms",
                "--symbol",
                "9128285M8",
                "--broker",
                "ibkr",
                "--coupon-rate",
                "4.625",
                "--confirm-protected",
                "secret",
            ]);
        let update = matches.subcommand_matches("update-bond-terms").unwrap();
        dispatcher.update_bond_terms(update).unwrap();

        let updated = dispatcher
            .trust
            .search_trading_vehicles()
            .unwrap()
            .into_iter()
            .find(|vehicle| vehicle.symbol == "9128285M8")
            .unwrap();
        let terms = updated.fixed_income.unwrap();
        assert_eq!(updated.broker_asset_id.as_deref(), Some("123456"));
        assert_eq!(terms.face_value, Some(dec!(1000)));
        assert_eq!(terms.annual_coupon_rate_pct, Some(dec!(4.625)));
        assert_eq!(
            terms.maturity_date,
            Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap())
        );
        assert_eq!(terms.coupon_frequency_per_year, Some(2));
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_update_bond_terms_validates_missing_not_found_and_wrong_category() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();
        dispatcher
            .trust
            .upsert_trading_vehicle(TradingVehicleUpsert {
                symbol: "AAPL".to_string(),
                isin: Some("US0378331005".to_string()),
                category: TradingVehicleCategory::Stock,
                broker: "alpaca".to_string(),
                broker_asset_id: None,
                exchange: Some("NASDAQ".to_string()),
                broker_asset_class: Some("us_equity".to_string()),
                broker_asset_status: Some("active".to_string()),
                tradable: Some(true),
                marginable: Some(true),
                shortable: Some(true),
                easy_to_borrow: Some(true),
                fractionable: Some(true),
                fixed_income: None,
            })
            .expect("stock vehicle");

        let missing_terms = TradingVehicleCommandBuilder::new()
            .update_bond_terms()
            .build()
            .get_matches_from([
                "trading-vehicle",
                "update-bond-terms",
                "--symbol",
                "AAPL",
                "--broker",
                "alpaca",
                "--confirm-protected",
                "secret",
            ]);
        let error = dispatcher
            .update_bond_terms(
                missing_terms
                    .subcommand_matches("update-bond-terms")
                    .unwrap(),
            )
            .expect_err("missing terms should fail");
        assert!(error.to_string().contains("missing_bond_terms"));

        let missing_symbol = Command::new("update-bond-terms")
            .arg(Arg::new("symbol").long("symbol"))
            .arg(Arg::new("broker").long("broker"))
            .arg(Arg::new("face-value").long("face-value"))
            .arg(Arg::new("confirm-protected").long("confirm-protected"))
            .get_matches_from([
                "update-bond-terms",
                "--broker",
                "alpaca",
                "--face-value",
                "1000",
                "--confirm-protected",
                "secret",
            ]);
        let error = dispatcher
            .update_bond_terms(&missing_symbol)
            .expect_err("missing symbol should fail");
        assert!(error.to_string().contains("missing_argument"));

        let not_found = TradingVehicleCommandBuilder::new()
            .update_bond_terms()
            .build()
            .get_matches_from([
                "trading-vehicle",
                "update-bond-terms",
                "--symbol",
                "MISSING",
                "--broker",
                "alpaca",
                "--face-value",
                "1000",
                "--confirm-protected",
                "secret",
            ]);
        let error = dispatcher
            .update_bond_terms(not_found.subcommand_matches("update-bond-terms").unwrap())
            .expect_err("missing vehicle should fail");
        assert!(error.to_string().contains("trading_vehicle_not_found"));

        let wrong_category = TradingVehicleCommandBuilder::new()
            .update_bond_terms()
            .build()
            .get_matches_from([
                "trading-vehicle",
                "update-bond-terms",
                "--symbol",
                "AAPL",
                "--broker",
                "alpaca",
                "--face-value",
                "1000",
                "--confirm-protected",
                "secret",
            ]);
        let error = dispatcher
            .update_bond_terms(
                wrong_category
                    .subcommand_matches("update-bond-terms")
                    .unwrap(),
            )
            .expect_err("stock vehicle should fail bond update");
        assert!(error.to_string().contains("bond_vehicle_wrong_category"));

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_trading_vehicle_search_filters_find_incomplete_bonds() {
        let vehicles = vec![
            model::TradingVehicle {
                symbol: "AAPL".to_string(),
                broker: "ibkr".to_string(),
                category: TradingVehicleCategory::Stock,
                ..Default::default()
            },
            model::TradingVehicle {
                symbol: "9128285M8".to_string(),
                broker: "ibkr".to_string(),
                isin: Some("US9128285M81".to_string()),
                category: TradingVehicleCategory::Bond,
                fixed_income: Some(FixedIncomeTerms {
                    face_value: Some(dec!(1000)),
                    annual_coupon_rate_pct: None,
                    maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                    coupon_frequency_per_year: Some(2),
                }),
                ..Default::default()
            },
            model::TradingVehicle {
                symbol: "912810TL2".to_string(),
                broker: "ibkr".to_string(),
                category: TradingVehicleCategory::Bond,
                fixed_income: Some(FixedIncomeTerms {
                    face_value: Some(dec!(1000)),
                    annual_coupon_rate_pct: Some(dec!(4.625)),
                    maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                    coupon_frequency_per_year: Some(2),
                }),
                ..Default::default()
            },
        ];
        let filters = TradingVehicleSearchFilters {
            category: Some(TradingVehicleCategory::Bond),
            broker: Some("ibkr".to_string()),
            symbol: Some("912".to_string()),
            isin: None,
            missing_bond_terms: true,
            list_all: false,
            format: ReportOutputFormat::Text,
        };

        let filtered = ArgDispatcher::filter_trading_vehicles(vehicles, &filters);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].symbol, "9128285M8");
    }

    #[test]
    fn test_trading_vehicle_search_filters_cover_all_predicates() {
        let base_filters = TradingVehicleSearchFilters {
            category: None,
            broker: None,
            symbol: None,
            isin: None,
            missing_bond_terms: false,
            list_all: false,
            format: ReportOutputFormat::Text,
        };
        assert!(!base_filters.has_non_interactive_filters());

        let mut list_all = base_filters.clone();
        list_all.list_all = true;
        assert!(list_all.has_non_interactive_filters());

        let mut by_category = base_filters.clone();
        by_category.category = Some(TradingVehicleCategory::Stock);
        assert!(by_category.has_non_interactive_filters());

        let mut by_broker = base_filters.clone();
        by_broker.broker = Some("ibkr".to_string());
        assert!(by_broker.has_non_interactive_filters());

        let mut by_symbol = base_filters.clone();
        by_symbol.symbol = Some("AAPL".to_string());
        assert!(by_symbol.has_non_interactive_filters());

        let mut by_isin = base_filters.clone();
        by_isin.isin = Some("US0378331005".to_string());
        assert!(by_isin.has_non_interactive_filters());

        let mut missing_terms = base_filters.clone();
        missing_terms.missing_bond_terms = true;
        assert!(missing_terms.has_non_interactive_filters());

        let mut json = base_filters.clone();
        json.format = ReportOutputFormat::Json;
        assert!(json.has_non_interactive_filters());

        let stock = model::TradingVehicle {
            symbol: "AAPL".to_string(),
            broker: "ibkr".to_string(),
            isin: Some("US0378331005".to_string()),
            category: TradingVehicleCategory::Stock,
            ..Default::default()
        };

        assert!(ArgDispatcher::trading_vehicle_matches_filters(
            &stock,
            &by_category
        ));
        assert!(ArgDispatcher::trading_vehicle_matches_filters(
            &stock, &by_broker
        ));

        let mut wrong_broker = base_filters.clone();
        wrong_broker.broker = Some("alpaca".to_string());
        assert!(!ArgDispatcher::trading_vehicle_matches_filters(
            &stock,
            &wrong_broker
        ));

        let mut wrong_symbol = base_filters.clone();
        wrong_symbol.symbol = Some("MSFT".to_string());
        assert!(!ArgDispatcher::trading_vehicle_matches_filters(
            &stock,
            &wrong_symbol
        ));

        let mut wrong_isin = base_filters.clone();
        wrong_isin.isin = Some("US999".to_string());
        assert!(!ArgDispatcher::trading_vehicle_matches_filters(
            &stock,
            &wrong_isin
        ));

        let stock_without_isin = model::TradingVehicle {
            isin: None,
            ..stock.clone()
        };
        assert!(!ArgDispatcher::trading_vehicle_matches_filters(
            &stock_without_isin,
            &wrong_isin
        ));

        assert!(!ArgDispatcher::trading_vehicle_matches_filters(
            &stock,
            &missing_terms
        ));

        let incomplete_bond = model::TradingVehicle {
            symbol: "9128285M8".to_string(),
            broker: "ibkr".to_string(),
            category: TradingVehicleCategory::Bond,
            fixed_income: None,
            ..Default::default()
        };
        assert!(ArgDispatcher::trading_vehicle_matches_filters(
            &incomplete_bond,
            &missing_terms
        ));
    }

    #[test]
    fn test_trading_vehicle_search_json_payload_includes_fixed_income_terms() {
        let vehicle = model::TradingVehicle {
            symbol: "912810TL2".to_string(),
            broker: "ibkr".to_string(),
            category: TradingVehicleCategory::Bond,
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(4.625)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                coupon_frequency_per_year: Some(2),
            }),
            ..Default::default()
        };
        let filters = TradingVehicleSearchFilters {
            category: Some(TradingVehicleCategory::Bond),
            broker: None,
            symbol: None,
            isin: None,
            missing_bond_terms: false,
            list_all: true,
            format: ReportOutputFormat::Json,
        };

        let payload = ArgDispatcher::trading_vehicle_search_payload(&[vehicle], &filters);

        assert_eq!(payload["report"], "trading_vehicle_search");
        assert_eq!(payload["count"], 1);
        assert_eq!(payload["data"][0]["category"], "bond");
        assert_eq!(
            payload["data"][0]["fixed_income"]["annual_coupon_rate_pct"],
            "4.625"
        );
        assert_eq!(
            payload["data"][0]["fixed_income"]["coupon_frequency_per_year"],
            2
        );
    }

    #[test]
    fn test_trading_vehicle_search_dispatch_covers_interactive_text_json_and_invalid_filters() {
        let mut dispatcher = test_dispatcher();
        dispatcher
            .trust
            .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
            .expect("seed stock");
        dispatcher
            .trust
            .upsert_trading_vehicle(TradingVehicleUpsert {
                symbol: "912810TL2".to_string(),
                isin: Some("US912810TL26".to_string()),
                category: TradingVehicleCategory::Bond,
                broker: "ibkr".to_string(),
                broker_asset_id: None,
                exchange: None,
                broker_asset_class: None,
                broker_asset_status: None,
                tradable: None,
                marginable: None,
                shortable: None,
                easy_to_borrow: None,
                fractionable: None,
                fixed_income: Some(FixedIncomeTerms {
                    face_value: Some(dec!(1000)),
                    annual_coupon_rate_pct: Some(dec!(4.625)),
                    maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                    coupon_frequency_per_year: Some(2),
                }),
            })
            .expect("seed first bond");
        dispatcher
            .trust
            .upsert_trading_vehicle(TradingVehicleUpsert {
                symbol: "912810TL2".to_string(),
                isin: Some("US912810TL27".to_string()),
                category: TradingVehicleCategory::Bond,
                broker: "alpaca".to_string(),
                broker_asset_id: None,
                exchange: None,
                broker_asset_class: None,
                broker_asset_status: None,
                tradable: None,
                marginable: None,
                shortable: None,
                easy_to_borrow: None,
                fractionable: None,
                fixed_income: Some(FixedIncomeTerms {
                    face_value: Some(dec!(1000)),
                    annual_coupon_rate_pct: Some(dec!(4.75)),
                    maturity_date: Some(NaiveDate::from_ymd_opt(2035, 5, 15).unwrap()),
                    coupon_frequency_per_year: Some(2),
                }),
            })
            .expect("seed duplicate-symbol bond");

        crate::dialogs::scripted_reset();
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        let interactive = TradingVehicleCommandBuilder::new()
            .search_trading_vehicle()
            .build()
            .get_matches_from(["trading-vehicle", "search"]);
        dispatcher
            .search_trading_vehicle(interactive.subcommand_matches("search").unwrap())
            .expect("interactive search should display selected vehicle");

        let text_all = TradingVehicleCommandBuilder::new()
            .search_trading_vehicle()
            .build()
            .get_matches_from(["trading-vehicle", "search", "--all"]);
        dispatcher
            .search_trading_vehicle(text_all.subcommand_matches("search").unwrap())
            .expect("text search should display filtered vehicles");

        let no_matches = TradingVehicleCommandBuilder::new()
            .search_trading_vehicle()
            .build()
            .get_matches_from(["trading-vehicle", "search", "--symbol", "MISSING"]);
        dispatcher
            .search_trading_vehicle(no_matches.subcommand_matches("search").unwrap())
            .expect("empty text search should render no-match message");

        let json_filtered = TradingVehicleCommandBuilder::new()
            .search_trading_vehicle()
            .build()
            .get_matches_from([
                "trading-vehicle",
                "search",
                "--category",
                "bond",
                "--broker",
                " IBKR ",
                "--symbol",
                " 912 ",
                "--isin",
                " us912 ",
                "--format",
                "json",
            ]);
        dispatcher
            .search_trading_vehicle(json_filtered.subcommand_matches("search").unwrap())
            .expect("json search should render filtered payload");

        let invalid_category = TradingVehicleCommandBuilder::new()
            .search_trading_vehicle()
            .build()
            .get_matches_from(["trading-vehicle", "search", "--category", "option"]);
        let error = dispatcher
            .search_trading_vehicle(invalid_category.subcommand_matches("search").unwrap())
            .expect_err("invalid category should fail");
        assert!(error
            .to_string()
            .contains("invalid_trading_vehicle_category"));

        crate::dialogs::scripted_reset();
    }

    #[test]
    fn test_trading_vehicle_inventory_stats_summarize_bonds_and_brokers() {
        let vehicles = vec![
            model::TradingVehicle {
                symbol: "AAPL".to_string(),
                broker: "alpaca".to_string(),
                category: TradingVehicleCategory::Stock,
                ..Default::default()
            },
            model::TradingVehicle {
                symbol: "SPY".to_string(),
                broker: "ibkr".to_string(),
                category: TradingVehicleCategory::Etf,
                ..Default::default()
            },
            model::TradingVehicle {
                symbol: "912810TL2".to_string(),
                broker: "ibkr".to_string(),
                category: TradingVehicleCategory::Bond,
                fixed_income: Some(FixedIncomeTerms {
                    face_value: Some(dec!(1000)),
                    annual_coupon_rate_pct: Some(dec!(4)),
                    maturity_date: Some(NaiveDate::from_ymd_opt(2030, 1, 15).unwrap()),
                    coupon_frequency_per_year: Some(2),
                }),
                ..Default::default()
            },
            model::TradingVehicle {
                symbol: "9128285M8".to_string(),
                broker: "ibkr".to_string(),
                category: TradingVehicleCategory::Bond,
                fixed_income: Some(FixedIncomeTerms {
                    face_value: Some(dec!(1000)),
                    annual_coupon_rate_pct: Some(dec!(6)),
                    maturity_date: None,
                    coupon_frequency_per_year: Some(2),
                }),
                ..Default::default()
            },
            model::TradingVehicle {
                symbol: "912810TM0".to_string(),
                broker: "ibkr".to_string(),
                category: TradingVehicleCategory::Bond,
                fixed_income: Some(FixedIncomeTerms {
                    face_value: Some(dec!(1000)),
                    annual_coupon_rate_pct: Some(dec!(8)),
                    maturity_date: Some(NaiveDate::from_ymd_opt(2040, 1, 15).unwrap()),
                    coupon_frequency_per_year: Some(2),
                }),
                ..Default::default()
            },
        ];

        let stats = ArgDispatcher::calculate_trading_vehicle_inventory_stats(&vehicles)
            .expect("inventory stats");
        let payload = ArgDispatcher::trading_vehicle_stats_payload(&stats);

        assert_eq!(stats.total, 5);
        assert_eq!(stats.by_category.get("stock"), Some(&1));
        assert_eq!(stats.by_category.get("etf"), Some(&1));
        assert_eq!(stats.by_category.get("bond"), Some(&3));
        assert_eq!(stats.by_broker.get("ibkr"), Some(&4));
        assert_eq!(stats.bonds.total, 3);
        assert_eq!(stats.bonds.complete_terms, 2);
        assert_eq!(stats.bonds.missing_terms, 1);
        assert_eq!(stats.bonds.average_coupon_rate_pct, Some(dec!(6)));
        assert_eq!(
            ArgDispatcher::bond_maturity_range_text(&stats.bonds),
            "2030-01-15..2040-01-15"
        );
        assert_eq!(payload["bonds"]["earliest_maturity"], "2030-01-15");
        assert_eq!(payload["bonds"]["latest_maturity"], "2040-01-15");
        assert_eq!(payload["bonds"]["average_coupon_rate_pct"], "6");
        ArgDispatcher::print_trading_vehicle_stats(&stats);

        let empty_stats =
            ArgDispatcher::calculate_trading_vehicle_inventory_stats(&[]).expect("empty stats");
        assert_eq!(
            ArgDispatcher::bond_maturity_range_text(&empty_stats.bonds),
            "n/a"
        );
        ArgDispatcher::print_trading_vehicle_stats(&empty_stats);
    }

    #[test]
    fn test_trading_vehicle_inventory_stats_report_overflow_edges() {
        let coupon_bond = model::TradingVehicle {
            symbol: "BIGCOUPON".to_string(),
            broker: "ibkr".to_string(),
            category: TradingVehicleCategory::Bond,
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(1)),
                maturity_date: None,
                coupon_frequency_per_year: Some(2),
            }),
            ..Default::default()
        };

        let mut stats = BondInventoryStats {
            total: 0,
            complete_terms: 0,
            missing_terms: 0,
            coupon_rate_count: 0,
            average_coupon_rate_pct: None,
            earliest_maturity: None,
            latest_maturity: None,
        };
        let mut coupon_rate_sum = Decimal::MAX;
        let mut coupon_rate_divisor = Decimal::ZERO;
        let error = ArgDispatcher::add_bond_inventory_stats(
            &mut stats,
            &coupon_bond,
            &mut coupon_rate_sum,
            &mut coupon_rate_divisor,
        )
        .expect_err("coupon sum overflow should be reported");
        assert!(error.to_string().contains("coupon sum overflow"));

        let mut stats = BondInventoryStats {
            total: 0,
            complete_terms: 0,
            missing_terms: 0,
            coupon_rate_count: 0,
            average_coupon_rate_pct: None,
            earliest_maturity: None,
            latest_maturity: None,
        };
        let mut coupon_rate_sum = Decimal::ZERO;
        let mut coupon_rate_divisor = Decimal::MAX;
        let error = ArgDispatcher::add_bond_inventory_stats(
            &mut stats,
            &coupon_bond,
            &mut coupon_rate_sum,
            &mut coupon_rate_divisor,
        )
        .expect_err("coupon count overflow should be reported");
        assert!(error.to_string().contains("coupon count overflow"));

        let error = ArgDispatcher::checked_increment_usize(usize::MAX)
            .expect_err("usize increment overflow should be reported");
        assert!(error.to_string().contains("count overflow"));

        let mut counts = std::collections::BTreeMap::from([("bond".to_string(), usize::MAX)]);
        let error = ArgDispatcher::increment_count(&mut counts, "bond".to_string())
            .expect_err("map count overflow should be reported");
        assert!(error.to_string().contains("count overflow"));
    }

    #[test]
    fn test_trading_vehicle_stats_dispatch_supports_text_and_json() {
        let mut dispatcher = test_dispatcher();
        dispatcher
            .trust
            .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
            .expect("seed stock vehicle");
        dispatcher
            .trust
            .upsert_trading_vehicle(TradingVehicleUpsert {
                symbol: "912810TL2".to_string(),
                isin: Some("US912810TL26".to_string()),
                category: TradingVehicleCategory::Bond,
                broker: "ibkr".to_string(),
                broker_asset_id: Some("123456".to_string()),
                exchange: Some("SMART".to_string()),
                broker_asset_class: Some("bond".to_string()),
                broker_asset_status: Some("active".to_string()),
                tradable: Some(true),
                marginable: None,
                shortable: None,
                easy_to_borrow: None,
                fractionable: None,
                fixed_income: Some(FixedIncomeTerms {
                    face_value: Some(dec!(1000)),
                    annual_coupon_rate_pct: Some(dec!(4.25)),
                    maturity_date: Some(NaiveDate::from_ymd_opt(2031, 2, 15).unwrap()),
                    coupon_frequency_per_year: Some(2),
                }),
            })
            .expect("seed bond vehicle");

        let text = TradingVehicleCommandBuilder::new()
            .stats()
            .build()
            .get_matches_from(["trading-vehicle", "stats"]);
        dispatcher
            .trading_vehicle_stats(text.subcommand_matches("stats").unwrap())
            .expect("text stats should render");

        let json = TradingVehicleCommandBuilder::new()
            .stats()
            .build()
            .get_matches_from(["trading-vehicle", "stats", "--format", "json"]);
        dispatcher
            .trading_vehicle_stats(json.subcommand_matches("stats").unwrap())
            .expect("json stats should render");
    }

    #[test]
    fn test_parse_bond_analytics_input_loads_stored_fixed_income_terms() {
        let mut dispatcher = test_dispatcher();
        dispatcher
            .trust
            .upsert_trading_vehicle(TradingVehicleUpsert {
                symbol: "9128285M8".to_string(),
                isin: None,
                category: TradingVehicleCategory::Bond,
                broker: "ibkr".to_string(),
                broker_asset_id: Some("123456".to_string()),
                exchange: Some("SMART".to_string()),
                broker_asset_class: Some("bond".to_string()),
                broker_asset_status: None,
                tradable: None,
                marginable: None,
                shortable: None,
                easy_to_borrow: None,
                fractionable: None,
                fixed_income: Some(FixedIncomeTerms {
                    face_value: Some(dec!(1000)),
                    annual_coupon_rate_pct: Some(dec!(4.625)),
                    maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                    coupon_frequency_per_year: Some(2),
                }),
            })
            .unwrap();

        let matches = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--symbol",
                "9128285M8",
                "--broker",
                "ibkr",
                "--market-price",
                "997.50",
                "--quantity",
                "5",
                "--years-to-maturity",
                "7",
            ]);
        let bond = matches.subcommand_matches("bond").unwrap();
        let input = dispatcher
            .parse_bond_analytics_input(bond, ReportOutputFormat::Text)
            .unwrap();

        assert_eq!(input.face_value, dec!(1000));
        assert_eq!(input.annual_coupon_rate_pct, dec!(4.625));
        assert_eq!(input.market_price, dec!(997.50));
        assert_eq!(input.quantity, 5);
        assert_eq!(input.years_to_maturity, dec!(7));
        assert_eq!(input.accrued_interest, None);
    }

    #[test]
    fn test_parse_bond_analytics_input_builds_accrued_interest_from_stored_frequency() {
        let mut dispatcher = test_dispatcher();
        dispatcher
            .trust
            .upsert_trading_vehicle(TradingVehicleUpsert {
                symbol: "9128285M8".to_string(),
                isin: None,
                category: TradingVehicleCategory::Bond,
                broker: "ibkr".to_string(),
                broker_asset_id: Some("123456".to_string()),
                exchange: Some("SMART".to_string()),
                broker_asset_class: Some("bond".to_string()),
                broker_asset_status: None,
                tradable: None,
                marginable: None,
                shortable: None,
                easy_to_borrow: None,
                fractionable: None,
                fixed_income: Some(FixedIncomeTerms {
                    face_value: Some(dec!(1000)),
                    annual_coupon_rate_pct: Some(dec!(6)),
                    maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                    coupon_frequency_per_year: Some(2),
                }),
            })
            .unwrap();

        let matches = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--symbol",
                "9128285M8",
                "--broker",
                "ibkr",
                "--market-price",
                "1000",
                "--quantity",
                "10",
                "--years-to-maturity",
                "5",
                "--settlement-date",
                "2026-04-01",
                "--last-coupon-date",
                "2026-01-01",
                "--next-coupon-date",
                "2026-07-01",
                "--day-count",
                "actual-360",
            ]);
        let bond = matches.subcommand_matches("bond").unwrap();
        let input = dispatcher
            .parse_bond_analytics_input(bond, ReportOutputFormat::Text)
            .unwrap();
        let accrued = input.accrued_interest.unwrap();

        assert_eq!(
            accrued.settlement_date,
            NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()
        );
        assert_eq!(accrued.coupon_frequency_per_year, 2);
        assert_eq!(
            accrued.day_count_basis,
            core::calculators_fixed_income::DayCountBasis::Actual360
        );
    }

    #[test]
    fn test_parse_bond_analytics_input_requires_frequency_for_manual_accrual() {
        let mut dispatcher = test_dispatcher();
        let matches = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--face-value",
                "1000",
                "--market-price",
                "1000",
                "--coupon-rate",
                "6",
                "--quantity",
                "10",
                "--years-to-maturity",
                "5",
                "--settlement-date",
                "2026-04-01",
                "--last-coupon-date",
                "2026-01-01",
                "--next-coupon-date",
                "2026-07-01",
            ]);
        let bond = matches.subcommand_matches("bond").unwrap();
        let error = dispatcher
            .parse_bond_analytics_input(bond, ReportOutputFormat::Text)
            .unwrap_err();

        assert!(error.to_string().contains("missing_argument"));
    }

    #[test]
    fn test_bond_metrics_cover_error_wrapping_and_stored_term_edges() {
        let mut dispatcher = test_dispatcher();
        let upsert_bond =
            |dispatcher: &mut ArgDispatcher, symbol: &str, fixed_income: FixedIncomeTerms| {
                dispatcher
                    .trust
                    .upsert_trading_vehicle(TradingVehicleUpsert {
                        symbol: symbol.to_string(),
                        isin: None,
                        category: TradingVehicleCategory::Bond,
                        broker: "ibkr".to_string(),
                        broker_asset_id: Some(format!("asset-{symbol}")),
                        exchange: Some("SMART".to_string()),
                        broker_asset_class: Some("bond".to_string()),
                        broker_asset_status: None,
                        tradable: None,
                        marginable: None,
                        shortable: None,
                        easy_to_borrow: None,
                        fractionable: None,
                        fixed_income: Some(fixed_income),
                    })
                    .expect("seed bond vehicle");
            };

        upsert_bond(
            &mut dispatcher,
            "OVERRIDE",
            FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(4.25)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                coupon_frequency_per_year: Some(2),
            },
        );
        let override_matches = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--symbol",
                "OVERRIDE",
                "--broker",
                "ibkr",
                "--face-value",
                "1001",
                "--market-price",
                "995.25",
                "--coupon-rate",
                "4.5",
                "--quantity",
                "3",
            ]);
        let input = dispatcher
            .parse_bond_analytics_input(
                override_matches
                    .subcommand_matches("bond")
                    .expect("bond subcommand"),
                ReportOutputFormat::Text,
            )
            .expect("stored bond should accept manual overrides");
        assert_eq!(input.face_value, dec!(1001));
        assert_eq!(input.annual_coupon_rate_pct, dec!(4.5));
        assert!(input.years_to_maturity > Decimal::ZERO);

        upsert_bond(
            &mut dispatcher,
            "MATURED",
            FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(3.75)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
                coupon_frequency_per_year: Some(2),
            },
        );
        let matured_matches = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--symbol",
                "MATURED",
                "--broker",
                "ibkr",
                "--market-price",
                "1000",
                "--quantity",
                "1",
            ]);
        let matured = dispatcher
            .parse_bond_analytics_input(
                matured_matches
                    .subcommand_matches("bond")
                    .expect("bond subcommand"),
                ReportOutputFormat::Text,
            )
            .expect("past stored maturity should clamp to zero years");
        assert_eq!(matured.years_to_maturity, Decimal::ZERO);

        upsert_bond(
            &mut dispatcher,
            "NOCOUPON",
            FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: None,
                maturity_date: Some(NaiveDate::from_ymd_opt(2030, 1, 1).unwrap()),
                coupon_frequency_per_year: Some(2),
            },
        );
        let no_coupon = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--symbol",
                "NOCOUPON",
                "--broker",
                "ibkr",
                "--market-price",
                "1000",
                "--quantity",
                "1",
                "--years-to-maturity",
                "1",
            ]);
        let error = dispatcher
            .parse_bond_analytics_input(
                no_coupon
                    .subcommand_matches("bond")
                    .expect("bond subcommand"),
                ReportOutputFormat::Json,
            )
            .expect_err("missing stored coupon rate should fail");
        assert!(error.to_string().contains("bond_terms_missing"));

        upsert_bond(
            &mut dispatcher,
            "NOFREQ",
            FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(5)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2030, 1, 1).unwrap()),
                coupon_frequency_per_year: None,
            },
        );
        let no_frequency = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--symbol",
                "NOFREQ",
                "--broker",
                "ibkr",
                "--market-price",
                "1000",
                "--quantity",
                "1",
                "--years-to-maturity",
                "1",
                "--settlement-date",
                "2026-04-01",
                "--last-coupon-date",
                "2026-01-01",
                "--next-coupon-date",
                "2026-07-01",
            ]);
        let error = dispatcher
            .parse_bond_analytics_input(
                no_frequency
                    .subcommand_matches("bond")
                    .expect("bond subcommand"),
                ReportOutputFormat::Json,
            )
            .expect_err("missing stored coupon frequency should fail for accrual");
        assert!(error.to_string().contains("missing_argument"));

        let invalid_calculation = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--face-value",
                "1000",
                "--market-price",
                "1000",
                "--coupon-rate",
                "5",
                "--quantity",
                "0",
                "--years-to-maturity",
                "1",
            ]);
        let error = dispatcher
            .metrics_bond(
                invalid_calculation
                    .subcommand_matches("bond")
                    .expect("bond subcommand"),
            )
            .expect_err("invalid quantity should be wrapped as bond analytics failure");
        assert!(error.to_string().contains("bond_analytics_failed"));
    }

    #[test]
    fn test_parse_bond_analytics_input_reaches_lax_accrual_validation_edges() {
        fn lax_bond_matches(args: &[&str]) -> clap::ArgMatches {
            let mut argv = vec!["bond"];
            argv.extend_from_slice(args);
            Command::new("bond")
                .arg(Arg::new("symbol").long("symbol"))
                .arg(Arg::new("broker").long("broker"))
                .arg(Arg::new("face-value").long("face-value"))
                .arg(Arg::new("market-price").long("market-price"))
                .arg(Arg::new("coupon-rate").long("coupon-rate"))
                .arg(
                    Arg::new("quantity")
                        .long("quantity")
                        .value_parser(clap::value_parser!(i64)),
                )
                .arg(Arg::new("years-to-maturity").long("years-to-maturity"))
                .arg(
                    Arg::new("coupon-frequency")
                        .long("coupon-frequency")
                        .value_parser(clap::value_parser!(u16)),
                )
                .arg(Arg::new("settlement-date").long("settlement-date"))
                .arg(Arg::new("last-coupon-date").long("last-coupon-date"))
                .arg(Arg::new("next-coupon-date").long("next-coupon-date"))
                .arg(Arg::new("day-count").long("day-count"))
                .get_matches_from(argv)
        }

        let mut dispatcher = test_dispatcher();
        let missing_quantity = lax_bond_matches(&[
            "--face-value",
            "1000",
            "--market-price",
            "1000",
            "--coupon-rate",
            "5",
            "--years-to-maturity",
            "1",
        ]);
        let error = dispatcher
            .parse_bond_analytics_input(&missing_quantity, ReportOutputFormat::Json)
            .expect_err("missing quantity should be reported by dispatcher");
        assert!(error.to_string().contains("missing_argument"));

        for args in [
            vec![
                "--face-value",
                "1000",
                "--market-price",
                "1000",
                "--quantity",
                "1",
                "--years-to-maturity",
                "1",
            ],
            vec![
                "--face-value",
                "1000",
                "--market-price",
                "1000",
                "--coupon-rate",
                "5",
                "--quantity",
                "1",
            ],
        ] {
            let matches = lax_bond_matches(&args);
            let error = dispatcher
                .parse_bond_analytics_input(&matches, ReportOutputFormat::Json)
                .expect_err("missing manual bond analytics argument should fail");
            assert!(error.to_string().contains("missing_argument"));
        }

        let invalid_day_count = lax_bond_matches(&[
            "--face-value",
            "1000",
            "--market-price",
            "1000",
            "--coupon-rate",
            "5",
            "--quantity",
            "1",
            "--years-to-maturity",
            "1",
            "--coupon-frequency",
            "2",
            "--settlement-date",
            "2026-04-01",
            "--last-coupon-date",
            "2026-01-01",
            "--next-coupon-date",
            "2026-07-01",
            "--day-count",
            "30-360",
        ]);
        let error = dispatcher
            .parse_bond_analytics_input(&invalid_day_count, ReportOutputFormat::Json)
            .expect_err("invalid day-count should be reported by dispatcher");
        assert!(error.to_string().contains("invalid_day_count"));

        for (args, expected) in [
            (
                vec![
                    "--face-value",
                    "1000",
                    "--market-price",
                    "1000",
                    "--coupon-rate",
                    "5",
                    "--quantity",
                    "1",
                    "--years-to-maturity",
                    "1",
                    "--coupon-frequency",
                    "2",
                    "--settlement-date",
                    "2026-04-01",
                    "--last-coupon-date",
                    "bad-date",
                    "--next-coupon-date",
                    "2026-07-01",
                ],
                "invalid_date",
            ),
            (
                vec![
                    "--face-value",
                    "1000",
                    "--market-price",
                    "1000",
                    "--coupon-rate",
                    "5",
                    "--quantity",
                    "1",
                    "--years-to-maturity",
                    "1",
                    "--coupon-frequency",
                    "2",
                    "--settlement-date",
                    "2026-04-01",
                    "--last-coupon-date",
                    "2026-01-01",
                    "--next-coupon-date",
                    "bad-date",
                ],
                "invalid_date",
            ),
        ] {
            let matches = lax_bond_matches(&args);
            let error = dispatcher
                .parse_bond_analytics_input(&matches, ReportOutputFormat::Json)
                .expect_err("invalid coupon date should be reported by dispatcher");
            assert!(error.to_string().contains(expected));
        }
    }

    #[test]
    fn test_metrics_bond_executes_text_and_validates_lookup_and_accrual_edges() {
        let mut dispatcher = test_dispatcher();
        let upsert_vehicle = |dispatcher: &mut ArgDispatcher,
                              symbol: &str,
                              category: TradingVehicleCategory,
                              fixed_income: Option<FixedIncomeTerms>| {
            dispatcher
                .trust
                .upsert_trading_vehicle(TradingVehicleUpsert {
                    symbol: symbol.to_string(),
                    isin: None,
                    category,
                    broker: "ibkr".to_string(),
                    broker_asset_id: Some(format!("asset-{symbol}")),
                    exchange: Some("SMART".to_string()),
                    broker_asset_class: Some("bond".to_string()),
                    broker_asset_status: None,
                    tradable: None,
                    marginable: None,
                    shortable: None,
                    easy_to_borrow: None,
                    fractionable: None,
                    fixed_income,
                })
                .expect("upsert vehicle");
        };

        let text = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--face-value",
                "1000",
                "--market-price",
                "980",
                "--coupon-rate",
                "5",
                "--quantity",
                "2",
                "--years-to-maturity",
                "4",
                "--coupon-frequency",
                "2",
                "--settlement-date",
                "2026-04-01",
                "--last-coupon-date",
                "2026-01-01",
                "--next-coupon-date",
                "2026-07-01",
                "--day-count",
                "actual-365",
            ]);
        dispatcher
            .metrics_bond(text.subcommand_matches("bond").expect("bond subcommand"))
            .expect("bond metrics text output should render");

        for args in [
            vec![
                "metrics",
                "bond",
                "--symbol",
                "BOND1",
                "--market-price",
                "1000",
                "--quantity",
                "1",
                "--years-to-maturity",
                "1",
            ],
            vec![
                "metrics",
                "bond",
                "--broker",
                "ibkr",
                "--market-price",
                "1000",
                "--quantity",
                "1",
                "--years-to-maturity",
                "1",
            ],
            vec![
                "metrics",
                "bond",
                "--symbol",
                "MISSING",
                "--broker",
                "ibkr",
                "--market-price",
                "1000",
                "--quantity",
                "1",
                "--years-to-maturity",
                "1",
            ],
        ] {
            let matches = MetricsCommandBuilder::new()
                .bond()
                .build()
                .get_matches_from(args);
            let error = dispatcher
                .parse_bond_analytics_input(
                    matches.subcommand_matches("bond").expect("bond subcommand"),
                    ReportOutputFormat::Json,
                )
                .expect_err("invalid lookup shape should fail");
            assert!(
                error.to_string().contains("missing_argument")
                    || error.to_string().contains("bond_vehicle_not_found")
            );
        }

        upsert_vehicle(&mut dispatcher, "ETF1", TradingVehicleCategory::Etf, None);
        let wrong_category = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--symbol",
                "ETF1",
                "--broker",
                "ibkr",
                "--market-price",
                "1000",
                "--quantity",
                "1",
                "--years-to-maturity",
                "1",
            ]);
        let error = dispatcher
            .parse_bond_analytics_input(
                wrong_category
                    .subcommand_matches("bond")
                    .expect("bond subcommand"),
                ReportOutputFormat::Json,
            )
            .expect_err("non-bond vehicle should fail");
        assert!(error.to_string().contains("bond_vehicle_wrong_category"));

        upsert_vehicle(
            &mut dispatcher,
            "NOTERMS",
            TradingVehicleCategory::Bond,
            None,
        );
        let no_terms = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--symbol",
                "NOTERMS",
                "--broker",
                "ibkr",
                "--market-price",
                "1000",
                "--quantity",
                "1",
                "--years-to-maturity",
                "1",
            ]);
        let error = dispatcher
            .parse_bond_analytics_input(
                no_terms
                    .subcommand_matches("bond")
                    .expect("bond subcommand"),
                ReportOutputFormat::Json,
            )
            .expect_err("bond without fixed-income terms should fail");
        assert!(error.to_string().contains("bond_terms_missing"));

        upsert_vehicle(
            &mut dispatcher,
            "NOFACE",
            TradingVehicleCategory::Bond,
            Some(FixedIncomeTerms {
                face_value: None,
                annual_coupon_rate_pct: Some(dec!(5)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2030, 1, 1).unwrap()),
                coupon_frequency_per_year: Some(2),
            }),
        );
        let no_face = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--symbol",
                "NOFACE",
                "--broker",
                "ibkr",
                "--market-price",
                "1000",
                "--quantity",
                "1",
                "--years-to-maturity",
                "1",
            ]);
        let error = dispatcher
            .parse_bond_analytics_input(
                no_face.subcommand_matches("bond").expect("bond subcommand"),
                ReportOutputFormat::Json,
            )
            .expect_err("missing stored face value should fail");
        assert!(error.to_string().contains("bond_terms_missing"));

        upsert_vehicle(
            &mut dispatcher,
            "NOMATURITY",
            TradingVehicleCategory::Bond,
            Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(5)),
                maturity_date: None,
                coupon_frequency_per_year: Some(2),
            }),
        );
        let no_maturity = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--symbol",
                "NOMATURITY",
                "--broker",
                "ibkr",
                "--market-price",
                "1000",
                "--quantity",
                "1",
            ]);
        let error = dispatcher
            .parse_bond_analytics_input(
                no_maturity
                    .subcommand_matches("bond")
                    .expect("bond subcommand"),
                ReportOutputFormat::Json,
            )
            .expect_err("missing maturity should require explicit years");
        assert!(error.to_string().contains("bond_terms_missing"));

        for (args, expected) in [
            (
                vec![
                    "metrics",
                    "bond",
                    "--face-value",
                    "1000",
                    "--market-price",
                    "1000",
                    "--coupon-rate",
                    "5",
                    "--quantity",
                    "1",
                    "--years-to-maturity",
                    "1",
                    "--coupon-frequency",
                    "0",
                    "--settlement-date",
                    "2026-04-01",
                    "--last-coupon-date",
                    "2026-01-01",
                    "--next-coupon-date",
                    "2026-07-01",
                ],
                "invalid_coupon_frequency",
            ),
            (
                vec![
                    "metrics",
                    "bond",
                    "--face-value",
                    "1000",
                    "--market-price",
                    "1000",
                    "--coupon-rate",
                    "5",
                    "--quantity",
                    "1",
                    "--years-to-maturity",
                    "1",
                    "--coupon-frequency",
                    "2",
                    "--last-coupon-date",
                    "2026-01-01",
                    "--next-coupon-date",
                    "2026-07-01",
                ],
                "missing_argument",
            ),
            (
                vec![
                    "metrics",
                    "bond",
                    "--face-value",
                    "1000",
                    "--market-price",
                    "1000",
                    "--coupon-rate",
                    "5",
                    "--quantity",
                    "1",
                    "--years-to-maturity",
                    "1",
                    "--coupon-frequency",
                    "2",
                    "--settlement-date",
                    "bad-date",
                    "--last-coupon-date",
                    "2026-01-01",
                    "--next-coupon-date",
                    "2026-07-01",
                ],
                "invalid_date",
            ),
        ] {
            let matches = MetricsCommandBuilder::new()
                .bond()
                .build()
                .get_matches_from(args);
            let error = dispatcher
                .parse_bond_analytics_input(
                    matches.subcommand_matches("bond").expect("bond subcommand"),
                    ReportOutputFormat::Json,
                )
                .expect_err("invalid accrued-interest inputs should fail");
            assert!(error.to_string().contains(expected));
        }
    }

    #[test]
    fn test_years_to_maturity_uses_decimal_day_count() {
        let maturity = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap();
        let as_of = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let years =
            ArgDispatcher::years_to_maturity(maturity, as_of, ReportOutputFormat::Text).unwrap();
        assert_eq!(years, dec!(1));
    }

    #[test]
    fn test_trade_size_preview_supports_text_output_and_rejects_invalid_currency() {
        let mut dispatcher = test_dispatcher();
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);

        let ok_matches = size_preview_matches(&[
            "--account",
            &account.id.to_string(),
            "--entry",
            "150",
            "--stop",
            "147.5",
            "--currency",
            "usd",
        ]);
        dispatcher
            .trade_size_preview(&ok_matches, ReportOutputFormat::Text)
            .expect("text preview should succeed");

        let invalid_currency = size_preview_matches(&[
            "--account",
            &account.id.to_string(),
            "--entry",
            "150",
            "--stop",
            "147.5",
            "--currency",
            "jpy",
        ]);
        let error = dispatcher
            .trade_size_preview(&invalid_currency, ReportOutputFormat::Json)
            .expect_err("unsupported currency should fail");
        assert!(error.to_string().contains("invalid_currency"));

        let mut empty_dispatcher = test_dispatcher();
        let account = empty_dispatcher
            .trust
            .create_account(
                "size-no-balance",
                "test",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account without balance");
        let no_balance = size_preview_matches(&[
            "--account",
            &account.id.to_string(),
            "--entry",
            "0",
            "--stop",
            "147.5",
            "--currency",
            "usd",
        ]);
        let error = empty_dispatcher
            .trade_size_preview(&no_balance, ReportOutputFormat::Json)
            .expect_err("size preview should surface calculation failures");
        assert!(error.to_string().contains("size_preview_failed"));

        let mut level_failure_dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::level_fails_after_quantity(),
        );
        let level_failure = size_preview_matches(&[
            "--account",
            &Uuid::new_v4().to_string(),
            "--entry",
            "150",
            "--stop",
            "147.5",
            "--currency",
            "usd",
        ]);
        let error = level_failure_dispatcher
            .trade_size_preview(&level_failure, ReportOutputFormat::Json)
            .expect_err("size preview should surface post-calculation level load failures");
        assert_eq!(error.code, "size_preview_level_load_failed");
    }

    #[test]
    fn test_trade_hypothesis_supports_text_output_and_rejects_invalid_inputs() {
        let mut dispatcher = test_dispatcher();
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);

        let ok_matches = hypothesis_matches(&[
            "--account",
            &account.id.to_string(),
            "--entry",
            "150",
            "--stop",
            "147.5",
            "--target",
            "160",
            "--quantity",
            "20",
            "--currency",
            "usd",
        ]);
        dispatcher
            .trade_hypothesis(&ok_matches, ReportOutputFormat::Text)
            .expect("text hypothesis should succeed");

        let invalid_quantity = hypothesis_matches(&[
            "--account",
            &account.id.to_string(),
            "--entry",
            "150",
            "--stop",
            "147.5",
            "--target",
            "160",
            "--quantity",
            "oops",
            "--currency",
            "usd",
        ]);
        let quantity_error = dispatcher
            .trade_hypothesis(&invalid_quantity, ReportOutputFormat::Json)
            .expect_err("invalid quantity should fail");
        assert!(quantity_error.to_string().contains("invalid_quantity"));

        let missing_quantity = hypothesis_matches(&[
            "--account",
            &account.id.to_string(),
            "--entry",
            "150",
            "--stop",
            "147.5",
            "--target",
            "160",
            "--currency",
            "usd",
        ]);
        let missing_quantity_error = dispatcher
            .trade_hypothesis(&missing_quantity, ReportOutputFormat::Json)
            .expect_err("missing quantity should fail");
        assert!(missing_quantity_error
            .to_string()
            .contains("missing_argument"));

        let zero_quantity = hypothesis_matches(&[
            "--account",
            &account.id.to_string(),
            "--entry",
            "150",
            "--stop",
            "147.5",
            "--target",
            "160",
            "--quantity",
            "0",
            "--currency",
            "usd",
        ]);
        let zero_quantity_error = dispatcher
            .trade_hypothesis(&zero_quantity, ReportOutputFormat::Json)
            .expect_err("zero quantity should fail");
        assert!(zero_quantity_error.to_string().contains("invalid_quantity"));

        let invalid_currency = hypothesis_matches(&[
            "--account",
            &account.id.to_string(),
            "--entry",
            "150",
            "--stop",
            "147.5",
            "--target",
            "160",
            "--quantity",
            "20",
            "--currency",
            "jpy",
        ]);
        let currency_error = dispatcher
            .trade_hypothesis(&invalid_currency, ReportOutputFormat::Json)
            .expect_err("unsupported currency should fail");
        assert!(currency_error.to_string().contains("invalid_currency"));
    }

    #[test]
    fn test_size_preview_levels_marks_current_level_and_scales_risk() {
        let levels = ArgDispatcher::size_preview_levels(100, 3, dec!(2.5));
        assert_eq!(levels.len(), 5);
        assert_eq!(levels[0]["quantity"], 10);
        assert_eq!(levels[2]["quantity"], 50);
        assert_eq!(levels[3]["current"], true);
        assert_eq!(levels[4]["quantity"], 150);
        assert_eq!(levels[4]["risk_amount"], "375");
    }

    #[test]
    fn test_trade_size_preview_text_lines_include_current_marker() {
        let preview = TradePreviewArgs {
            account_id: Uuid::nil(),
            entry_price: dec!(200),
            stop_price: dec!(190),
            currency: Currency::USD,
        };
        let levels = ArgDispatcher::size_preview_levels(100, 4, dec!(10));
        let lines =
            ArgDispatcher::trade_size_preview_text_lines(&preview, 100, 150, dec!(1.5), &levels);

        assert_eq!(lines[0], "Position Size Preview");
        assert!(lines[3].contains("Risk/Share: 10 USD"));
        assert!(lines
            .iter()
            .any(|line| line.contains("L4 (1.5x): qty 150 | risk 1500 USD  <- current")));
    }

    #[test]
    fn test_trade_hypothesis_formatters_render_na_for_missing_values() {
        assert_eq!(ArgDispatcher::format_optional_decimal(None), "n/a");
        assert_eq!(ArgDispatcher::format_optional_percentage(None), "n/a");
        assert_eq!(
            ArgDispatcher::format_optional_decimal(Some(dec!(4.250000000))),
            "4.25"
        );
        assert_eq!(
            ArgDispatcher::format_optional_percentage(Some(dec!(1.600000000))),
            "1.6%"
        );
    }

    #[test]
    fn test_trade_hypothesis_text_lines_render_optional_values() {
        let args = TradeHypothesisArgs {
            preview: TradePreviewArgs {
                account_id: Uuid::nil(),
                entry_price: dec!(40),
                stop_price: dec!(38),
                currency: Currency::USD,
            },
            target_price: dec!(48),
            quantity: 100,
        };
        let report = TradeHypothesisReport {
            available_capital: Decimal::ZERO,
            capital_required: dec!(4000),
            capital_required_pct_of_available: None,
            risk_per_share: dec!(2),
            reward_per_share: dec!(8),
            max_loss: dec!(200),
            max_loss_pct_of_available: None,
            max_gain: dec!(800),
            max_gain_pct_of_available: None,
            risk_reward_ratio: Some(dec!(4)),
        };

        let lines = ArgDispatcher::trade_hypothesis_text_lines(&args, &report);
        assert_eq!(lines[0], "Trade Hypothesis");
        assert!(lines[4].contains("Available capital: 0 USD"));
        assert!(lines[5].contains("Capital required: 4000 USD (n/a)"));
        assert!(lines[6].contains("Risk/share: 2 USD | Reward/share: 8 USD | R:R 4"));
        assert!(lines[7].contains("Max loss: 200 USD (n/a)"));
        assert!(lines[8].contains("Max gain: 800 USD (n/a)"));
    }

    #[test]
    fn test_trade_hypothesis_payload_preserves_optional_fields() {
        let args = TradeHypothesisArgs {
            preview: TradePreviewArgs {
                account_id: Uuid::nil(),
                entry_price: dec!(40),
                stop_price: dec!(38),
                currency: Currency::USD,
            },
            target_price: dec!(48),
            quantity: 100,
        };
        let report = TradeHypothesisReport {
            available_capital: dec!(50_000),
            capital_required: dec!(4000),
            capital_required_pct_of_available: Some(dec!(8)),
            risk_per_share: dec!(2),
            reward_per_share: dec!(8),
            max_loss: dec!(200),
            max_loss_pct_of_available: Some(dec!(0.4)),
            max_gain: dec!(800),
            max_gain_pct_of_available: None,
            risk_reward_ratio: Some(dec!(4)),
        };

        let payload = ArgDispatcher::trade_hypothesis_payload(&args, &report);
        assert_eq!(payload["report"], "trade_hypothesis");
        assert_eq!(payload["scope"]["account_id"], Uuid::nil().to_string());
        assert_eq!(payload["data"]["capital_required_pct_of_available"], "8");
        assert_eq!(payload["data"]["max_loss_pct_of_available"], "0.4");
        assert!(payload["data"]["max_gain_pct_of_available"].is_null());
        assert_eq!(payload["data"]["risk_reward_ratio"], "4");
    }

    #[test]
    fn test_reporting_methods_execute_text_paths_with_account_scope() {
        let mut dispatcher = test_dispatcher();
        let (account, vehicle) = seed_account_and_vehicle(&mut dispatcher);
        dispatcher
            .trust
            .create_trade(
                model::DraftTrade {
                    account: account.clone(),
                    trading_vehicle: vehicle,
                    quantity: 10,
                    category: model::TradeCategory::Long,
                    currency: Currency::USD,
                    thesis: None,
                    sector: Some("tech".to_string()),
                    asset_class: Some("equity".to_string()),
                    context: None,
                },
                dec!(95),
                dec!(100),
                dec!(110),
            )
            .expect("seed trade creation should succeed");

        let account_only = report_args_matches(&["--account", &account.id.to_string()]);
        let perf_with_days =
            report_args_matches(&["--account", &account.id.to_string(), "--days", "30"]);
        let concentration = report_args_matches(&["--account", &account.id.to_string()]);

        dispatcher
            .drawdown_report(&account_only, ReportOutputFormat::Text)
            .expect("drawdown text report should succeed");
        dispatcher
            .performance_report(&perf_with_days, ReportOutputFormat::Text)
            .expect("performance text report should succeed");
        dispatcher
            .risk_report(&account_only, ReportOutputFormat::Text)
            .expect("risk text report should succeed");
        dispatcher
            .concentration_report(&concentration, ReportOutputFormat::Text)
            .expect("concentration text report should succeed");
        dispatcher
            .summary_report(&account_only, ReportOutputFormat::Text)
            .expect("summary text report should succeed");
    }

    #[test]
    fn test_reporting_methods_execute_aggregate_scope_paths_without_account_argument() {
        let mut dispatcher = test_dispatcher();
        let (_account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);
        let no_account = report_args_matches(&[]);
        let no_account_with_days = report_args_matches(&["--days", "7"]);
        let open_only = report_args_matches(&["--open-only"]);

        dispatcher
            .drawdown_report(&no_account, ReportOutputFormat::Text)
            .expect("drawdown aggregate report should succeed");
        dispatcher
            .performance_report(&no_account_with_days, ReportOutputFormat::Text)
            .expect("performance aggregate report should succeed");
        dispatcher
            .risk_report(&no_account, ReportOutputFormat::Text)
            .expect("risk aggregate report should succeed");
        dispatcher
            .concentration_report(&open_only, ReportOutputFormat::Text)
            .expect("concentration open-only aggregate report should succeed");
        dispatcher
            .summary_report(&no_account, ReportOutputFormat::Text)
            .expect("summary aggregate report should succeed");
    }

    #[test]
    fn test_attribution_benchmark_and_timeline_reports_execute_text_and_json_paths() {
        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (account, _closed) = seed_closed_target_trade(&mut dispatcher);
        let account_id = account.id.to_string();

        let attribution = ReportCommandBuilder::new()
            .attribution()
            .build()
            .get_matches_from([
                "report",
                "attribution",
                "--account",
                &account_id,
                "--by",
                "symbol",
                "--from",
                "2020-01-01",
                "--to",
                "2030-01-01",
            ]);
        let attribution_matches = attribution.subcommand_matches("attribution").unwrap();
        dispatcher
            .attribution_report(attribution_matches, ReportOutputFormat::Text)
            .expect("attribution text report should succeed");
        dispatcher
            .attribution_report(attribution_matches, ReportOutputFormat::Json)
            .expect("attribution json report should succeed");
        for (grouping, format) in [
            ("sector", ReportOutputFormat::Text),
            ("asset-class", ReportOutputFormat::Json),
        ] {
            let grouped = ReportCommandBuilder::new()
                .attribution()
                .build()
                .get_matches_from([
                    "report",
                    "attribution",
                    "--account",
                    &account_id,
                    "--by",
                    grouping,
                    "--from",
                    "2020-01-01",
                    "--to",
                    "2030-01-01",
                ]);
            dispatcher
                .attribution_report(grouped.subcommand_matches("attribution").unwrap(), format)
                .expect("grouped attribution report should succeed");
        }

        let benchmark = ReportCommandBuilder::new()
            .benchmark()
            .build()
            .get_matches_from([
                "report",
                "benchmark",
                "--account",
                &account_id,
                "--benchmark",
                "SPY",
                "--from",
                "2020-01-01",
                "--to",
                "2030-01-01",
            ]);
        let benchmark_matches = benchmark.subcommand_matches("benchmark").unwrap();
        dispatcher
            .benchmark_report(benchmark_matches, ReportOutputFormat::Text)
            .expect("benchmark text report should succeed");
        dispatcher
            .benchmark_report(benchmark_matches, ReportOutputFormat::Json)
            .expect("benchmark json report should succeed");

        for (granularity, format) in [
            ("day", ReportOutputFormat::Text),
            ("week", ReportOutputFormat::Json),
            ("month", ReportOutputFormat::Text),
        ] {
            let timeline = ReportCommandBuilder::new()
                .timeline()
                .build()
                .get_matches_from([
                    "report",
                    "timeline",
                    "--account",
                    &account_id,
                    "--granularity",
                    granularity,
                    "--from",
                    "2020-01-01",
                    "--to",
                    "2030-01-01",
                ]);
            dispatcher
                .timeline_report(timeline.subcommand_matches("timeline").unwrap(), format)
                .expect("timeline report should succeed");
        }
    }

    #[test]
    fn test_bias_summary_report_aggregates_mistakes_and_renders_formats() {
        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (account, closed) = seed_closed_target_trade(&mut dispatcher);
        seed_mistake(
            &mut dispatcher,
            closed.id,
            vec![
                MungerTendency::DeprivalSuperreaction,
                MungerTendency::SocialProof,
            ],
            false,
            MistakeErrorType::Commission,
        );
        seed_mistake(
            &mut dispatcher,
            closed.id,
            vec![MungerTendency::DeprivalSuperreaction],
            false,
            MistakeErrorType::Omission,
        );
        seed_mistake(
            &mut dispatcher,
            closed.id,
            vec![
                MungerTendency::DeprivalSuperreaction,
                MungerTendency::Lollapalooza,
            ],
            true,
            MistakeErrorType::Commission,
        );

        let account_id = account.id.to_string();
        let json_matches = ReportCommandBuilder::new()
            .bias_summary()
            .build()
            .get_matches_from([
                "report",
                "bias-summary",
                "--format",
                "json",
                "--account",
                &account_id,
                "--days",
                "7",
            ]);
        dispatcher
            .dispatch_report(&json_matches)
            .expect("bias summary json should dispatch");

        let human_matches = ReportCommandBuilder::new()
            .bias_summary()
            .build()
            .get_matches_from([
                "report",
                "bias-summary",
                "--format",
                "human",
                "--account",
                &account_id,
                "--days",
                "7",
            ]);
        dispatcher
            .dispatch_report(&human_matches)
            .expect("bias summary human should dispatch");

        let aggregation = dispatcher
            .trust
            .bias_aggregation_for_account(account.id, 7)
            .expect("bias aggregation should calculate");
        let payload = ArgDispatcher::bias_aggregation_payload(account.id, &aggregation);
        assert_eq!(payload["report"], "bias_summary");
        assert_eq!(payload["window_days"], 7);
        assert_eq!(payload["total_mistakes"], 3);
        assert_eq!(payload["most_frequent_tendency"]["tendency_number"], 14);
        assert_eq!(payload["most_frequent_tendency"]["count"], 3);
        assert_eq!(payload["lollapalooza_count"], 1);
        assert_eq!(payload["omission_count"], 1);
        assert_eq!(payload["commission_count"], 2);
        assert_eq!(payload["mechanical_antidote_needed"], true);
        assert_eq!(payload["standdown_recommended"], false);
    }

    #[test]
    fn test_advisor_commands_validate_inputs_and_history_defaults() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);

        let bad_uuid = advisor_configure_matches(&[
            "--confirm-protected",
            "secret",
            "--account",
            "bad",
            "--sector-limit",
            "40",
            "--asset-class-limit",
            "40",
            "--single-position-limit",
            "25",
        ]);
        let error = dispatcher
            .advisor_configure(&bad_uuid)
            .expect_err("invalid uuid should fail");
        assert!(error.to_string().contains("invalid_uuid"));

        let bad_decimal = advisor_configure_matches(&[
            "--confirm-protected",
            "secret",
            "--account",
            &account.id.to_string(),
            "--sector-limit",
            "bad",
            "--asset-class-limit",
            "40",
            "--single-position-limit",
            "25",
        ]);
        let error = dispatcher
            .advisor_configure(&bad_decimal)
            .expect_err("invalid decimal should fail");
        assert!(error.to_string().contains("invalid_decimal"));

        let bad_provider = advisor_configure_matches(&["--calendar-provider", "bad"]);
        let error = dispatcher
            .advisor_configure(&bad_provider)
            .expect_err("invalid provider should fail");
        assert!(error.to_string().contains("invalid_calendar_provider"));

        let mixed_modes = advisor_configure_matches(&[
            "--calendar-api-key",
            "secret",
            "--account",
            &account.id.to_string(),
            "--sector-limit",
            "40",
            "--asset-class-limit",
            "40",
            "--single-position-limit",
            "25",
        ]);
        let error = dispatcher
            .advisor_configure(&mixed_modes)
            .expect_err("mixed config modes should fail");
        assert!(error
            .to_string()
            .contains("advisor_configure_mode_conflict"));

        let check_bad_decimal = advisor_check_matches(&[
            "--account",
            &account.id.to_string(),
            "--symbol",
            "AAPL",
            "--entry",
            "bad",
            "--quantity",
            "10",
        ]);
        let error = dispatcher
            .advisor_check(&check_bad_decimal)
            .expect_err("invalid entry should fail");
        assert!(error.to_string().contains("invalid_decimal"));

        let status_bad_uuid = advisor_check_matches(&["--account", "bad"]);
        let error = dispatcher
            .advisor_status(&status_bad_uuid)
            .expect_err("invalid status uuid should fail");
        assert!(error.to_string().contains("invalid_uuid"));

        let history_default_days = advisor_history_matches(&["--account", &account.id.to_string()]);
        dispatcher
            .advisor_history(&history_default_days)
            .expect("history should support default days when no entries exist");

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_advisor_success_paths_emit_warnings_status_and_history() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (account, _) = seed_filled_trade(&mut dispatcher);
        let account_id = account.id.to_string();

        let configure = advisor_configure_matches(&[
            "--confirm-protected",
            "secret",
            "--account",
            &account_id,
            "--sector-limit",
            "50",
            "--asset-class-limit",
            "50",
            "--single-position-limit",
            "50",
        ]);
        dispatcher
            .advisor_configure(&configure)
            .expect("advisor thresholds should configure");

        let check = advisor_check_matches(&[
            "--account",
            &account_id,
            "--symbol",
            "MSFT",
            "--entry",
            "100",
            "--quantity",
            "2",
            "--sector",
            "technology",
            "--asset-class",
            "equity",
        ]);
        dispatcher
            .advisor_check(&check)
            .expect("concentrated proposal should produce advisory output");

        let history = dispatcher
            .trust
            .advisory_history_for_account(account.id, 30);
        assert_eq!(history.len(), 1);
        assert!(history[0].summary.contains("sector concentration"));

        let status = advisor_check_matches(&["--account", &account_id]);
        dispatcher
            .advisor_status(&status)
            .expect("filled concentrated portfolio should render status warnings");

        let history_matches = advisor_history_matches(&["--account", &account_id, "--days", "30"]);
        dispatcher
            .advisor_history(&history_matches)
            .expect("non-empty advisor history should render entries");

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_distribution_and_transfer_commands_validate_inputs_and_defaults() {
        let mut dispatcher = test_dispatcher();
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);

        let configure_bad_uuid = distribution_config_matches(&[
            "--account-id",
            "bad",
            "--earnings",
            "10",
            "--tax",
            "20",
            "--reinvestment",
            "70",
            "--threshold",
            "100",
            "--password",
            "pw",
        ]);
        let error = dispatcher
            .configure_distribution(&configure_bad_uuid)
            .expect_err("invalid uuid should fail");
        assert!(error.to_string().contains("invalid_uuid"));

        let configure_bad_decimal = distribution_config_matches(&[
            "--account-id",
            &account.id.to_string(),
            "--earnings",
            "bad",
            "--tax",
            "20",
            "--reinvestment",
            "70",
            "--threshold",
            "100",
            "--password",
            "pw",
        ]);
        let error = dispatcher
            .configure_distribution(&configure_bad_decimal)
            .expect_err("invalid decimal should fail");
        assert!(error.to_string().contains("invalid_decimal"));

        let execute_bad_amount = distribution_execute_matches(&[
            "--account-id",
            &account.id.to_string(),
            "--amount",
            "bad",
        ]);
        let error = dispatcher
            .execute_distribution(&execute_bad_amount)
            .expect_err("invalid amount should fail");
        assert!(error.to_string().contains("invalid_decimal"));

        let history_default_limit = distribution_history_matches(&[
            "--account-id",
            &account.id.to_string(),
            "--limit",
            "bad",
        ]);
        dispatcher
            .distribution_history(&history_default_limit)
            .expect("history should default to 20 for invalid limit");

        let rules_bad_uuid = distribution_rules_matches(&["--account-id", "bad"]);
        let error = dispatcher
            .distribution_rules(&rules_bad_uuid)
            .expect_err("invalid uuid should fail");
        assert!(error.to_string().contains("invalid_uuid"));

        let transfer_bad_from = transfer_matches(&[
            "--from",
            "bad",
            "--to",
            &account.id.to_string(),
            "--amount",
            "10",
            "--reason",
            "rebalance",
        ]);
        let error = dispatcher
            .transfer_accounts(&transfer_bad_from)
            .expect_err("invalid from uuid should fail");
        assert!(error.to_string().contains("invalid_uuid"));

        let transfer_bad_amount = transfer_matches(&[
            "--from",
            &account.id.to_string(),
            "--to",
            &account.id.to_string(),
            "--amount",
            "bad",
            "--reason",
            "rebalance",
        ]);
        let error = dispatcher
            .transfer_accounts(&transfer_bad_amount)
            .expect_err("invalid amount should fail");
        assert!(error.to_string().contains("invalid_decimal"));
    }

    #[test]
    fn test_parse_report_format_defaults_and_json() {
        let text_matches = Command::new("test")
            .arg(Arg::new("format").long("format").required(false))
            .get_matches_from(["test"]);
        assert_eq!(
            ArgDispatcher::parse_report_format(&text_matches),
            ReportOutputFormat::Text
        );

        let json_matches = Command::new("test")
            .arg(Arg::new("format").long("format").required(false))
            .get_matches_from(["test", "--format", "json"]);
        assert_eq!(
            ArgDispatcher::parse_report_format(&json_matches),
            ReportOutputFormat::Json
        );
    }

    #[test]
    fn test_parse_decimal_arg_valid_invalid_missing() {
        let ok_matches = Command::new("test")
            .arg(Arg::new("value").long("value").required(true))
            .get_matches_from(["test", "--value", "123.456"]);
        let parsed =
            ArgDispatcher::parse_decimal_arg(&ok_matches, "value", ReportOutputFormat::Json)
                .unwrap();
        assert_eq!(parsed.to_string(), "123.456");

        let bad_matches = Command::new("test")
            .arg(Arg::new("value").long("value").required(true))
            .get_matches_from(["test", "--value", "abc"]);
        let error =
            ArgDispatcher::parse_decimal_arg(&bad_matches, "value", ReportOutputFormat::Json)
                .unwrap_err();
        assert!(error.to_string().contains("invalid_decimal"));
        assert!(error.already_printed());

        let missing_matches = Command::new("test")
            .arg(Arg::new("value").long("value").required(false))
            .get_matches_from(["test"]);
        let missing_error =
            ArgDispatcher::parse_decimal_arg(&missing_matches, "value", ReportOutputFormat::Text)
                .unwrap_err();
        assert_eq!(
            missing_error.to_string(),
            "missing_argument: Missing argument --value"
        );
        assert!(missing_error.already_printed());
    }

    #[test]
    fn test_parse_uuid_arg_valid_invalid_missing() {
        let expected = Uuid::new_v4();
        let ok_matches = Command::new("test")
            .arg(Arg::new("account").long("account").required(true))
            .get_matches_from(["test", "--account", &expected.to_string()]);
        let parsed =
            ArgDispatcher::parse_uuid_arg(&ok_matches, "account", ReportOutputFormat::Json)
                .unwrap();
        assert_eq!(parsed, expected);

        let bad_matches = Command::new("test")
            .arg(Arg::new("account").long("account").required(true))
            .get_matches_from(["test", "--account", "not-a-uuid"]);
        let bad_error =
            ArgDispatcher::parse_uuid_arg(&bad_matches, "account", ReportOutputFormat::Json)
                .unwrap_err();
        assert!(bad_error.to_string().contains("invalid_uuid"));
        assert!(bad_error.already_printed());

        let missing_matches = Command::new("test")
            .arg(Arg::new("account").long("account").required(false))
            .get_matches_from(["test"]);
        let missing_error =
            ArgDispatcher::parse_uuid_arg(&missing_matches, "account", ReportOutputFormat::Text)
                .unwrap_err();
        assert_eq!(
            missing_error.to_string(),
            "missing_argument: Missing argument --account"
        );
        assert!(missing_error.already_printed());
    }

    #[test]
    fn test_parse_grade_weights_default_percent_permille_and_errors() {
        let no_weights = Command::new("test")
            .arg(Arg::new("weights").long("weights").required(false))
            .get_matches_from(["test"]);
        let defaults =
            ArgDispatcher::parse_grade_weights(&no_weights, ReportOutputFormat::Text).unwrap();
        assert_eq!(defaults.process, 400);
        assert_eq!(defaults.risk, 300);
        assert_eq!(defaults.execution, 200);
        assert_eq!(defaults.documentation, 100);

        let percent = Command::new("test")
            .arg(Arg::new("weights").long("weights").required(false))
            .get_matches_from(["test", "--weights", "30,20,30,20"]);
        let percent_weights =
            ArgDispatcher::parse_grade_weights(&percent, ReportOutputFormat::Text).unwrap();
        assert_eq!(percent_weights.process, 300);
        assert_eq!(percent_weights.risk, 200);
        assert_eq!(percent_weights.execution, 300);
        assert_eq!(percent_weights.documentation, 200);

        let permille = Command::new("test")
            .arg(Arg::new("weights").long("weights").required(false))
            .get_matches_from(["test", "--weights", "300,200,300,200"]);
        let permille_weights =
            ArgDispatcher::parse_grade_weights(&permille, ReportOutputFormat::Text).unwrap();
        assert_eq!(permille_weights.process, 300);
        assert_eq!(permille_weights.risk, 200);
        assert_eq!(permille_weights.execution, 300);
        assert_eq!(permille_weights.documentation, 200);

        let wrong_len = Command::new("test")
            .arg(Arg::new("weights").long("weights").required(false))
            .get_matches_from(["test", "--weights", "10,20,30"]);
        let wrong_len_error =
            ArgDispatcher::parse_grade_weights(&wrong_len, ReportOutputFormat::Text).unwrap_err();
        assert!(wrong_len_error.to_string().contains("invalid_weights"));

        let invalid_number = Command::new("test")
            .arg(Arg::new("weights").long("weights").required(false))
            .get_matches_from(["test", "--weights", "10,20,x,70"]);
        let invalid_number_error =
            ArgDispatcher::parse_grade_weights(&invalid_number, ReportOutputFormat::Text)
                .unwrap_err();
        assert!(invalid_number_error.to_string().contains("invalid_weights"));

        let invalid_sum = Command::new("test")
            .arg(Arg::new("weights").long("weights").required(false))
            .get_matches_from(["test", "--weights", "10,20,30,10"]);
        let invalid_sum_error =
            ArgDispatcher::parse_grade_weights(&invalid_sum, ReportOutputFormat::Text).unwrap_err();
        assert!(invalid_sum_error
            .to_string()
            .contains("Weights must sum to 100 (percent) or 1000 (permille)"));
    }

    #[test]
    fn test_category_to_str_maps_expected_values() {
        assert_eq!(
            super::category_to_str(model::TradingVehicleCategory::Crypto),
            "crypto"
        );
        assert_eq!(
            super::category_to_str(model::TradingVehicleCategory::Fiat),
            "fiat"
        );
        assert_eq!(
            super::category_to_str(model::TradingVehicleCategory::Stock),
            "stock"
        );
        assert_eq!(
            super::category_to_str(model::TradingVehicleCategory::Etf),
            "etf"
        );
        assert_eq!(
            super::category_to_str(model::TradingVehicleCategory::Bond),
            "bond"
        );
    }

    #[test]
    fn test_report_error_marks_error_as_printed() {
        let error: CliError =
            ArgDispatcher::report_error(ReportOutputFormat::Text, "test_code", "test_message");
        assert!(error.already_printed());
        assert_eq!(error.to_string(), "test_code: test_message");
    }

    #[test]
    fn test_decimal_string_rounds_and_normalizes() {
        assert_eq!(ArgDispatcher::decimal_string(dec!(1.2300000000)), "1.23");
        assert_eq!(
            ArgDispatcher::decimal_string(dec!(1.2345678912)),
            "1.23456789"
        );
    }

    #[test]
    fn test_summarize_agent_status_prioritizes_critical_then_warn() {
        assert_eq!(ArgDispatcher::summarize_agent_status(&[]), "ok");
        assert_eq!(
            ArgDispatcher::summarize_agent_status(&["warn:thing".to_string()]),
            "warn"
        );
        assert_eq!(
            ArgDispatcher::summarize_agent_status(&[
                "warn:thing".to_string(),
                "critical:other".to_string()
            ]),
            "critical"
        );
    }

    #[test]
    fn test_summary_agent_signals_returns_expected_breaches_and_actions() {
        let payload =
            ArgDispatcher::summary_agent_signals(dec!(21), dec!(61), dec!(16), dec!(-0.1));
        assert_eq!(payload["status"], "critical");
        let breaches = payload["breaches"]
            .as_array()
            .expect("breaches should be array");
        assert!(breaches
            .iter()
            .any(|v| v.as_str() == Some("critical:capital_at_risk_above_20pct")));
        assert!(breaches
            .iter()
            .any(|v| v.as_str() == Some("critical:concentration_above_60pct")));
        assert!(breaches
            .iter()
            .any(|v| v.as_str() == Some("critical:max_drawdown_above_15pct")));
        assert!(breaches
            .iter()
            .any(|v| v.as_str() == Some("warn:negative_expectancy")));
        assert!(!payload["recommended_actions"]
            .as_array()
            .expect("actions should be array")
            .is_empty());
    }

    #[test]
    fn test_metrics_agent_signals_handles_warn_and_critical_paths() {
        let payload = ArgDispatcher::metrics_agent_signals(dec!(-1), dec!(39), Some(dec!(0.41)), 5);
        assert_eq!(payload["status"], "critical");
        let breaches = payload["breaches"]
            .as_array()
            .expect("breaches should be array");
        assert!(breaches
            .iter()
            .any(|v| v.as_str() == Some("critical:negative_expectancy")));
        assert!(breaches
            .iter()
            .any(|v| v.as_str() == Some("warn:win_rate_below_40pct")));
        assert!(breaches
            .iter()
            .any(|v| v.as_str() == Some("warn:loss_streak_ge_5")));
        assert!(breaches
            .iter()
            .any(|v| v.as_str() == Some("critical:risk_of_ruin_proxy_above_40pct")));

        let ok_payload = ArgDispatcher::metrics_agent_signals(dec!(1), dec!(60), None, 1);
        assert_eq!(ok_payload["status"], "ok");
        assert_eq!(ok_payload["breaches"].as_array().expect("array").len(), 0);
    }

    #[test]
    fn test_level_payload_helpers_include_expected_fields() {
        let criterion = LevelCriterionProgress {
            key: "win_rate_percentage",
            comparator: ">=",
            actual: dec!(55),
            threshold: dec!(60),
            missing: dec!(5),
            met: false,
        };
        let path = LevelPathProgress {
            path: "performance_upgrade",
            trigger_type: LevelTrigger::PerformanceUpgrade,
            direction: LevelDirection::Upgrade,
            target_level: Some(4),
            criteria: vec![criterion.clone()],
            all_met: false,
        };
        let report = LevelProgressReport {
            current_level: 3,
            status: LevelStatus::Normal,
            upgrade_paths: vec![path.clone()],
            downgrade_paths: vec![],
        };

        let criterion_payload = ArgDispatcher::level_criterion_payload(&criterion);
        assert_eq!(criterion_payload["key"], "win_rate_percentage");
        assert_eq!(criterion_payload["actual"], "55");
        assert_eq!(criterion_payload["missing"], "5");

        let path_payload = ArgDispatcher::level_path_payload(&path);
        assert_eq!(path_payload["path"], "performance_upgrade");
        assert_eq!(path_payload["direction"], "Upgrade");
        assert_eq!(path_payload["target_level"], 4);

        let progress_payload = ArgDispatcher::level_progress_payload(&report);
        assert_eq!(progress_payload["current_level"], 3);
        assert_eq!(progress_payload["status"], "normal");
        assert_eq!(
            progress_payload["upgrade_paths"]
                .as_array()
                .expect("array")
                .len(),
            1
        );
    }

    #[test]
    fn test_level_evaluation_payload_includes_decision_and_apply_state() {
        let now = Utc
            .with_ymd_and_hms(2026, 2, 25, 10, 0, 0)
            .unwrap()
            .naive_utc();
        let account_id = Uuid::new_v4();
        let current = Level {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id,
            current_level: 3,
            risk_multiplier: dec!(1.0),
            status: LevelStatus::Normal,
            trades_at_level: 10,
            level_start_date: NaiveDate::from_ymd_opt(2026, 2, 1).expect("date"),
        };
        let applied = Level {
            current_level: 4,
            ..current.clone()
        };
        let outcome = LevelEvaluationOutcome {
            current_level: current,
            decision: Some(LevelDecision {
                target_level: 4,
                reason: "Strong performance".to_string(),
                trigger_type: LevelTrigger::PerformanceUpgrade,
                direction: LevelDirection::Upgrade,
            }),
            applied_level: Some(applied),
            progress: LevelProgressReport {
                current_level: 3,
                status: LevelStatus::Normal,
                upgrade_paths: vec![],
                downgrade_paths: vec![],
            },
        };

        let payload = ArgDispatcher::level_evaluation_payload(account_id, true, &outcome);
        assert_eq!(payload["report"], "level_evaluate");
        assert_eq!(payload["scope"]["account_id"], account_id.to_string());
        assert_eq!(payload["apply"], true);
        assert_eq!(payload["data"]["current_level"], 3);
        assert_eq!(payload["data"]["decision"]["target_level"], 4);
        assert_eq!(payload["data"]["decision"]["direction"], "Upgrade");
        assert_eq!(payload["data"]["applied_level"], 4);
    }

    #[test]
    fn test_concentration_helpers_sort_and_encode_warnings() {
        let groups = vec![
            ConcentrationGroup {
                name: "Tech".to_string(),
                trade_count: 2,
                total_capital_deployed: dec!(2000),
                realized_pnl: dec!(100),
                current_open_risk: dec!(1000),
            },
            ConcentrationGroup {
                name: "Energy".to_string(),
                trade_count: 1,
                total_capital_deployed: dec!(1000),
                realized_pnl: dec!(50),
                current_open_risk: dec!(500),
            },
        ];
        let groups_payload = ArgDispatcher::concentration_groups_json(&groups, dec!(1500));
        let arr = groups_payload.as_slice();
        assert_eq!(arr[0]["name"], "Tech");
        assert_eq!(arr[0]["open_risk_share_percentage"], "66.66666667");
        assert_eq!(arr[1]["name"], "Energy");
        assert_eq!(arr[1]["open_risk_share_percentage"], "33.33333333");

        let warnings = vec![ConcentrationWarning {
            group_name: "Tech".to_string(),
            risk_percentage: dec!(66.67),
            level: WarningLevel::High,
        }];
        let warnings_payload = ArgDispatcher::warnings_json(&warnings);
        assert_eq!(warnings_payload[0]["group_name"], "Tech");
        assert_eq!(warnings_payload[0]["risk_percentage"], "66.67");
        assert_eq!(warnings_payload[0]["level"], "High");
    }

    #[test]
    fn test_reports_reject_invalid_account_id_before_data_access() {
        let invalid_matches = Command::new("test")
            .arg(Arg::new("account").long("account").required(false))
            .arg(Arg::new("open-only").long("open-only"))
            .arg(
                Arg::new("days")
                    .long("days")
                    .value_parser(clap::value_parser!(u32)),
            )
            .get_matches_from(["test", "--account", "not-a-uuid"]);

        let mut dispatcher = test_dispatcher();
        let drawdown_error = dispatcher
            .drawdown_report(&invalid_matches, ReportOutputFormat::Json)
            .expect_err("invalid account id should fail");
        assert!(drawdown_error.to_string().contains("invalid_account_id"));

        let mut dispatcher = test_dispatcher();
        let performance_error = dispatcher
            .performance_report(&invalid_matches, ReportOutputFormat::Json)
            .expect_err("invalid account id should fail");
        assert!(performance_error.to_string().contains("invalid_account_id"));

        let mut dispatcher = test_dispatcher();
        let risk_error = dispatcher
            .risk_report(&invalid_matches, ReportOutputFormat::Json)
            .expect_err("invalid account id should fail");
        assert!(risk_error.to_string().contains("invalid_account_id"));

        let mut dispatcher = test_dispatcher();
        let concentration_error = dispatcher
            .concentration_report(&invalid_matches, ReportOutputFormat::Json)
            .expect_err("invalid account id should fail");
        assert!(concentration_error
            .to_string()
            .contains("invalid_account_id"));

        let mut dispatcher = test_dispatcher();
        let summary_error = dispatcher
            .summary_report(&invalid_matches, ReportOutputFormat::Json)
            .expect_err("invalid account id should fail");
        assert!(summary_error.to_string().contains("invalid_account_id"));
    }

    #[test]
    fn test_reports_surface_missing_account_data_for_valid_unknown_account_id() {
        let missing_account = Uuid::new_v4().to_string();
        let matches = report_args_matches(&["--account", &missing_account]);

        let mut dispatcher = test_dispatcher();
        let risk_error = dispatcher
            .risk_report(&matches, ReportOutputFormat::Json)
            .expect_err("missing account balance should fail risk report");
        assert!(risk_error
            .to_string()
            .contains("account_balance_unavailable"));

        let mut dispatcher = test_dispatcher();
        let summary_error = dispatcher
            .summary_report(&matches, ReportOutputFormat::Json)
            .expect_err("missing account should fail summary report");
        assert!(summary_error
            .to_string()
            .contains("summary_generation_failed"));
    }

    #[test]
    fn test_non_interactive_arg_detection_helpers() {
        let empty_accounts = account_matches(&[]);
        assert!(!ArgDispatcher::has_non_interactive_account_args(
            &empty_accounts
        ));
        let named_accounts = account_matches(&["--name", "demo"]);
        assert!(ArgDispatcher::has_non_interactive_account_args(
            &named_accounts
        ));

        let empty_trade = trade_matches(&[]);
        assert!(!ArgDispatcher::has_non_interactive_trade_args(&empty_trade));
        let trade_with_symbol = trade_matches(&["--symbol", "AAPL"]);
        assert!(ArgDispatcher::has_non_interactive_trade_args(
            &trade_with_symbol
        ));
    }

    #[test]
    fn test_ensure_protected_keyword_requires_argument_and_validates_value() {
        let _guard = env_guard();
        crate::protected_keyword::delete().expect("reset protected keyword state");
        crate::protected_keyword::store("secret").expect("seed protected keyword");

        let mut dispatcher = test_dispatcher();
        let missing = account_matches(&[]);
        let missing_error = dispatcher
            .ensure_protected_keyword(&missing, ReportOutputFormat::Text, "op")
            .expect_err("missing keyword should fail");
        assert!(missing_error
            .to_string()
            .contains("protected_keyword_required"));

        let mut dispatcher = test_dispatcher();
        let invalid = account_matches(&["--confirm-protected", "wrong"]);
        let invalid_error = dispatcher
            .ensure_protected_keyword(&invalid, ReportOutputFormat::Text, "op")
            .expect_err("invalid keyword should fail");
        assert!(invalid_error
            .to_string()
            .contains("protected_keyword_invalid"));

        let mut dispatcher = test_dispatcher();
        let valid = account_matches(&["--confirm-protected", "secret"]);
        dispatcher
            .ensure_protected_keyword(&valid, ReportOutputFormat::Text, "op")
            .expect("valid keyword should authorize");

        crate::protected_keyword::delete().expect("clear protected keyword state");
    }

    #[test]
    fn test_ensure_protected_keyword_reports_not_configured() {
        let _guard = env_guard();
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
        crate::protected_keyword::delete().expect("reset protected keyword state");

        let mut dispatcher = test_dispatcher();
        let matches = account_matches(&["--confirm-protected", "secret"]);
        let error = dispatcher
            .ensure_protected_keyword(&matches, ReportOutputFormat::Json, "op")
            .expect_err("missing configured keyword should fail first");

        assert!(error
            .to_string()
            .contains("protected_keyword_not_configured"));
    }

    #[test]
    fn test_account_and_trade_resolution_helpers_cover_success_and_not_found_paths() {
        let mut dispatcher = test_dispatcher();
        let (account, vehicle) = seed_account_and_vehicle(&mut dispatcher);
        let trade = dispatcher
            .trust
            .create_trade(
                model::DraftTrade {
                    account: account.clone(),
                    trading_vehicle: vehicle,
                    quantity: 1,
                    category: model::TradeCategory::Long,
                    currency: Currency::USD,
                    thesis: None,
                    sector: None,
                    asset_class: None,
                    context: None,
                },
                dec!(95),
                dec!(100),
                dec!(120),
            )
            .expect("seed trade");

        let by_name = dispatcher
            .resolve_account_arg(&account.name, ReportOutputFormat::Json)
            .expect("account by name");
        assert_eq!(by_name.id, account.id);

        let by_id = dispatcher
            .resolve_account_arg(&account.id.to_string(), ReportOutputFormat::Json)
            .expect("account by id");
        assert_eq!(by_id.id, account.id);

        let resolved_by_id = dispatcher
            .account_by_id(account.id, ReportOutputFormat::Json)
            .expect("account by id helper");
        assert_eq!(resolved_by_id.id, account.id);

        let found_trade = dispatcher
            .find_trade_by_id(trade.id, &[Status::New], ReportOutputFormat::Json)
            .expect("trade by id");
        assert_eq!(found_trade.id, trade.id);

        let missing_account_error = dispatcher
            .resolve_account_arg("does-not-exist", ReportOutputFormat::Json)
            .expect_err("unknown account should fail");
        assert!(missing_account_error
            .to_string()
            .contains("account_not_found"));

        let missing_uuid_error = dispatcher
            .resolve_account_arg(&Uuid::new_v4().to_string(), ReportOutputFormat::Json)
            .expect_err("unknown account uuid should fail");
        assert!(missing_uuid_error.to_string().contains("account_not_found"));

        let missing_id_error = dispatcher
            .account_by_id(Uuid::new_v4(), ReportOutputFormat::Json)
            .expect_err("unknown account id should fail");
        assert!(missing_id_error.to_string().contains("account_not_found"));

        let missing_trade_error = dispatcher
            .find_trade_by_id(
                Uuid::new_v4(),
                &[Status::Submitted],
                ReportOutputFormat::Json,
            )
            .expect_err("missing trade should fail");
        assert!(missing_trade_error.to_string().contains("trade_not_found"));
    }

    #[test]
    fn test_resolution_helpers_surface_account_read_failures() {
        let mut dispatcher =
            test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
        let account_id = Uuid::new_v4();

        let account_arg_error = dispatcher
            .resolve_account_arg(&account_id.to_string(), ReportOutputFormat::Json)
            .expect_err("account list failure should fail UUID lookup");
        assert_eq!(account_arg_error.code, "accounts_unavailable");

        let account_by_id_error = dispatcher
            .account_by_id(account_id, ReportOutputFormat::Json)
            .expect_err("account list failure should fail account_by_id");
        assert_eq!(account_by_id_error.code, "accounts_unavailable");

        let trade_lookup_error = dispatcher
            .find_trade_by_id(Uuid::new_v4(), &[Status::New], ReportOutputFormat::Json)
            .expect_err("account list failure should fail trade lookup");
        assert_eq!(trade_lookup_error.code, "accounts_unavailable");

        let no_account_matches = Command::new("level")
            .arg(Arg::new("account").long("account"))
            .get_matches_from(["level"]);
        let level_account_error = dispatcher
            .resolve_level_account_id(&no_account_matches, ReportOutputFormat::Json)
            .expect_err("account list failure should fail implicit level account resolution");
        assert_eq!(level_account_error.code, "accounts_unavailable");
    }

    #[test]
    fn test_level_dispatchers_surface_level_store_failures() {
        let _guard = env_guard();
        std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        crate::protected_keyword::delete().expect("reset protected keyword");

        let account_id = Uuid::new_v4().to_string();
        let mut dispatcher =
            test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::levels());

        let status = LevelCommandBuilder::new()
            .status()
            .build()
            .get_matches_from(["level", "status", "--account", &account_id]);
        let status_error = dispatcher
            .dispatch_level(&status)
            .expect_err("level status read failure should surface");
        assert_eq!(status_error.code, "level_status_failed");

        let history = LevelCommandBuilder::new()
            .history()
            .build()
            .get_matches_from(["level", "history", "--account", &account_id]);
        let history_error = dispatcher
            .dispatch_level(&history)
            .expect_err("level history read failure should surface");
        assert_eq!(history_error.code, "level_history_failed");

        let rules_show = LevelCommandBuilder::new()
            .rules()
            .build()
            .get_matches_from(["level", "rules", "show", "--account", &account_id]);
        let rules_show_error = dispatcher
            .dispatch_level(&rules_show)
            .expect_err("level rules read failure should surface");
        assert_eq!(rules_show_error.code, "level_rules_show_failed");

        let rules_set = LevelCommandBuilder::new()
            .rules()
            .build()
            .get_matches_from([
                "level",
                "rules",
                "set",
                "--account",
                &account_id,
                "--rule",
                "monthly_loss_downgrade_pct",
                "--value",
                "8",
                "--confirm-protected",
                "secret",
            ]);
        let rules_set_read_error = dispatcher
            .dispatch_level(&rules_set)
            .expect_err("level rules read failure should surface before update");
        assert_eq!(rules_set_read_error.code, "level_rules_read_failed");

        let mut dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::level_write_after_read(),
        );
        let rules_set_write_error = dispatcher
            .dispatch_level(&rules_set)
            .expect_err("level rules write failure should surface");
        assert_eq!(rules_set_write_error.code, "level_rules_set_failed");

        crate::protected_keyword::delete().expect("cleanup protected keyword");
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
        std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
    }

    #[test]
    fn test_protected_keyword_lifecycle_set_rotate_and_delete_requires_valid_confirmation() {
        let _guard = env_guard();
        std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
        crate::protected_keyword::delete().expect("reset protected keyword");
        let mut dispatcher = test_dispatcher();

        let missing_value = Command::new("keys")
            .arg(Arg::new("value").long("value"))
            .arg(Arg::new("confirm-protected").long("confirm-protected"))
            .get_matches_from(["keys"]);
        let missing_error = dispatcher
            .set_protected_keyword(&missing_value)
            .expect_err("missing value should fail");
        assert!(missing_error.to_string().contains("missing_value"));
        dispatcher.show_protected_keyword();

        let blank_initial = KeysCommandBuilder::new()
            .protected_set()
            .build()
            .get_matches_from(["keys", "protected-set", "--value", "   "]);
        let blank_initial_error = dispatcher
            .set_protected_keyword(
                blank_initial
                    .subcommand_matches("protected-set")
                    .expect("protected-set matches"),
            )
            .expect_err("blank initial keyword should fail in storage");
        assert!(blank_initial_error
            .to_string()
            .contains("protected_keyword_store_failed"));

        let initial_set = KeysCommandBuilder::new()
            .protected_set()
            .build()
            .get_matches_from(["keys", "protected-set", "--value", "first-secret"]);
        dispatcher
            .set_protected_keyword(
                initial_set
                    .subcommand_matches("protected-set")
                    .expect("protected-set matches"),
            )
            .expect("initial set should succeed");
        assert_eq!(
            crate::protected_keyword::read_expected().expect("keyword should be stored"),
            "first-secret"
        );
        dispatcher.show_protected_keyword();

        let rotate_missing_confirm = KeysCommandBuilder::new()
            .protected_set()
            .build()
            .get_matches_from(["keys", "protected-set", "--value", "second-secret"]);
        let rotate_missing_error = dispatcher
            .set_protected_keyword(
                rotate_missing_confirm
                    .subcommand_matches("protected-set")
                    .expect("protected-set matches"),
            )
            .expect_err("rotation without confirmation should fail");
        assert!(rotate_missing_error
            .to_string()
            .contains("protected_keyword_required"));

        let rotate_wrong_confirm = KeysCommandBuilder::new()
            .protected_set()
            .build()
            .get_matches_from([
                "keys",
                "protected-set",
                "--value",
                "second-secret",
                "--confirm-protected",
                "wrong",
            ]);
        let rotate_wrong_error = dispatcher
            .set_protected_keyword(
                rotate_wrong_confirm
                    .subcommand_matches("protected-set")
                    .expect("protected-set matches"),
            )
            .expect_err("rotation with wrong confirmation should fail");
        assert!(rotate_wrong_error
            .to_string()
            .contains("protected_keyword_invalid"));

        let rotate_blank_value = KeysCommandBuilder::new()
            .protected_set()
            .build()
            .get_matches_from([
                "keys",
                "protected-set",
                "--value",
                "   ",
                "--confirm-protected",
                "first-secret",
            ]);
        let rotate_blank_error = dispatcher
            .set_protected_keyword(
                rotate_blank_value
                    .subcommand_matches("protected-set")
                    .expect("protected-set matches"),
            )
            .expect_err("blank rotation keyword should fail in storage");
        assert!(rotate_blank_error
            .to_string()
            .contains("protected_keyword_store_failed"));
        assert_eq!(
            crate::protected_keyword::read_expected().expect("keyword after failed blank rotation"),
            "first-secret"
        );

        let rotate_valid = KeysCommandBuilder::new()
            .protected_set()
            .build()
            .get_matches_from([
                "keys",
                "protected-set",
                "--value",
                "second-secret",
                "--confirm-protected",
                "first-secret",
            ]);
        dispatcher
            .set_protected_keyword(
                rotate_valid
                    .subcommand_matches("protected-set")
                    .expect("protected-set matches"),
            )
            .expect("rotation with valid confirmation should succeed");
        assert_eq!(
            crate::protected_keyword::read_expected().expect("keyword after rotation"),
            "second-secret"
        );

        let delete_wrong_confirm = KeysCommandBuilder::new()
            .protected_delete()
            .build()
            .get_matches_from(["keys", "protected-delete", "--confirm-protected", "wrong"]);
        let delete_wrong_error = dispatcher
            .delete_protected_keyword(
                delete_wrong_confirm
                    .subcommand_matches("protected-delete")
                    .expect("protected-delete matches"),
            )
            .expect_err("deletion with wrong confirmation should fail");
        assert!(delete_wrong_error
            .to_string()
            .contains("protected_keyword_invalid"));

        let delete_valid = KeysCommandBuilder::new()
            .protected_delete()
            .build()
            .get_matches_from([
                "keys",
                "protected-delete",
                "--confirm-protected",
                "second-secret",
            ]);
        dispatcher
            .delete_protected_keyword(
                delete_valid
                    .subcommand_matches("protected-delete")
                    .expect("protected-delete matches"),
            )
            .expect("deletion with valid confirmation should succeed");
        assert!(
            crate::protected_keyword::read_expected().is_err(),
            "keyword should be deleted"
        );

        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        std::env::set_var("TRUST_DISABLE_KEYCHAIN", "0");
        let delete_store_failure = KeysCommandBuilder::new()
            .protected_delete()
            .build()
            .get_matches_from(["keys", "protected-delete", "--confirm-protected", "secret"]);
        let delete_store_error = dispatcher
            .delete_protected_keyword(
                delete_store_failure
                    .subcommand_matches("protected-delete")
                    .expect("protected-delete matches"),
            )
            .expect_err("keychain deletion failures should surface");
        assert!(delete_store_error
            .to_string()
            .contains("protected_keyword_delete_failed"));
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
        std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");

        crate::protected_keyword::delete().expect("cleanup protected keyword");
        std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
    }

    #[test]
    fn test_resolve_level_account_id_paths() {
        let mut dispatcher = test_dispatcher();

        let invalid_matches = Command::new("level")
            .arg(Arg::new("account").long("account"))
            .get_matches_from(["level", "--account", "not-a-uuid"]);
        let invalid_error = dispatcher
            .resolve_level_account_id(&invalid_matches, ReportOutputFormat::Json)
            .expect_err("invalid account id should fail");
        assert!(invalid_error.to_string().contains("invalid_account_id"));

        let account = dispatcher
            .trust
            .create_account(
                "solo",
                "single account",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("create account");
        let no_arg_matches = Command::new("level")
            .arg(Arg::new("account").long("account"))
            .get_matches_from(["level"]);
        let inferred = dispatcher
            .resolve_level_account_id(&no_arg_matches, ReportOutputFormat::Json)
            .expect("single account should be inferred");
        assert_eq!(inferred, account.id);

        dispatcher
            .trust
            .create_account(
                "second",
                "second account",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("create second account");
        let requires_selection = dispatcher
            .resolve_level_account_id(&no_arg_matches, ReportOutputFormat::Json)
            .expect_err("multiple accounts should require explicit selection");
        assert!(requires_selection
            .to_string()
            .contains("account_selection_required"));
    }

    fn level_eval_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv: Vec<&str> = vec!["level-eval"];
        argv.extend_from_slice(args);
        Command::new("level-eval")
            .arg(
                Arg::new("profitable-trades")
                    .long("profitable-trades")
                    .value_parser(clap::value_parser!(u32)),
            )
            .arg(Arg::new("win-rate").long("win-rate"))
            .arg(Arg::new("monthly-loss").long("monthly-loss"))
            .arg(Arg::new("largest-loss").long("largest-loss"))
            .arg(
                Arg::new("consecutive-wins")
                    .long("consecutive-wins")
                    .value_parser(clap::value_parser!(u32)),
            )
            .get_matches_from(argv)
    }

    #[test]
    fn test_level_snapshot_from_args_validates_required_and_decimal_fields() {
        let valid = level_eval_matches(&[
            "--profitable-trades",
            "7",
            "--win-rate",
            "62.5",
            "--monthly-loss",
            "4.2",
            "--largest-loss",
            "1.1",
            "--consecutive-wins",
            "3",
        ]);
        let snapshot = ArgDispatcher::level_snapshot_from_args(&valid, ReportOutputFormat::Json)
            .expect("valid snapshot should parse");
        let expected = LevelPerformanceSnapshot {
            profitable_trades: 7,
            win_rate_percentage: dec!(62.5),
            monthly_loss_percentage: dec!(4.2),
            largest_loss_percentage: dec!(1.1),
            consecutive_wins: 3,
        };
        assert_eq!(snapshot.profitable_trades, expected.profitable_trades);
        assert_eq!(snapshot.win_rate_percentage, expected.win_rate_percentage);
        assert_eq!(
            snapshot.monthly_loss_percentage,
            expected.monthly_loss_percentage
        );
        assert_eq!(
            snapshot.largest_loss_percentage,
            expected.largest_loss_percentage
        );
        assert_eq!(snapshot.consecutive_wins, expected.consecutive_wins);

        let missing_profitable = level_eval_matches(&[
            "--win-rate",
            "60",
            "--monthly-loss",
            "2",
            "--largest-loss",
            "1",
            "--consecutive-wins",
            "2",
        ]);
        let missing_profitable_error =
            ArgDispatcher::level_snapshot_from_args(&missing_profitable, ReportOutputFormat::Json)
                .expect_err("missing profitable trades should fail");
        assert!(missing_profitable_error
            .to_string()
            .contains("missing_profitable_trades"));

        let missing_consecutive = level_eval_matches(&[
            "--profitable-trades",
            "2",
            "--win-rate",
            "60",
            "--monthly-loss",
            "2",
            "--largest-loss",
            "1",
        ]);
        let missing_consecutive_error =
            ArgDispatcher::level_snapshot_from_args(&missing_consecutive, ReportOutputFormat::Json)
                .expect_err("missing consecutive wins should fail");
        assert!(missing_consecutive_error
            .to_string()
            .contains("missing_consecutive_wins"));

        let invalid_decimal = level_eval_matches(&[
            "--profitable-trades",
            "2",
            "--win-rate",
            "bad",
            "--monthly-loss",
            "2",
            "--largest-loss",
            "1",
            "--consecutive-wins",
            "2",
        ]);
        let invalid_decimal_error =
            ArgDispatcher::level_snapshot_from_args(&invalid_decimal, ReportOutputFormat::Json)
                .expect_err("invalid decimal should fail");
        assert!(invalid_decimal_error
            .to_string()
            .contains("invalid_decimal"));

        let invalid_monthly_loss = level_eval_matches(&[
            "--profitable-trades",
            "2",
            "--win-rate",
            "60",
            "--monthly-loss",
            "bad",
            "--largest-loss",
            "1",
            "--consecutive-wins",
            "2",
        ]);
        let invalid_monthly_loss_error = ArgDispatcher::level_snapshot_from_args(
            &invalid_monthly_loss,
            ReportOutputFormat::Json,
        )
        .expect_err("invalid monthly loss should fail");
        assert!(invalid_monthly_loss_error
            .to_string()
            .contains("invalid_decimal"));

        let invalid_largest_loss = level_eval_matches(&[
            "--profitable-trades",
            "2",
            "--win-rate",
            "60",
            "--monthly-loss",
            "2",
            "--largest-loss",
            "bad",
            "--consecutive-wins",
            "2",
        ]);
        let invalid_largest_loss_error = ArgDispatcher::level_snapshot_from_args(
            &invalid_largest_loss,
            ReportOutputFormat::Json,
        )
        .expect_err("invalid largest loss should fail");
        assert!(invalid_largest_loss_error
            .to_string()
            .contains("invalid_decimal"));
    }

    #[test]
    fn test_summary_agent_signals_warn_and_ok_paths() {
        let warn_payload =
            ArgDispatcher::summary_agent_signals(dec!(11), dec!(55), dec!(11), dec!(0.5));
        assert_eq!(warn_payload["status"], "warn");
        let warn_breaches = warn_payload["breaches"]
            .as_array()
            .expect("warn breaches array");
        assert!(warn_breaches
            .iter()
            .any(|entry| entry == "warn:capital_at_risk_above_10pct"));
        assert!(warn_breaches
            .iter()
            .any(|entry| entry == "warn:concentration_above_50pct"));
        assert!(warn_breaches
            .iter()
            .any(|entry| entry == "warn:max_drawdown_above_10pct"));

        let ok_payload = ArgDispatcher::summary_agent_signals(dec!(5), dec!(20), dec!(3), dec!(1));
        assert_eq!(ok_payload["status"], "ok");
        assert_eq!(
            ok_payload["breaches"]
                .as_array()
                .expect("ok breaches")
                .len(),
            0
        );
        assert_eq!(
            ok_payload["recommended_actions"]
                .as_array()
                .expect("ok actions")
                .len(),
            0
        );
    }

    #[test]
    fn test_metrics_agent_signals_warn_only_and_ok_paths() {
        let warn_payload =
            ArgDispatcher::metrics_agent_signals(dec!(1), dec!(35), Some(dec!(0.3)), 5);
        assert_eq!(warn_payload["status"], "warn");
        let warn_breaches = warn_payload["breaches"]
            .as_array()
            .expect("warn breaches array");
        assert!(warn_breaches
            .iter()
            .any(|entry| entry == "warn:win_rate_below_40pct"));
        assert!(warn_breaches
            .iter()
            .any(|entry| entry == "warn:loss_streak_ge_5"));
        assert!(warn_breaches
            .iter()
            .any(|entry| entry == "warn:risk_of_ruin_proxy_above_20pct"));
        assert!(!warn_breaches
            .iter()
            .any(|entry| entry == "critical:negative_expectancy"));

        let ok_payload =
            ArgDispatcher::metrics_agent_signals(dec!(1.5), dec!(55), Some(dec!(0.1)), 2);
        assert_eq!(ok_payload["status"], "ok");
        assert_eq!(
            ok_payload["breaches"]
                .as_array()
                .expect("ok breaches")
                .len(),
            0
        );
    }

    #[test]
    fn test_print_level_progress_text_handles_empty_and_non_empty_paths() {
        let empty_progress = LevelProgressReport {
            current_level: 1,
            status: LevelStatus::Normal,
            upgrade_paths: vec![],
            downgrade_paths: vec![],
        };
        ArgDispatcher::print_level_progress_text(&empty_progress);

        let path = LevelPathProgress {
            path: "policy",
            trigger_type: LevelTrigger::PerformanceUpgrade,
            direction: LevelDirection::Upgrade,
            target_level: Some(2),
            all_met: false,
            criteria: vec![LevelCriterionProgress {
                key: "win_rate",
                comparator: ">=",
                actual: dec!(55),
                threshold: dec!(60),
                missing: dec!(5),
                met: false,
            }],
        };
        let non_empty_progress = LevelProgressReport {
            current_level: 1,
            status: LevelStatus::Normal,
            upgrade_paths: vec![path.clone()],
            downgrade_paths: vec![path],
        };
        ArgDispatcher::print_level_progress_text(&non_empty_progress);
    }

    #[test]
    fn test_print_level_path_text_handles_missing_target_level() {
        let path = LevelPathProgress {
            path: "upper-bound",
            trigger_type: LevelTrigger::PerformanceUpgrade,
            direction: LevelDirection::Upgrade,
            target_level: None,
            all_met: true,
            criteria: vec![],
        };

        ArgDispatcher::print_level_path_text("Upgrade", &path);
    }

    #[test]
    fn test_create_account_non_interactive_missing_required_fields() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();

        let matches = account_matches(&["--confirm-protected", "secret", "--description", "x"]);
        let error = dispatcher
            .create_account(&matches)
            .expect_err("missing name should fail");
        assert!(error.to_string().contains("missing_name"));

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_create_account_non_interactive_validates_remaining_required_and_enum_fields() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");

        let assert_error = |args: &[&str], expected_code: &str| {
            let mut dispatcher = test_dispatcher();
            let matches = account_matches(args);
            let error = dispatcher
                .create_account(&matches)
                .expect_err("invalid account args should fail");
            assert!(
                error.to_string().contains(expected_code),
                "expected `{expected_code}` in `{error}`"
            );
        };

        assert_error(
            &["--confirm-protected", "secret", "--name", "missing-desc"],
            "missing_description",
        );
        assert_error(
            &[
                "--confirm-protected",
                "secret",
                "--name",
                "missing-env",
                "--description",
                "missing env",
            ],
            "missing_environment",
        );
        assert_error(
            &[
                "--confirm-protected",
                "secret",
                "--name",
                "missing-taxes",
                "--description",
                "missing taxes",
                "--environment",
                "paper",
            ],
            "missing_taxes",
        );
        assert_error(
            &[
                "--confirm-protected",
                "secret",
                "--name",
                "missing-earnings",
                "--description",
                "missing earnings",
                "--environment",
                "paper",
                "--taxes",
                "20",
            ],
            "missing_earnings",
        );
        assert_error(
            &[
                "--confirm-protected",
                "secret",
                "--name",
                "bad-type",
                "--description",
                "bad type",
                "--environment",
                "paper",
                "--taxes",
                "20",
                "--earnings",
                "10",
                "--type",
                "margin",
            ],
            "invalid_account_type",
        );
        assert_error(
            &[
                "--confirm-protected",
                "secret",
                "--name",
                "bad-broker",
                "--description",
                "bad broker",
                "--environment",
                "paper",
                "--taxes",
                "20",
                "--earnings",
                "10",
                "--broker",
                "manual",
            ],
            "invalid_broker",
        );

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_create_account_non_interactive_rejects_invalid_parent_uuid() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();

        let matches = account_matches(&[
            "--confirm-protected",
            "secret",
            "--name",
            "child",
            "--description",
            "child",
            "--environment",
            "paper",
            "--taxes",
            "20",
            "--earnings",
            "10",
            "--parent",
            "not-a-uuid",
        ]);
        let error = dispatcher
            .create_account(&matches)
            .expect_err("invalid parent uuid should fail");
        assert!(error.to_string().contains("invalid_uuid"));

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_create_account_non_interactive_requires_ibkr_primary_account_id() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();

        let matches = account_matches(&[
            "--confirm-protected",
            "secret",
            "--name",
            "ibkr-main",
            "--description",
            "ibkr trading",
            "--environment",
            "paper",
            "--taxes",
            "20",
            "--earnings",
            "10",
            "--broker",
            "ibkr",
        ]);
        let error = dispatcher
            .create_account(&matches)
            .expect_err("missing ibkr broker account id should fail");
        assert!(error.to_string().contains("missing_broker_account_id"));

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_create_account_non_interactive_validates_environment_and_decimals() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");

        let invalid_environment = account_matches(&[
            "--confirm-protected",
            "secret",
            "--name",
            "bad-env",
            "--description",
            "bad env",
            "--environment",
            "demo",
            "--taxes",
            "20",
            "--earnings",
            "10",
        ]);
        let mut dispatcher = test_dispatcher();
        let env_error = dispatcher
            .create_account(&invalid_environment)
            .expect_err("invalid environment should fail");
        assert!(env_error.to_string().contains("invalid_environment"));

        let invalid_taxes = account_matches(&[
            "--confirm-protected",
            "secret",
            "--name",
            "bad-tax",
            "--description",
            "bad tax",
            "--environment",
            "paper",
            "--taxes",
            "abc",
            "--earnings",
            "10",
        ]);
        let mut dispatcher = test_dispatcher();
        let tax_error = dispatcher
            .create_account(&invalid_taxes)
            .expect_err("invalid taxes should fail");
        assert!(tax_error.to_string().contains("invalid_decimal"));

        let invalid_earnings = account_matches(&[
            "--confirm-protected",
            "secret",
            "--name",
            "bad-earnings",
            "--description",
            "bad earnings",
            "--environment",
            "paper",
            "--taxes",
            "20",
            "--earnings",
            "abc",
        ]);
        let mut dispatcher = test_dispatcher();
        let earnings_error = dispatcher
            .create_account(&invalid_earnings)
            .expect_err("invalid earnings should fail");
        assert!(earnings_error.to_string().contains("invalid_decimal"));

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_create_account_non_interactive_success_with_broker_profiles() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();

        let ibkr_primary = account_matches(&[
            "--confirm-protected",
            "secret",
            "--name",
            "ibkr-profile",
            "--description",
            "ibkr primary",
            "--environment",
            "paper",
            "--taxes",
            "20",
            "--earnings",
            "10",
            "--broker",
            "ibkr",
            "--broker-account-id",
            "U1234567",
        ]);
        dispatcher
            .create_account(&ibkr_primary)
            .expect("ibkr primary account should be created");
        let created = dispatcher
            .trust
            .search_account("ibkr-profile")
            .expect("created ibkr account should be searchable");
        assert_eq!(created.account_type, AccountType::Primary);
        assert_eq!(created.broker_kind, BrokerKind::Ibkr);
        assert_eq!(created.broker_account_id.as_deref(), Some("U1234567"));

        let parent_id = created.id.to_string();
        let earnings = account_matches(&[
            "--confirm-protected",
            "secret",
            "--name",
            "ibkr-earnings",
            "--description",
            "ibkr earnings",
            "--environment",
            "paper",
            "--taxes",
            "20",
            "--earnings",
            "10",
            "--type",
            "earnings",
            "--broker",
            "ibkr",
            "--parent",
            &parent_id,
        ]);
        dispatcher
            .create_account(&earnings)
            .expect("non-primary ibkr account with parent does not require broker account id");
        let earnings = dispatcher
            .trust
            .search_account("ibkr-earnings")
            .expect("created earnings account should be searchable");
        assert_eq!(earnings.account_type, AccountType::Earnings);
        assert_eq!(earnings.broker_kind, BrokerKind::Ibkr);
        assert!(earnings.broker_account_id.is_none());
        assert_eq!(earnings.parent_account_id, Some(created.id));

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_create_account_non_interactive_success_with_hierarchy() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();

        let parent = dispatcher
            .trust
            .create_account("parent", "root", Environment::Paper, dec!(15), dec!(5))
            .expect("create parent account");

        let matches = account_matches(&[
            "--confirm-protected",
            "secret",
            "--name",
            "child",
            "--description",
            "child account",
            "--environment",
            "paper",
            "--taxes",
            "20",
            "--earnings",
            "10",
            "--type",
            "reinvestment",
            "--parent",
            &parent.id.to_string(),
        ]);

        assert!(
            dispatcher.create_account(&matches).is_ok(),
            "non-interactive create should succeed"
        );

        let created = dispatcher
            .trust
            .search_account("child")
            .expect("find created child account");
        assert_eq!(created.account_type, AccountType::Reinvestment);
        assert_eq!(created.parent_account_id, Some(parent.id));
        assert_eq!(created.environment, Environment::Paper);

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_create_account_non_interactive_surfaces_core_create_errors() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();

        let args = [
            "--confirm-protected",
            "secret",
            "--name",
            "duplicate-account",
            "--description",
            "original",
            "--environment",
            "paper",
            "--taxes",
            "20",
            "--earnings",
            "10",
        ];

        let first = account_matches(&args);
        dispatcher
            .create_account(&first)
            .expect("first account creation should succeed");

        let duplicate = account_matches(&args);
        let error = dispatcher
            .create_account(&duplicate)
            .expect_err("duplicate account creation should fail");
        assert!(error.to_string().contains("create_account_failed"));

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_delete_account_requires_protected_keyword_and_deletes_zero_balance_account() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();
        let account = dispatcher
            .trust
            .create_account(
                "delete-cli",
                "delete",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");
        let account_id = account.id.to_string();

        let missing = account_matches(&["--account", &account_id]);
        let error = dispatcher
            .delete_account(&missing)
            .expect_err("protected keyword should be required");
        assert!(error.to_string().contains("protected_keyword_required"));

        let invalid = account_matches(&[
            "--account",
            &account_id,
            "--confirm-protected",
            "wrong-secret",
        ]);
        let error = dispatcher
            .delete_account(&invalid)
            .expect_err("invalid protected keyword should fail");
        assert!(error.to_string().contains("protected_keyword_invalid"));

        let valid = account_matches(&["--account", &account_id, "--confirm-protected", "secret"]);
        dispatcher
            .delete_account(&valid)
            .expect("account deletion should succeed");
        assert!(dispatcher.trust.search_account("delete-cli").is_err());
        assert!(dispatcher
            .trust
            .search_all_accounts()
            .expect("accounts should load")
            .is_empty());

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_delete_account_rejects_non_zero_balance_unless_forced() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();
        let account = dispatcher
            .trust
            .create_account(
                "delete-balance-cli",
                "delete",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account should be created");
        dispatcher
            .trust
            .create_transaction(
                &account,
                &TransactionCategory::Deposit,
                dec!(50),
                &Currency::USD,
            )
            .expect("deposit should create non-zero balance");
        let account_id = account.id.to_string();

        let safe_delete =
            account_matches(&["--account", &account_id, "--confirm-protected", "secret"]);
        let error = dispatcher
            .delete_account(&safe_delete)
            .expect_err("non-zero account should require force");
        assert!(error.to_string().contains("non-zero balance"));

        let forced = account_matches(&[
            "--account",
            &account_id,
            "--force",
            "--confirm-protected",
            "secret",
        ]);
        dispatcher
            .delete_account(&forced)
            .expect("force should bypass zero-balance check");
        assert!(dispatcher
            .trust
            .search_account("delete-balance-cli")
            .is_err());

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_delete_account_rejects_open_trades() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();
        let (account, _trade) = seed_new_trade(&mut dispatcher);
        let account_id = account.id.to_string();
        let matches = account_matches(&[
            "--account",
            &account_id,
            "--force",
            "--confirm-protected",
            "secret",
        ]);

        let error = dispatcher
            .delete_account(&matches)
            .expect_err("account with open trades should not delete");
        assert!(error.to_string().contains("open trade"));

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_create_trade_interactive_dispatch_builds_trade_from_scripted_dialog() {
        let mut dispatcher = test_dispatcher();
        let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

        crate::dialogs::scripted_reset();
        crate::dialogs::scripted_push_select(Ok(Some(0))); // account
        crate::dialogs::scripted_push_select(Ok(Some(0))); // trading vehicle
        crate::dialogs::scripted_push_select(Ok(Some(0))); // long category
        crate::dialogs::scripted_push_input(Ok("100".to_string()));
        crate::dialogs::scripted_push_input(Ok("95".to_string()));
        crate::dialogs::scripted_push_select(Ok(Some(1))); // stop-limit safety order
        crate::dialogs::scripted_push_select(Ok(Some(0))); // USD balance
        crate::dialogs::scripted_push_input(Ok("2".to_string()));
        crate::dialogs::scripted_push_input(Ok("110".to_string()));
        crate::dialogs::scripted_push_input(Ok("scripted breakout".to_string()));
        crate::dialogs::scripted_push_input(Ok("technology".to_string()));
        crate::dialogs::scripted_push_input(Ok("equity".to_string()));
        crate::dialogs::scripted_push_input(Ok("daily setup".to_string()));

        let matches = trade_matches(&[]);
        dispatcher
            .create_trade(&matches)
            .expect("scripted interactive trade creation should succeed");

        let created = dispatcher
            .trust
            .search_trades(account.id, Status::New)
            .expect("new trades should load");
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].trading_vehicle.symbol, "AAPL");
        assert_eq!(created[0].entry.quantity, 2);
        assert_eq!(created[0].safety_stop.category, OrderCategory::StopLimit);
        assert_eq!(created[0].thesis.as_deref(), Some("scripted breakout"));

        crate::dialogs::scripted_reset();
    }

    #[test]
    fn test_session_open_interactive_persists_plan_and_blocks_duplicate_open() {
        let mut dispatcher = test_dispatcher();
        let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

        crate::dialogs::scripted_reset();
        crate::dialogs::scripted_push_select(Ok(Some(1)));
        crate::dialogs::scripted_push_input(Ok("breakout, pullback".to_string()));
        crate::dialogs::scripted_push_input(Ok("2".to_string()));
        crate::dialogs::scripted_push_input(Ok("Wait for A-grade continuation setups".to_string()));
        crate::dialogs::scripted_push_input(Ok("Take one clean setup and honor stops".to_string()));
        crate::dialogs::scripted_push_input(Ok("Chase a low-quality trade".to_string()));

        dispatcher
            .session_open(
                &session_matches(&["--account", &account.name]),
                ReportOutputFormat::Text,
            )
            .expect("session open should persist plan");

        let open = dispatcher
            .trust
            .open_session_for_account(account.id)
            .expect("open session lookup should succeed")
            .expect("session should be open");
        assert_eq!(open.regime, SessionRegime::Normal);
        assert_eq!(open.permitted_setups, vec!["breakout", "pullback"]);
        assert_eq!(open.max_positions, 2);
        assert!(open.closed_at.is_none());

        let duplicate = dispatcher
            .session_open(
                &session_matches(&["--account", &account.name]),
                ReportOutputFormat::Text,
            )
            .expect_err("duplicate open session should fail before dialog");
        assert_eq!(duplicate.code, "session_already_open");

        crate::dialogs::scripted_reset();
    }

    #[test]
    fn test_session_close_reviews_unplanned_trades_and_list_formats() {
        let mut dispatcher = test_dispatcher();
        let (account, vehicle) = seed_account_and_vehicle(&mut dispatcher);

        crate::dialogs::scripted_reset();
        crate::dialogs::scripted_push_select(Ok(Some(1)));
        crate::dialogs::scripted_push_input(Ok("breakout, pullback".to_string()));
        crate::dialogs::scripted_push_input(Ok("1".to_string()));
        crate::dialogs::scripted_push_input(Ok("Only take planned setups".to_string()));
        crate::dialogs::scripted_push_input(Ok("One A setup maximum".to_string()));
        crate::dialogs::scripted_push_input(Ok("Any unplanned setup".to_string()));
        dispatcher
            .session_open(
                &session_matches(&["--account", &account.name]),
                ReportOutputFormat::Text,
            )
            .expect("session should open");

        for setup in ["breakout", "mean reversion"] {
            dispatcher
                .trust
                .create_trade(
                    model::DraftTrade {
                        account: account.clone(),
                        trading_vehicle: vehicle.clone(),
                        quantity: 1,
                        category: model::TradeCategory::Long,
                        currency: Currency::USD,
                        thesis: Some(format!("{setup} thesis")),
                        sector: Some("technology".to_string()),
                        asset_class: Some("equity".to_string()),
                        context: Some(setup.to_string()),
                    },
                    dec!(95),
                    dec!(100),
                    dec!(110),
                )
                .expect("session trade should create");
        }

        let open = dispatcher
            .trust
            .open_session_for_account(account.id)
            .expect("open session lookup")
            .expect("open session");
        let trades = dispatcher
            .trust
            .trades_for_account_in_period(account.id, open.opened_at, Utc::now().naive_utc())
            .expect("session trades should load");
        let rows = ArgDispatcher::session_trade_review_rows(&open, &trades);
        assert_eq!(rows.iter().filter(|row| row.planned).count(), 1);
        assert_eq!(rows.iter().filter(|row| !row.planned).count(), 1);

        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_input(Ok("One unplanned trade slipped in.".to_string()));
        dispatcher
            .session_close(&session_matches(&[]), ReportOutputFormat::Text)
            .expect("session close should persist review");

        assert!(dispatcher
            .trust
            .open_session_for_account(account.id)
            .expect("open session lookup after close")
            .is_none());
        let sessions = dispatcher
            .trust
            .session_plans_for_account(
                account.id,
                chrono::NaiveDateTime::MIN,
                Utc::now().naive_utc(),
            )
            .expect("session history should load");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_grade.as_deref(), Some("A"));
        assert_eq!(
            sessions[0].adherence_notes.as_deref(),
            Some("One unplanned trade slipped in.")
        );

        dispatcher
            .session_list(
                &session_matches(&["--account", &account.name, "--format", "json"]),
                ReportOutputFormat::Json,
            )
            .expect("json session list should render");
        dispatcher
            .session_list(
                &session_matches(&["--account", &account.name, "--format", "human"]),
                ReportOutputFormat::Text,
            )
            .expect("human session list should render");

        crate::dialogs::scripted_reset();
    }

    #[test]
    fn test_session_close_fails_when_no_session_is_open() {
        let mut dispatcher = test_dispatcher();
        let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

        let no_account_error = dispatcher
            .session_close(&session_matches(&[]), ReportOutputFormat::Text)
            .expect_err("close without an open session should fail");
        assert_eq!(no_account_error.code, "no_open_session");

        let account_error = dispatcher
            .session_close(
                &session_matches(&["--account", &account.name]),
                ReportOutputFormat::Text,
            )
            .expect_err("close for an account without a session should fail");
        assert_eq!(account_error.code, "no_open_session");
    }

    #[test]
    fn test_trade_interactive_dispatch_funds_submits_and_syncs_selected_trade() {
        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (account, _trade) = seed_new_trade(&mut dispatcher);
        dispatcher
            .trust
            .configure_advisory_thresholds(
                account.id,
                core::services::AdvisoryThresholds {
                    sector_limit_pct: dec!(100),
                    asset_class_limit_pct: dec!(100),
                    single_position_limit_pct: dec!(100),
                },
            )
            .expect("configure permissive advisory thresholds");

        crate::dialogs::scripted_reset();
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_confirm(Ok(true));
        let fund_matches = TradeCommandBuilder::new()
            .fund_trade()
            .build()
            .get_matches_from(["trade", "fund"]);
        dispatcher
            .create_funding(fund_matches.subcommand_matches("fund").unwrap())
            .expect("scripted interactive funding should succeed");
        let funded = dispatcher
            .trust
            .search_trades(account.id, Status::Funded)
            .expect("funded trades should load");
        assert_eq!(funded.len(), 1);

        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        let submit_matches = TradeCommandBuilder::new()
            .submit_trade()
            .build()
            .get_matches_from(["trade", "submit"]);
        dispatcher
            .create_submit(submit_matches.subcommand_matches("submit").unwrap())
            .expect("scripted interactive submit should succeed");
        let submitted = dispatcher
            .trust
            .search_trades(account.id, Status::Submitted)
            .expect("submitted trades should load");
        assert_eq!(submitted.len(), 1);

        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        let sync_matches = TradeCommandBuilder::new()
            .sync_trade()
            .build()
            .get_matches_from(["trade", "sync"]);
        dispatcher
            .create_sync(sync_matches.subcommand_matches("sync").unwrap())
            .expect("scripted interactive sync should succeed");
        let filled = dispatcher
            .trust
            .search_trades(account.id, Status::Filled)
            .expect("filled trades should load");
        assert_eq!(filled.len(), 1);

        crate::dialogs::scripted_reset();
    }

    #[test]
    fn test_trade_interactive_dispatch_cancels_selected_funded_trade() {
        let mut dispatcher = test_dispatcher();
        let (account, trade) = seed_new_trade(&mut dispatcher);
        dispatcher.trust.fund_trade(&trade).expect("fund trade");

        crate::dialogs::scripted_reset();
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        let cancel_matches = TradeCommandBuilder::new()
            .cancel_trade()
            .build()
            .get_matches_from(["trade", "cancel"]);
        dispatcher
            .create_cancel(cancel_matches.subcommand_matches("cancel").unwrap())
            .expect("scripted interactive cancel should succeed");

        let canceled = dispatcher
            .trust
            .search_trades(account.id, Status::Canceled)
            .expect("canceled trades should load");
        assert_eq!(canceled.len(), 1);

        crate::dialogs::scripted_reset();
    }

    #[test]
    fn test_trade_interactive_dispatch_manual_fill_error_and_manual_exit_paths() {
        let mut fill_dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (fill_account, fill_trade) = seed_new_trade(&mut fill_dispatcher);
        let (funded, _, _, _) = fill_dispatcher
            .trust
            .fund_trade(&fill_trade)
            .expect("fund trade before manual fill");
        fill_dispatcher
            .trust
            .submit_trade(&funded)
            .expect("submit trade before manual fill");

        crate::dialogs::scripted_reset();
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_input(Ok("1.25".to_string()));
        fill_dispatcher.create_fill();
        let filled_after_manual_fill_attempt = fill_dispatcher
            .trust
            .search_trades(fill_account.id, Status::Filled)
            .expect("filled trades should load after manual fill attempt");
        assert!(
            filled_after_manual_fill_attempt.is_empty(),
            "manual fill without a broker-reported average fill price should not mutate status"
        );
        let still_submitted = fill_dispatcher
            .trust
            .search_trades(fill_account.id, Status::Submitted)
            .expect("submitted trades should remain loadable");
        assert_eq!(still_submitted.len(), 1);

        let mut stop_dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (stop_account, _filled_trade) = seed_filled_trade(&mut stop_dispatcher);
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_input(Ok("2.00".to_string()));
        stop_dispatcher.create_stop();
        let stopped = stop_dispatcher
            .trust
            .search_trades(stop_account.id, Status::ClosedStopLoss)
            .expect("stopped trades should load");
        assert!(
            stopped.is_empty(),
            "manual stop without a broker-reported stop fill price should not mutate status"
        );
        let still_filled_after_stop_attempt = stop_dispatcher
            .trust
            .search_trades(stop_account.id, Status::Filled)
            .expect("filled trades should remain loadable after manual stop attempt");
        assert_eq!(still_filled_after_stop_attempt.len(), 1);

        let mut target_dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (target_account, _filled_trade) = seed_filled_trade(&mut target_dispatcher);
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_input(Ok("2.50".to_string()));
        target_dispatcher.create_target();
        let targeted = target_dispatcher
            .trust
            .search_trades(target_account.id, Status::ClosedTarget)
            .expect("target-closed trades should load");
        assert!(
            targeted.is_empty(),
            "manual target without a broker-reported target fill price should not mutate status"
        );
        let still_filled_after_target_attempt = target_dispatcher
            .trust
            .search_trades(target_account.id, Status::Filled)
            .expect("filled trades should remain loadable after manual target attempt");
        assert_eq!(still_filled_after_target_attempt.len(), 1);

        crate::dialogs::scripted_reset();
    }

    #[test]
    fn test_trade_non_interactive_fund_submit_sync_and_cancel_success_paths() {
        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (account, trade) = seed_new_trade(&mut dispatcher);
        let trade_id = trade.id.to_string();

        let fund_matches = TradeCommandBuilder::new()
            .fund_trade()
            .build()
            .get_matches_from(["trade", "fund", "--trade-id", &trade_id]);
        dispatcher
            .create_funding(fund_matches.subcommand_matches("fund").unwrap())
            .expect("non-interactive funding should succeed");
        let funded = dispatcher
            .trust
            .search_trades(account.id, Status::Funded)
            .expect("funded trade should load");
        assert_eq!(funded.len(), 1);
        let funded_id = funded[0].id.to_string();

        let submit_matches = TradeCommandBuilder::new()
            .submit_trade()
            .build()
            .get_matches_from(["trade", "submit", "--trade-id", &funded_id]);
        dispatcher
            .create_submit(submit_matches.subcommand_matches("submit").unwrap())
            .expect("non-interactive submit should succeed");
        let submitted = dispatcher
            .trust
            .search_trades(account.id, Status::Submitted)
            .expect("submitted trade should load");
        assert_eq!(submitted.len(), 1);
        let submitted_id = submitted[0].id.to_string();

        let sync_matches = TradeCommandBuilder::new()
            .sync_trade()
            .build()
            .get_matches_from(["trade", "sync", "--trade-id", &submitted_id]);
        dispatcher
            .create_sync(sync_matches.subcommand_matches("sync").unwrap())
            .expect("non-interactive sync should succeed");
        let filled = dispatcher
            .trust
            .search_trades(account.id, Status::Filled)
            .expect("filled trade should load");
        assert_eq!(filled.len(), 1);

        let mut cancel_dispatcher = test_dispatcher();
        let (cancel_account, cancel_trade) = seed_new_trade(&mut cancel_dispatcher);
        let (funded_cancel, _, _, _) = cancel_dispatcher
            .trust
            .fund_trade(&cancel_trade)
            .expect("fund trade before cancel");
        let funded_cancel_id = funded_cancel.id.to_string();
        let cancel_matches = TradeCommandBuilder::new()
            .cancel_trade()
            .build()
            .get_matches_from(["trade", "cancel", "--trade-id", &funded_cancel_id]);
        cancel_dispatcher
            .create_cancel(cancel_matches.subcommand_matches("cancel").unwrap())
            .expect("non-interactive cancel should succeed");
        let canceled = cancel_dispatcher
            .trust
            .search_trades(cancel_account.id, Status::Canceled)
            .expect("canceled trade should load");
        assert_eq!(canceled.len(), 1);
    }

    #[test]
    fn test_trade_non_interactive_handlers_surface_core_and_broker_failures() {
        let trade_id_matches = |trade_id: &str| {
            Command::new("test")
                .arg(Arg::new("trade-id").long("trade-id"))
                .get_matches_from(["test", "--trade-id", trade_id])
        };

        let mut funding_dispatcher = test_dispatcher();
        let account = funding_dispatcher
            .trust
            .create_account(
                "no-cash-fund",
                "test",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account");
        let vehicle = funding_dispatcher
            .trust
            .create_trading_vehicle("NOCASH", None, &TradingVehicleCategory::Stock, "alpaca")
            .expect("vehicle");
        let trade = funding_dispatcher
            .trust
            .create_trade(
                model::DraftTrade {
                    account,
                    trading_vehicle: vehicle,
                    quantity: 1,
                    category: model::TradeCategory::Long,
                    currency: Currency::USD,
                    thesis: None,
                    sector: None,
                    asset_class: None,
                    context: None,
                },
                dec!(95),
                dec!(100),
                dec!(110),
            )
            .expect("trade can be drafted without cash");
        let error = funding_dispatcher
            .create_funding(&trade_id_matches(&trade.id.to_string()))
            .expect_err("funding without available cash should fail");
        assert!(error.to_string().contains("trade_fund_failed"));

        let trade_create_matches = trade_matches(&[
            "--account",
            "readable-account",
            "--symbol",
            "AAPL",
            "--category",
            "long",
            "--entry",
            "100",
            "--stop",
            "95",
            "--safety-order-type",
            "stop-limit",
            "--target",
            "110",
            "--quantity",
            "1",
        ]);
        let mut vehicle_lookup_dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::trading_vehicles_after_account(),
        );
        let error = vehicle_lookup_dispatcher
            .create_trade(&trade_create_matches)
            .expect_err("trading vehicle read failure should surface during trade creation");
        assert!(error.to_string().contains("trading_vehicle_lookup_failed"));

        let mut order_write_dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::order_write_after_reads(),
        );
        let error = order_write_dispatcher
            .create_trade(&trade_create_matches)
            .expect_err("order write failure should surface during trade creation");
        assert!(error.to_string().contains("trade_create_failed"));

        let mut submitted_cancel_broker = StubBroker::quote_trade_fixture();
        submitted_cancel_broker.submit_succeeds = true;
        let mut cancel_dispatcher = test_dispatcher_with_broker(Box::new(submitted_cancel_broker));
        let (_account, trade) = seed_new_trade(&mut cancel_dispatcher);
        let (funded, _, _, _) = cancel_dispatcher
            .trust
            .fund_trade(&trade)
            .expect("fund trade");
        let (submitted, _) = cancel_dispatcher
            .trust
            .submit_trade(&funded)
            .expect("submit trade");
        let error = cancel_dispatcher
            .create_cancel(&trade_id_matches(&submitted.id.to_string()))
            .expect_err("submitted cancel broker failure should surface");
        assert!(error.to_string().contains("trade_cancel_failed"));

        let mut sync_broker = StubBroker::quote_trade_fixture();
        sync_broker.submit_succeeds = true;
        sync_broker.sync_fills_entry = false;
        let mut sync_dispatcher = test_dispatcher_with_broker(Box::new(sync_broker));
        let (_account, trade) = seed_new_trade(&mut sync_dispatcher);
        let (funded, _, _, _) = sync_dispatcher
            .trust
            .fund_trade(&trade)
            .expect("fund trade");
        let (submitted, _) = sync_dispatcher
            .trust
            .submit_trade(&funded)
            .expect("submit trade");
        let error = sync_dispatcher
            .create_sync(&trade_id_matches(&submitted.id.to_string()))
            .expect_err("sync broker failure should surface");
        assert!(error.to_string().contains("trade_sync_failed"));
    }

    #[test]
    fn test_trade_non_interactive_sync_reports_empty_order_updates() {
        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_empty_sync_fixture()));
        let (_account, trade) = seed_new_trade(&mut dispatcher);
        let (funded, _, _, _) = dispatcher.trust.fund_trade(&trade).expect("fund trade");
        let (submitted, _) = dispatcher
            .trust
            .submit_trade(&funded)
            .expect("submit trade");
        let matches = Command::new("test")
            .arg(Arg::new("trade-id").long("trade-id"))
            .get_matches_from(["test", "--trade-id", &submitted.id.to_string()]);

        dispatcher
            .create_sync(&matches)
            .expect("empty broker order update sync should still succeed");
    }

    #[test]
    fn test_trade_watch_dispatch_handles_no_open_trades_for_search_and_latest() {
        let mut dispatcher = test_dispatcher();
        dispatcher
            .trust
            .create_account(
                "watch-empty",
                "watch",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("create watch account");

        crate::dialogs::scripted_reset();
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        let search_watch = TradeCommandBuilder::new()
            .watch_trade()
            .build()
            .get_matches_from(["trade", "watch"]);
        dispatcher.create_watch(search_watch.subcommand_matches("watch").unwrap());

        crate::dialogs::scripted_push_select(Ok(Some(0)));
        let latest_watch = TradeCommandBuilder::new()
            .watch_trade()
            .build()
            .get_matches_from(["trade", "watch", "--latest"]);
        dispatcher.create_watch(latest_watch.subcommand_matches("watch").unwrap());

        crate::dialogs::scripted_reset();
    }

    #[test]
    fn test_create_trade_non_interactive_validates_missing_account_and_quantity() {
        let mut dispatcher = test_dispatcher();

        let missing_account = trade_matches(&["--symbol", "AAPL"]);
        let account_error = dispatcher
            .create_trade(&missing_account)
            .expect_err("missing account should fail");
        assert!(account_error.to_string().contains("missing_argument"));

        let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);
        let invalid_qty = trade_matches(&[
            "--account",
            &account.name,
            "--symbol",
            "AAPL",
            "--category",
            "long",
            "--entry",
            "100",
            "--stop",
            "95",
            "--target",
            "110",
            "--quantity",
            "abc",
        ]);
        let qty_error = dispatcher
            .create_trade(&invalid_qty)
            .expect_err("invalid quantity should fail");
        assert!(qty_error.to_string().contains("invalid_quantity"));

        let zero_qty = trade_matches(&[
            "--account",
            &account.name,
            "--symbol",
            "AAPL",
            "--category",
            "long",
            "--entry",
            "100",
            "--stop",
            "95",
            "--target",
            "110",
            "--quantity",
            "0",
        ]);
        let zero_error = dispatcher
            .create_trade(&zero_qty)
            .expect_err("zero quantity should fail");
        assert!(zero_error
            .to_string()
            .contains("Quantity must be greater than 0"));
    }

    #[test]
    fn test_trade_events_add_persists_and_list_json_path_runs() {
        let mut dispatcher = test_dispatcher();
        let (_account, trade) = seed_new_trade(&mut dispatcher);
        let event_date = Utc::now()
            .date_naive()
            .checked_add_days(Days::new(14))
            .expect("event date");
        let trade_id = trade.id.to_string();
        let date = event_date.to_string();
        let add_matches = trade_event_add_matches(&[
            "--trade-id",
            &trade_id,
            "--type",
            "Earnings",
            "--date",
            &date,
            "--severity",
            "High",
            "--notes",
            "post-close earnings",
        ]);

        dispatcher
            .trade_events_add(&add_matches)
            .expect("event add should succeed");

        let events = dispatcher
            .trust
            .trade_events_for_trade(trade.id)
            .expect("events should be persisted");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, model::TradeEventType::Earnings);
        assert_eq!(events[0].severity, model::TradeEventSeverity::High);
        assert_eq!(events[0].notes.as_deref(), Some("post-close earnings"));

        let list_matches = trade_event_list_matches(&["--trade-id", &trade_id, "--format", "json"]);
        dispatcher
            .trade_events_list(&list_matches, ReportOutputFormat::Json)
            .expect("event list JSON should succeed");
    }

    #[test]
    fn test_trade_advisor_json_contract_and_verdict_logic_for_high_catalyst() {
        let mut dispatcher = test_dispatcher();
        let (_account, trade) = seed_new_trade(&mut dispatcher);
        let today = Utc::now().date_naive();
        let event_date = today.checked_add_days(Days::new(2)).expect("event date");
        let _event = dispatcher
            .trust
            .create_trade_event(
                trade.id,
                model::TradeEventType::Earnings,
                event_date,
                model::TradeEventSeverity::High,
                None,
            )
            .expect("seed event");
        let events = dispatcher
            .trust
            .trade_events_for_trade(trade.id)
            .expect("read events");

        let catalyst = ArgDispatcher::trade_advisor_catalyst_check(&events, today, 30, false, None);
        let correlation = TradeAdvisorCheck {
            status: "OK",
            payload: json!({
                "max_corr": "0.12",
                "corr_with": "MSFT",
                "cluster": null,
                "heat_adjusted_pct": "1.5",
                "status": "OK",
            }),
        };
        let regime = TradeAdvisorCheck {
            status: "OK",
            payload: json!({
                "vol": "Normal",
                "trend": "Trending",
                "breadth": null,
                "composite": "Normal",
                "breakouts_permitted": true,
                "status": "OK",
            }),
        };
        let verdict = ArgDispatcher::trade_advisor_verdict(&[&catalyst, &correlation, &regime]);
        let payload = json!({
            "trade_id": trade.id.to_string(),
            "verdict": verdict,
            "catalyst": catalyst.payload,
            "correlation": correlation.payload,
            "regime": regime.payload,
        });

        assert_eq!(payload["verdict"], "NO_TRADE");
        assert_eq!(payload["catalyst"]["status"], "BLOCKED");
        assert_eq!(payload["catalyst"]["events"][0]["type"], "Earnings");
        assert_eq!(payload["catalyst"]["events"][0]["severity"], "High");
        assert_eq!(payload["correlation"]["status"], "OK");
        assert_eq!(payload["regime"]["composite"], "Normal");
    }

    #[test]
    fn test_trade_autopsy_interactive_persists_mistake_for_closed_graded_trade() {
        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (_account, trade) = seed_closed_target_trade(&mut dispatcher);
        dispatcher
            .trust
            .grade_trade(
                trade.id,
                core::services::grading::GradingWeightsPermille::default(),
            )
            .expect("closed trade should grade");
        let trade_id = trade.id.to_string();
        let matches = trade_autopsy_matches(&["--trade-id", &trade_id]);

        crate::dialogs::scripted_reset();
        crate::dialogs::scripted_push_multi_select(Ok(vec![3, 6]));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_input(Ok("held past stop rule".to_string()));
        crate::dialogs::scripted_push_input(Ok("1.5".to_string()));
        crate::dialogs::scripted_push_input(Ok(
            "Exit when the invalidation level breaks.".to_string()
        ));

        dispatcher
            .trade_autopsy(&matches)
            .expect("trade autopsy should persist");

        let mistakes = dispatcher
            .trust
            .mistakes_for_trade(trade.id)
            .expect("mistakes should read");
        assert_eq!(mistakes.len(), 1);
        let mistake = &mistakes[0];
        assert_eq!(mistake.trade_id, trade.id);
        assert!(mistake
            .bias_tags
            .contains(&model::MungerTendency::PainAvoidingDenial));
        assert!(mistake
            .bias_tags
            .contains(&model::MungerTendency::DeprivalSuperreaction));
        assert!(mistake
            .bias_tags
            .contains(&model::MungerTendency::Lollapalooza));
        assert!(mistake.lollapalooza);
        assert_eq!(mistake.error_type, model::MistakeErrorType::Commission);
        assert_eq!(
            mistake.rule_violated.as_deref(),
            Some("held past stop rule")
        );
        assert_eq!(mistake.counterfactual_r, dec!(1.5));
        assert_eq!(mistake.lesson, "Exit when the invalidation level breaks.");
        crate::dialogs::scripted_reset();
    }

    #[test]
    fn test_trade_autopsy_rejects_open_and_ungraded_trades_before_dialogs() {
        let mut open_dispatcher = test_dispatcher();
        let (_account, open_trade) = seed_new_trade(&mut open_dispatcher);
        let open_trade_id = open_trade.id.to_string();
        let open_matches = trade_autopsy_matches(&["--trade-id", &open_trade_id]);
        let error = open_dispatcher
            .trade_autopsy(&open_matches)
            .expect_err("open trade should be rejected");
        assert_eq!(error.code, "trade_not_closed");

        let mut ungraded_dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (_account, closed_trade) = seed_closed_target_trade(&mut ungraded_dispatcher);
        let closed_trade_id = closed_trade.id.to_string();
        let closed_matches = trade_autopsy_matches(&["--trade-id", &closed_trade_id]);
        let error = ungraded_dispatcher
            .trade_autopsy(&closed_matches)
            .expect_err("ungraded trade should be rejected");
        assert_eq!(error.code, "trade_not_graded");
    }

    #[test]
    fn test_create_trade_non_interactive_validates_required_symbol_category_and_vehicle() {
        let mut dispatcher = test_dispatcher();
        let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

        let missing_symbol = trade_matches(&[
            "--account",
            &account.name,
            "--category",
            "long",
            "--quantity",
            "1",
        ]);
        let error = dispatcher
            .create_trade(&missing_symbol)
            .expect_err("missing symbol should fail");
        assert!(error.to_string().contains("missing_argument"));

        let missing_category = trade_matches(&[
            "--account",
            &account.name,
            "--symbol",
            "AAPL",
            "--quantity",
            "1",
        ]);
        let error = dispatcher
            .create_trade(&missing_category)
            .expect_err("missing category should fail");
        assert!(error.to_string().contains("missing_argument"));

        let missing_quantity = trade_matches(&[
            "--account",
            &account.name,
            "--symbol",
            "AAPL",
            "--category",
            "long",
        ]);
        let error = dispatcher
            .create_trade(&missing_quantity)
            .expect_err("missing quantity should fail");
        assert!(error.to_string().contains("missing_argument"));

        let unknown_vehicle = trade_matches(&[
            "--account",
            &account.name,
            "--symbol",
            "MSFT",
            "--category",
            "long",
            "--entry",
            "100",
            "--stop",
            "95",
            "--target",
            "110",
            "--quantity",
            "1",
        ]);
        let error = dispatcher
            .create_trade(&unknown_vehicle)
            .expect_err("unknown trading vehicle should fail");
        assert!(error.to_string().contains("trading_vehicle_not_found"));

        let invalid_entry = trade_matches(&[
            "--account",
            &account.name,
            "--symbol",
            "AAPL",
            "--category",
            "long",
            "--entry",
            "not-a-decimal",
            "--stop",
            "95",
            "--target",
            "110",
            "--quantity",
            "1",
        ]);
        let error = dispatcher
            .create_trade(&invalid_entry)
            .expect_err("invalid entry decimal should fail");
        assert!(error.to_string().contains("invalid_decimal"));
    }

    #[test]
    fn test_create_trade_non_interactive_validates_category_and_currency() {
        let mut dispatcher = test_dispatcher();
        let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

        let invalid_category = trade_matches(&[
            "--account",
            &account.name,
            "--symbol",
            "AAPL",
            "--category",
            "swing",
            "--entry",
            "100",
            "--stop",
            "95",
            "--target",
            "110",
            "--quantity",
            "1",
        ]);
        let category_error = dispatcher
            .create_trade(&invalid_category)
            .expect_err("invalid category should fail");
        assert!(category_error
            .to_string()
            .contains("invalid_trade_category"));

        let invalid_currency = trade_matches(&[
            "--account",
            &account.name,
            "--symbol",
            "AAPL",
            "--category",
            "long",
            "--entry",
            "100",
            "--stop",
            "95",
            "--target",
            "110",
            "--quantity",
            "1",
            "--currency",
            "jpy",
        ]);
        let currency_error = dispatcher
            .create_trade(&invalid_currency)
            .expect_err("invalid currency should fail");
        assert!(currency_error.to_string().contains("invalid_currency"));

        let invalid_safety_order_type = trade_matches(&[
            "--account",
            &account.name,
            "--symbol",
            "AAPL",
            "--category",
            "long",
            "--entry",
            "100",
            "--stop",
            "95",
            "--safety-order-type",
            "market",
            "--target",
            "110",
            "--quantity",
            "1",
        ]);
        let safety_order_error = dispatcher
            .create_trade(&invalid_safety_order_type)
            .expect_err("invalid safety order type should fail");
        assert!(safety_order_error
            .to_string()
            .contains("invalid_safety_order_type"));
    }

    #[test]
    fn test_create_trade_non_interactive_surfaces_fund_and_submit_failures() {
        let mut unfunded_dispatcher = test_dispatcher();
        let unfunded_account = unfunded_dispatcher
            .trust
            .create_account(
                "no-cash-for-auto-fund",
                "test",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account");
        unfunded_dispatcher
            .trust
            .create_trading_vehicle("MSFT", None, &TradingVehicleCategory::Stock, "alpaca")
            .expect("vehicle");
        let auto_fund_without_cash = trade_matches(&[
            "--account",
            &unfunded_account.name,
            "--symbol",
            "MSFT",
            "--category",
            "long",
            "--entry",
            "100",
            "--stop",
            "95",
            "--safety-order-type",
            "stop-limit",
            "--target",
            "110",
            "--quantity",
            "1",
            "--auto-fund",
        ]);
        let error = unfunded_dispatcher
            .create_trade(&auto_fund_without_cash)
            .expect_err("auto-fund without available cash should fail");
        assert!(error.to_string().contains("trade_fund_failed"));

        let mut submit_failure_dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::quote_trade_fixture()));
        let (submit_failure_account, _vehicle) =
            seed_account_and_vehicle(&mut submit_failure_dispatcher);
        let auto_submit_with_broker_error = trade_matches(&[
            "--account",
            &submit_failure_account.name,
            "--symbol",
            "AAPL",
            "--category",
            "long",
            "--entry",
            "100",
            "--stop",
            "95",
            "--target",
            "110",
            "--quantity",
            "1",
            "--auto-submit",
        ]);
        let error = submit_failure_dispatcher
            .create_trade(&auto_submit_with_broker_error)
            .expect_err("broker submit failure should surface");
        assert!(error.to_string().contains("trade_submit_failed"));
    }

    #[test]
    fn test_create_trade_non_interactive_success_can_auto_fund() {
        let mut dispatcher = test_dispatcher();
        let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

        let matches = trade_matches(&[
            "--account",
            &account.name,
            "--symbol",
            "aapl",
            "--category",
            "long",
            "--entry",
            "100",
            "--stop",
            "95",
            "--safety-order-type",
            "stop-limit",
            "--target",
            "110",
            "--quantity",
            "2",
            "--currency",
            "usd",
            "--thesis",
            "breakout",
            "--sector",
            "technology",
            "--asset-class",
            "equity",
            "--context",
            "daily setup",
            "--auto-fund",
        ]);

        dispatcher
            .create_trade(&matches)
            .expect("valid non-interactive trade should be created and funded");

        let funded = dispatcher
            .trust
            .search_trades(account.id, Status::Funded)
            .expect("funded trades should load");
        assert_eq!(funded.len(), 1);
        assert_eq!(funded[0].trading_vehicle.symbol, "AAPL");
        assert_eq!(funded[0].entry.quantity, 2);
        assert_eq!(funded[0].safety_stop.category, OrderCategory::StopLimit);
        assert_eq!(funded[0].thesis.as_deref(), Some("breakout"));
    }

    #[test]
    fn test_create_trade_non_interactive_auto_submit_uses_broker_success_path() {
        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

        let matches = trade_matches(&[
            "--account",
            &account.name,
            "--symbol",
            "aapl",
            "--category",
            "long",
            "--entry",
            "100",
            "--stop",
            "95",
            "--target",
            "110",
            "--quantity",
            "2",
            "--currency",
            "usd",
            "--auto-submit",
        ]);

        dispatcher
            .create_trade(&matches)
            .expect("auto-submit should fund and submit through the stub broker");

        let submitted = dispatcher
            .trust
            .search_trades(account.id, Status::Submitted)
            .expect("submitted trades should load");
        assert_eq!(submitted.len(), 1);
        assert_eq!(submitted[0].trading_vehicle.symbol, "AAPL");
        assert!(submitted[0]
            .entry
            .broker_order_id
            .as_deref()
            .expect("entry broker id should be persisted")
            .starts_with("entry-"));
    }

    #[test]
    fn test_trade_search_list_open_and_reconcile_non_interactive_success_paths() {
        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (account, trade) = seed_filled_trade(&mut dispatcher);
        let account_id = account.id.to_string();
        let trade_id = trade.id.to_string();

        let search_matches = |args: &[&str]| {
            let mut argv = vec!["trade", "search"];
            argv.extend_from_slice(args);
            TradeCommandBuilder::new()
                .search_trade()
                .build()
                .get_matches_from(argv)
        };
        let list_open_matches = |args: &[&str]| {
            let mut argv = vec!["trade", "list-open"];
            argv.extend_from_slice(args);
            TradeCommandBuilder::new()
                .list_open()
                .build()
                .get_matches_from(argv)
        };
        let reconcile_matches = |args: &[&str]| {
            let mut argv = vec!["trade", "reconcile"];
            argv.extend_from_slice(args);
            TradeCommandBuilder::new()
                .reconcile()
                .build()
                .get_matches_from(argv)
        };

        let filtered_text = search_matches(&[
            "--account",
            &account_id,
            "--status",
            "filled",
            "--symbol",
            "aapl",
            "--from",
            "2000-01-01",
            "--to",
            "2999-12-31",
        ]);
        dispatcher
            .search_trade(filtered_text.subcommand_matches("search").unwrap())
            .expect("filtered text search should succeed");

        let aggregate_json = search_matches(&["--symbol", "AAPL", "--format", "json"]);
        dispatcher
            .search_trade(aggregate_json.subcommand_matches("search").unwrap())
            .expect("aggregate json search should succeed");

        let status_only = search_matches(&["--status", "filled"]);
        dispatcher
            .search_trade(status_only.subcommand_matches("search").unwrap())
            .expect("status-only search should scan all accounts");

        let list_text = list_open_matches(&["--account", &account_id]);
        dispatcher
            .list_open_trades(list_text.subcommand_matches("list-open").unwrap())
            .expect("account-scoped list-open should succeed");

        let list_json = list_open_matches(&["--format", "json"]);
        dispatcher
            .list_open_trades(list_json.subcommand_matches("list-open").unwrap())
            .expect("aggregate list-open json should succeed");

        let reconcile_all_json = reconcile_matches(&["--format", "json"]);
        dispatcher
            .reconcile_trades(reconcile_all_json.subcommand_matches("reconcile").unwrap())
            .expect("aggregate reconcile should scan open trades");

        let reconcile_text = reconcile_matches(&["--account", &account_id]);
        dispatcher
            .reconcile_trades(reconcile_text.subcommand_matches("reconcile").unwrap())
            .expect("account-scoped reconcile should collect broker failures");

        let reconcile_json = reconcile_matches(&["--trade-id", &trade_id, "--format", "json"]);
        dispatcher
            .reconcile_trades(reconcile_json.subcommand_matches("reconcile").unwrap())
            .expect("trade-id reconcile should collect broker failure as json");

        let mut many_dispatcher = test_dispatcher();
        let (many_account, many_vehicle) = seed_account_and_vehicle(&mut many_dispatcher);
        for index in 0..51 {
            many_dispatcher
                .trust
                .create_trade(
                    model::DraftTrade {
                        account: many_account.clone(),
                        trading_vehicle: many_vehicle.clone(),
                        quantity: 1,
                        category: model::TradeCategory::Long,
                        currency: Currency::USD,
                        thesis: Some(format!("setup-{index}")),
                        sector: Some("technology".to_string()),
                        asset_class: Some("equity".to_string()),
                        context: None,
                    },
                    dec!(95),
                    dec!(100),
                    dec!(110),
                )
                .expect("bulk trade should be created");
        }
        let many_search = search_matches(&["--account", &many_account.id.to_string()]);
        many_dispatcher
            .search_trade(many_search.subcommand_matches("search").unwrap())
            .expect("large text search should print truncation notice");

        let mut broker = StubBroker::quote_trade_fixture();
        broker.submit_succeeds = true;
        let mut failing_sync_dispatcher = test_dispatcher_with_broker(Box::new(broker));
        let (sync_account, sync_trade) = seed_new_trade(&mut failing_sync_dispatcher);
        let (funded, _, _, _) = failing_sync_dispatcher
            .trust
            .fund_trade(&sync_trade)
            .expect("fund trade");
        failing_sync_dispatcher
            .trust
            .submit_trade(&funded)
            .expect("submit trade");
        let failing_reconcile = reconcile_matches(&["--account", &sync_account.id.to_string()]);
        failing_sync_dispatcher
            .reconcile_trades(failing_reconcile.subcommand_matches("reconcile").unwrap())
            .expect("reconcile should print broker sync failures in text mode");
        let submitted_trade = failing_sync_dispatcher
            .trust
            .search_trades(sync_account.id, Status::Submitted)
            .expect("submitted trade should load")
            .into_iter()
            .find(|candidate| candidate.id == sync_trade.id)
            .expect("submitted trade should be persisted");
        let failing_trade_id_reconcile =
            reconcile_matches(&["--trade-id", &submitted_trade.id.to_string()]);
        failing_sync_dispatcher
            .reconcile_trades(
                failing_trade_id_reconcile
                    .subcommand_matches("reconcile")
                    .unwrap(),
            )
            .expect("trade-id reconcile should print broker sync failure");

        let missing_account_reconcile =
            reconcile_matches(&["--trade-id", &Uuid::nil().to_string(), "--format", "json"]);
        let mut missing_account_dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::submitted_trade_missing_account(),
        );
        missing_account_dispatcher
            .reconcile_trades(
                missing_account_reconcile
                    .subcommand_matches("reconcile")
                    .unwrap(),
            )
            .expect("reconcile should collect account lookup failures");
    }

    #[test]
    fn test_trade_search_validates_date_filters_before_querying() {
        let mut dispatcher = test_dispatcher();
        let search_matches = |args: &[&str]| {
            let mut argv = vec!["trade", "search"];
            argv.extend_from_slice(args);
            TradeCommandBuilder::new()
                .search_trade()
                .build()
                .get_matches_from(argv)
        };

        let invalid_from = search_matches(&["--symbol", "AAPL", "--from", "not-a-date"]);
        let error = dispatcher
            .search_trade(invalid_from.subcommand_matches("search").unwrap())
            .expect_err("invalid --from should fail");
        assert!(error.to_string().contains("invalid_date"));

        let invalid_to = search_matches(&["--symbol", "AAPL", "--to", "not-a-date"]);
        let error = dispatcher
            .search_trade(invalid_to.subcommand_matches("search").unwrap())
            .expect_err("invalid --to should fail");
        assert!(error.to_string().contains("invalid_date"));

        let reversed = search_matches(&[
            "--symbol",
            "AAPL",
            "--from",
            "2026-02-02",
            "--to",
            "2026-02-01",
        ]);
        let error = dispatcher
            .search_trade(reversed.subcommand_matches("search").unwrap())
            .expect_err("reversed date window should fail");
        assert!(error.to_string().contains("invalid_date_range"));
    }

    #[test]
    fn test_trade_non_interactive_lifecycle_handlers_persist_expected_statuses() {
        let trade_id_matches = |trade_id: &str| {
            Command::new("test")
                .arg(Arg::new("trade-id").long("trade-id"))
                .get_matches_from(["test", "--trade-id", trade_id])
        };

        let mut cancel_dispatcher = test_dispatcher();
        let (cancel_account, cancel_trade) = seed_new_trade(&mut cancel_dispatcher);
        cancel_dispatcher
            .create_funding(&trade_id_matches(&cancel_trade.id.to_string()))
            .expect("funding handler should fund a new trade");
        let funded_for_cancel = cancel_dispatcher
            .trust
            .search_trades(cancel_account.id, Status::Funded)
            .expect("funded trades should load")
            .into_iter()
            .find(|candidate| candidate.id == cancel_trade.id)
            .expect("funded trade should exist");
        cancel_dispatcher
            .create_cancel(&trade_id_matches(&funded_for_cancel.id.to_string()))
            .expect("cancel handler should cancel funded trade");
        let canceled = cancel_dispatcher
            .trust
            .search_trades(cancel_account.id, Status::Canceled)
            .expect("canceled trades should load");
        assert!(canceled
            .iter()
            .any(|candidate| candidate.id == funded_for_cancel.id));

        let mut sync_dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (sync_account, sync_trade) = seed_new_trade(&mut sync_dispatcher);
        sync_dispatcher
            .create_funding(&trade_id_matches(&sync_trade.id.to_string()))
            .expect("funding handler should fund before submit");
        let funded_for_submit = sync_dispatcher
            .trust
            .search_trades(sync_account.id, Status::Funded)
            .expect("funded trades should load")
            .into_iter()
            .find(|candidate| candidate.id == sync_trade.id)
            .expect("funded trade should exist");
        sync_dispatcher
            .create_submit(&trade_id_matches(&funded_for_submit.id.to_string()))
            .expect("submit handler should submit funded trade");
        let submitted = sync_dispatcher
            .trust
            .search_trades(sync_account.id, Status::Submitted)
            .expect("submitted trades should load")
            .into_iter()
            .find(|candidate| candidate.id == funded_for_submit.id)
            .expect("submitted trade should exist");
        sync_dispatcher
            .create_sync(&trade_id_matches(&submitted.id.to_string()))
            .expect("sync handler should apply filled broker payload");
        let filled = sync_dispatcher
            .trust
            .search_trades(sync_account.id, Status::Filled)
            .expect("filled trades should load");
        assert!(filled.iter().any(|candidate| candidate.id == submitted.id));
    }

    #[test]
    fn test_transaction_non_interactive_arg_detection_helper() {
        let matches = TransactionCommandBuilder::new()
            .deposit()
            .build()
            .get_matches_from(["transaction", "deposit", "--account", "main"]);
        let sub_matches = matches
            .subcommand_matches("deposit")
            .expect("deposit subcommand");
        assert!(ArgDispatcher::has_non_interactive_transaction_args(
            sub_matches
        ));

        let empty = TransactionCommandBuilder::new()
            .deposit()
            .build()
            .get_matches_from(["transaction", "deposit"]);
        let empty_sub = empty
            .subcommand_matches("deposit")
            .expect("deposit subcommand");
        assert!(!ArgDispatcher::has_non_interactive_transaction_args(
            empty_sub
        ));
    }

    #[test]
    fn test_dispatch_transactions_non_interactive_success_and_validation_errors() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();

        let account = dispatcher
            .trust
            .create_account("tx-account", "tx", Environment::Paper, dec!(20), dec!(10))
            .expect("create account");

        let deposit = TransactionCommandBuilder::new()
            .deposit()
            .build()
            .get_matches_from([
                "transaction",
                "deposit",
                "--account",
                &account.name,
                "--currency",
                "usd",
                "--amount",
                "100.50",
                "--confirm-protected",
                "secret",
            ]);
        assert!(
            dispatcher.dispatch_transactions(&deposit).is_ok(),
            "non-interactive deposit should succeed"
        );

        let after_deposit = dispatcher
            .trust
            .search_balance(account.id, &Currency::USD)
            .expect("balance after deposit");
        assert_eq!(after_deposit.total_balance, dec!(100.50));
        assert_eq!(after_deposit.total_available, dec!(100.50));

        let withdraw = TransactionCommandBuilder::new()
            .withdraw()
            .build()
            .get_matches_from([
                "transaction",
                "withdraw",
                "--account",
                &account.name,
                "--currency",
                "USD",
                "--amount",
                "40.25",
                "--confirm-protected",
                "secret",
            ]);
        assert!(
            dispatcher.dispatch_transactions(&withdraw).is_ok(),
            "non-interactive withdrawal should succeed"
        );

        let after_withdraw = dispatcher
            .trust
            .search_balance(account.id, &Currency::USD)
            .expect("balance after withdrawal");
        assert_eq!(after_withdraw.total_balance, dec!(60.25));
        assert_eq!(after_withdraw.total_available, dec!(60.25));

        let invalid_currency = TransactionCommandBuilder::new()
            .deposit()
            .build()
            .get_matches_from([
                "transaction",
                "deposit",
                "--account",
                &account.name,
                "--currency",
                "JPY",
                "--amount",
                "1",
                "--confirm-protected",
                "secret",
            ]);
        let currency_error = dispatcher
            .dispatch_transactions(&invalid_currency)
            .expect_err("unsupported currency should fail");
        assert!(currency_error.to_string().contains("invalid_currency"));

        let invalid_amount = TransactionCommandBuilder::new()
            .deposit()
            .build()
            .get_matches_from([
                "transaction",
                "deposit",
                "--account",
                &account.name,
                "--currency",
                "USD",
                "--amount",
                "abc",
                "--confirm-protected",
                "secret",
            ]);
        let amount_error = dispatcher
            .dispatch_transactions(&invalid_amount)
            .expect_err("invalid decimal should fail");
        assert!(amount_error.to_string().contains("invalid_decimal"));

        let missing_account = TransactionCommandBuilder::new()
            .deposit()
            .build()
            .get_matches_from([
                "transaction",
                "deposit",
                "--currency",
                "USD",
                "--amount",
                "1",
                "--confirm-protected",
                "secret",
            ]);
        let missing_account_error = dispatcher
            .dispatch_transactions(&missing_account)
            .expect_err("missing account should fail");
        assert!(missing_account_error
            .to_string()
            .contains("missing_argument"));

        let missing_currency = TransactionCommandBuilder::new()
            .deposit()
            .build()
            .get_matches_from([
                "transaction",
                "deposit",
                "--account",
                &account.name,
                "--amount",
                "1",
                "--confirm-protected",
                "secret",
            ]);
        let missing_currency_error = dispatcher
            .dispatch_transactions(&missing_currency)
            .expect_err("missing currency should fail");
        assert!(missing_currency_error
            .to_string()
            .contains("missing_argument"));

        let missing_amount = TransactionCommandBuilder::new()
            .deposit()
            .build()
            .get_matches_from([
                "transaction",
                "deposit",
                "--account",
                &account.name,
                "--currency",
                "USD",
                "--confirm-protected",
                "secret",
            ]);
        let missing_amount_error = dispatcher
            .dispatch_transactions(&missing_amount)
            .expect_err("missing amount should fail");
        assert!(missing_amount_error
            .to_string()
            .contains("missing_argument"));

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_dispatch_transactions_interactive_deposit_and_withdraw_use_scripted_dialog_io() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        crate::dialogs::scripted_reset();
        let mut dispatcher = test_dispatcher();

        let account = dispatcher
            .trust
            .create_account(
                "interactive-tx",
                "tx",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("create account");

        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_input(Ok("125".to_string()));
        let deposit = TransactionCommandBuilder::new()
            .deposit()
            .build()
            .get_matches_from(["transaction", "deposit", "--confirm-protected", "secret"]);
        dispatcher
            .dispatch_transactions(&deposit)
            .expect("scripted interactive deposit should dispatch");

        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_input(Ok("25".to_string()));
        let withdraw = TransactionCommandBuilder::new()
            .withdraw()
            .build()
            .get_matches_from(["transaction", "withdraw", "--confirm-protected", "secret"]);
        dispatcher
            .dispatch_transactions(&withdraw)
            .expect("scripted interactive withdrawal should dispatch");

        let balance = dispatcher
            .trust
            .search_balance(account.id, &Currency::USD)
            .expect("balance after scripted transactions");
        assert_eq!(balance.total_balance, dec!(100));
        assert_eq!(balance.total_available, dec!(100));

        crate::dialogs::scripted_reset();
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_accounts_transfer_moves_balance_between_accounts() {
        let mut dispatcher = test_dispatcher();
        let parent = dispatcher
            .trust
            .create_account("parent", "source", Environment::Paper, dec!(10), dec!(5))
            .expect("create source account");
        let child = dispatcher
            .trust
            .create_account_with_hierarchy(
                "child-earnings",
                "dest",
                Environment::Paper,
                dec!(10),
                dec!(5),
                AccountType::Earnings,
                Some(parent.id),
            )
            .expect("create destination account");
        dispatcher
            .trust
            .create_transaction(
                &parent,
                &model::TransactionCategory::Deposit,
                dec!(200),
                &Currency::USD,
            )
            .expect("seed source balance");

        let transfer = AccountCommandBuilder::new()
            .transfer_account()
            .build()
            .get_matches_from([
                "accounts",
                "transfer",
                "--from",
                &parent.id.to_string(),
                "--to",
                &child.id.to_string(),
                "--amount",
                "50",
                "--reason",
                "rebalance",
            ]);
        assert!(
            dispatcher.dispatch_accounts(&transfer).is_ok(),
            "transfer should succeed"
        );

        let from_balance = dispatcher
            .trust
            .search_balance(parent.id, &Currency::USD)
            .expect("source balance");
        assert!(
            from_balance.total_balance >= Decimal::ZERO,
            "source balance should remain non-negative"
        );

        let child_balances = dispatcher
            .trust
            .search_all_balances(child.id)
            .expect("destination balances query should succeed");
        assert!(
            child_balances
                .iter()
                .all(|balance| balance.total_balance >= Decimal::ZERO),
            "child balances should remain non-negative after transfer"
        );
    }

    #[test]
    fn test_accounts_dispatch_covers_search_hierarchy_and_detailed_balance_paths() {
        let mut dispatcher = test_dispatcher();

        let search = AccountCommandBuilder::new()
            .read_account()
            .build()
            .get_matches_from(["accounts", "search"]);
        assert!(
            dispatcher.dispatch_accounts(&search).is_ok(),
            "empty account search should report through dialog display without failing dispatch"
        );

        let parent = dispatcher
            .trust
            .create_account("hier-parent", "root", Environment::Paper, dec!(10), dec!(5))
            .expect("create parent account");
        let child = dispatcher
            .trust
            .create_account_with_hierarchy(
                "hier-child",
                "child",
                Environment::Paper,
                dec!(10),
                dec!(5),
                AccountType::Earnings,
                Some(parent.id),
            )
            .expect("create child account");
        dispatcher
            .trust
            .create_account("aaa-root", "root", Environment::Paper, dec!(10), dec!(5))
            .expect("create second root account");
        dispatcher
            .trust
            .create_account_with_hierarchy(
                "aaa-child",
                "child",
                Environment::Paper,
                dec!(10),
                dec!(5),
                AccountType::TaxReserve,
                Some(parent.id),
            )
            .expect("create second child account");
        dispatcher
            .trust
            .create_transaction(
                &parent,
                &model::TransactionCategory::Deposit,
                dec!(100),
                &Currency::USD,
            )
            .expect("seed parent balance");
        dispatcher
            .trust
            .create_transaction(
                &child,
                &model::TransactionCategory::Deposit,
                dec!(25),
                &Currency::USD,
            )
            .expect("seed child balance");

        let list_hierarchy = AccountCommandBuilder::new()
            .list_accounts()
            .build()
            .get_matches_from(["accounts", "list", "--hierarchy"]);
        assert!(dispatcher.dispatch_accounts(&list_hierarchy).is_ok());

        let balance_detailed = AccountCommandBuilder::new()
            .balance_accounts()
            .build()
            .get_matches_from(["accounts", "balance", "--detailed"]);
        assert!(dispatcher.dispatch_accounts(&balance_detailed).is_ok());
    }

    #[test]
    fn test_account_create_dispatch_interactive_path_uses_scripted_dialog_io() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        crate::dialogs::scripted_reset();
        crate::dialogs::scripted_push_input(Ok("interactive-account".to_string()));
        crate::dialogs::scripted_push_input(Ok("created from scripted dialog".to_string()));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_input(Ok("0.2".to_string()));
        crate::dialogs::scripted_push_input(Ok("0.1".to_string()));
        let mut dispatcher = test_dispatcher();

        let create_interactive = AccountCommandBuilder::new()
            .create_account()
            .build()
            .get_matches_from(["accounts", "create", "--confirm-protected", "secret"]);
        assert!(
            dispatcher.dispatch_accounts(&create_interactive).is_ok(),
            "scripted interactive account creation should dispatch successfully"
        );

        let account = dispatcher
            .trust
            .search_account("interactive-account")
            .expect("scripted account should be created");
        assert_eq!(account.environment, Environment::Paper);
        assert_eq!(account.taxes_percentage, dec!(0.2));
        assert_eq!(account.earnings_percentage, dec!(0.1));

        crate::dialogs::scripted_reset();
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_rule_remove_dispatch_validates_protected_keyword_before_dialogs() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();

        let remove_rule = RuleCommandBuilder::new()
            .remove_rule()
            .build()
            .arg(Arg::new("format").long("format"))
            .get_matches_from(["rule", "remove", "--confirm-protected", "wrong"]);
        let error = dispatcher
            .dispatch_rules(&remove_rule)
            .expect_err("wrong protected keyword should fail before rule removal dialog");

        assert!(error.to_string().contains("protected_keyword_invalid"));
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_rule_dispatch_interactive_create_and_remove_use_scripted_dialog_io() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        crate::dialogs::scripted_reset();
        let mut dispatcher = test_dispatcher();
        let account = dispatcher
            .trust
            .create_account(
                "interactive-rule",
                "rules",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("create account");

        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_input(Ok("2.5".to_string()));
        crate::dialogs::scripted_push_input(Ok("scripted risk rule".to_string()));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        let create_rule = RuleCommandBuilder::new()
            .create_rule()
            .build()
            .arg(Arg::new("format").long("format"))
            .get_matches_from(["rule", "create", "--confirm-protected", "secret"]);
        dispatcher
            .dispatch_rules(&create_rule)
            .expect("scripted rule creation should dispatch");

        let rules = dispatcher
            .trust
            .search_all_rules(account.id)
            .expect("created rule should be readable");
        assert_eq!(rules.len(), 1);
        assert!(rules[0].active);

        crate::dialogs::scripted_push_select(Ok(Some(0)));
        crate::dialogs::scripted_push_select(Ok(Some(0)));
        let remove_rule = RuleCommandBuilder::new()
            .remove_rule()
            .build()
            .arg(Arg::new("format").long("format"))
            .get_matches_from(["rule", "remove", "--confirm-protected", "secret"]);
        dispatcher
            .dispatch_rules(&remove_rule)
            .expect("scripted rule removal should dispatch");

        let active_rules = dispatcher
            .trust
            .search_all_rules(account.id)
            .expect("rules should remain readable");
        assert!(active_rules.is_empty());

        crate::dialogs::scripted_reset();
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_distribution_dispatch_supports_legacy_rules_route() {
        let mut dispatcher = test_dispatcher();
        let legacy_rules = DistributionCommandBuilder::new()
            .show_rules()
            .build()
            .get_matches_from(["distribution", "rules", "--account-id", "not-a-uuid"]);

        let error = dispatcher
            .dispatch_distribution(&legacy_rules)
            .expect_err("legacy rules route should validate account id");

        assert!(error.to_string().contains("invalid_uuid"));
    }

    #[test]
    fn test_distribution_dispatch_paths_cover_parse_and_service_edges() {
        let mut dispatcher = test_dispatcher();
        let account = dispatcher
            .trust
            .create_account(
                "dist",
                "distribution",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("create distribution account");

        let configure = DistributionCommandBuilder::new()
            .configure_distribution()
            .build()
            .get_matches_from([
                "distribution",
                "configure",
                "--account-id",
                &account.id.to_string(),
                "--earnings",
                "40",
                "--tax",
                "30",
                "--reinvestment",
                "30",
                "--threshold",
                "100",
                "--password",
                "strong-password",
            ]);
        let _ = dispatcher.dispatch_distribution(&configure);

        let execute_invalid_amount = DistributionCommandBuilder::new()
            .execute_distribution()
            .build()
            .get_matches_from([
                "distribution",
                "execute",
                "--account-id",
                &account.id.to_string(),
                "--amount",
                "abc",
            ]);
        let execute_error = dispatcher
            .dispatch_distribution(&execute_invalid_amount)
            .expect_err("invalid execute amount should fail");
        assert!(execute_error.to_string().contains("invalid_decimal"));

        let history = DistributionCommandBuilder::new()
            .history()
            .build()
            .get_matches_from([
                "distribution",
                "history",
                "--account-id",
                &account.id.to_string(),
                "--limit",
                "5",
            ]);
        assert!(
            dispatcher.dispatch_distribution(&history).is_ok(),
            "history should succeed for empty history"
        );

        let rules = DistributionCommandBuilder::new()
            .show_rules()
            .build()
            .get_matches_from([
                "distribution",
                "rules",
                "show",
                "--account-id",
                &account.id.to_string(),
            ]);
        let _ = dispatcher.dispatch_distribution(&rules);
    }

    #[test]
    fn test_distribution_success_paths_execute_history_and_rules_output() {
        let mut dispatcher = test_dispatcher();
        let source = dispatcher
            .trust
            .create_account(
                "dist-source",
                "source",
                Environment::Paper,
                dec!(10),
                dec!(5),
            )
            .expect("create source account");
        dispatcher
            .trust
            .create_account_with_hierarchy(
                "dist-earnings",
                "earnings",
                Environment::Paper,
                dec!(10),
                dec!(5),
                AccountType::Earnings,
                Some(source.id),
            )
            .expect("create earnings account");
        dispatcher
            .trust
            .create_account_with_hierarchy(
                "dist-tax",
                "tax",
                Environment::Paper,
                dec!(10),
                dec!(5),
                AccountType::TaxReserve,
                Some(source.id),
            )
            .expect("create tax account");
        dispatcher
            .trust
            .create_account_with_hierarchy(
                "dist-reinvestment",
                "reinvestment",
                Environment::Paper,
                dec!(10),
                dec!(5),
                AccountType::Reinvestment,
                Some(source.id),
            )
            .expect("create reinvestment account");
        dispatcher
            .trust
            .create_transaction(
                &source,
                &TransactionCategory::Deposit,
                dec!(1000),
                &Currency::USD,
            )
            .expect("seed distributable balance");

        let configure = distribution_config_matches(&[
            "--account-id",
            &source.id.to_string(),
            "--earnings",
            "40",
            "--tax",
            "30",
            "--reinvestment",
            "30",
            "--threshold",
            "0",
            "--password",
            "strong-password",
        ]);
        dispatcher
            .configure_distribution(&configure)
            .expect("distribution configuration should succeed");

        for amount in ["100", "50"] {
            let execute = distribution_execute_matches(&[
                "--account-id",
                &source.id.to_string(),
                "--amount",
                amount,
            ]);
            dispatcher
                .execute_distribution(&execute)
                .expect("distribution execution should succeed");
        }

        let history =
            distribution_history_matches(&["--account-id", &source.id.to_string(), "--limit", "1"]);
        dispatcher
            .distribution_history(&history)
            .expect("distribution history should render the truncated latest entry");

        let rules = distribution_rules_matches(&["--account-id", &source.id.to_string()]);
        dispatcher
            .distribution_rules(&rules)
            .expect("distribution rules should render configured values");
    }

    #[test]
    fn test_dispatchers_require_nested_subcommands() {
        let mut dispatcher = test_dispatcher();
        use std::panic::{catch_unwind, AssertUnwindSafe};

        let mut check_panics_without_subcommand =
            |f: fn(&mut ArgDispatcher, &clap::ArgMatches) -> Result<(), CliError>,
             name: &'static str| {
                let matches = Command::new(name).get_matches_from([name]);
                let result = catch_unwind(AssertUnwindSafe(|| f(&mut dispatcher, &matches)));
                assert!(
                    result.is_err(),
                    "{name} dispatcher should panic when subcommand is absent"
                );
            };

        check_panics_without_subcommand(ArgDispatcher::dispatch_db, "db");
        check_panics_without_subcommand(ArgDispatcher::dispatch_keys, "keys");
        check_panics_without_subcommand(ArgDispatcher::dispatch_accounts, "accounts");
        check_panics_without_subcommand(ArgDispatcher::dispatch_transactions, "transaction");
        check_panics_without_subcommand(ArgDispatcher::dispatch_rules, "rule");
        check_panics_without_subcommand(ArgDispatcher::dispatch_trading_vehicle, "trading-vehicle");
        check_panics_without_subcommand(ArgDispatcher::dispatch_trade, "trade");
        check_panics_without_subcommand(ArgDispatcher::dispatch_distribution, "distribution");
        check_panics_without_subcommand(ArgDispatcher::dispatch_report, "report");
        check_panics_without_subcommand(ArgDispatcher::dispatch_market_data, "market-data");
        check_panics_without_subcommand(ArgDispatcher::dispatch_grade, "grade");
        check_panics_without_subcommand(ArgDispatcher::dispatch_level, "level");
        check_panics_without_subcommand(ArgDispatcher::dispatch_onboarding, "onboarding");
        check_panics_without_subcommand(ArgDispatcher::dispatch_metrics, "metrics");
        check_panics_without_subcommand(ArgDispatcher::dispatch_advisor, "advisor");
        check_panics_without_subcommand(ArgDispatcher::dispatch_session, "session");
    }

    #[test]
    fn test_public_dispatch_routes_top_level_subcommands() {
        let _guard = env_guard();
        std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");

        let dispatch = |argv: &[&str]| {
            let matches = root_command().get_matches_from(argv);
            test_dispatcher().dispatch(matches)
        };

        assert!(dispatch(&[
            "trust",
            "db",
            "import",
            "--input",
            "/tmp/missing-trust.json"
        ])
        .is_err());
        assert!(dispatch(&["trust", "keys", "protected-show"]).is_ok());
        assert!(dispatch(&["trust", "accounts", "list"]).is_ok());
        assert!(dispatch(&["trust", "transaction", "deposit"]).is_err());
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = dispatch(&["trust", "rule", "create"]);
        }))
        .is_err());
        assert!(dispatch(&[
            "trust",
            "trading-vehicle",
            "create",
            "--from-alpaca",
            "--symbol",
            "AAPL",
        ])
        .is_err());
        assert!(dispatch(&["trust", "trade", "fund", "--trade-id", "not-a-uuid"]).is_err());
        assert!(dispatch(&[
            "trust",
            "distribution",
            "history",
            "--account-id",
            "not-a-uuid"
        ])
        .is_err());
        assert!(dispatch(&[
            "trust",
            "report",
            "--format",
            "json",
            "performance",
            "--account",
            "bad",
        ])
        .is_err());
        assert!(dispatch(&[
            "trust",
            "market-data",
            "quote",
            "--account",
            "bad",
            "--symbol",
            "AAPL",
        ])
        .is_err());
        assert!(dispatch(&["trust", "grade", "show", "not-a-uuid"]).is_err());
        assert!(dispatch(&["trust", "level", "status", "--account", "bad"]).is_err());
        assert!(dispatch(&["trust", "onboarding", "status", "--format", "json"]).is_ok());
        assert!(dispatch(&["trust", "policy", "--format", "json"]).is_ok());
        assert!(dispatch(&[
            "trust",
            "metrics",
            "bond",
            "--face-value",
            "1000",
            "--market-price",
            "950",
            "--coupon-rate",
            "4",
            "--quantity",
            "3",
            "--years-to-maturity",
            "5",
        ])
        .is_ok());
        assert!(dispatch(&["trust", "advisor", "status", "--account", "bad"]).is_err());
        assert!(dispatch(&["trust", "session", "list", "--account", "bad"]).is_err());
        assert!(dispatch(&["trust", "external-plugin", "arg1", "arg2"]).is_ok());

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
        std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
    }

    #[test]
    fn test_dispatchers_cover_non_interactive_routes_with_safe_error_paths() {
        let _guard = env_guard();
        std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");

        // db
        let mut dispatcher = test_dispatcher();
        let db = DbCommandBuilder::new().import().build().get_matches_from([
            "db",
            "import",
            "--input",
            "/tmp/does-not-exist.json",
        ]);
        assert!(dispatcher.dispatch_db(&db).is_err());

        // keys (protected-set + protected-delete)
        let mut dispatcher = test_dispatcher();
        let keys_set = KeysCommandBuilder::new()
            .protected_set()
            .build()
            .get_matches_from([
                "keys",
                "protected-set",
                "--value",
                "next-secret",
                "--confirm-protected",
                "wrong",
            ]);
        let _ = dispatcher.dispatch_keys(&keys_set);
        let keys_delete = KeysCommandBuilder::new()
            .protected_delete()
            .build()
            .get_matches_from(["keys", "protected-delete", "--confirm-protected", "wrong"]);
        assert!(dispatcher.dispatch_keys(&keys_delete).is_err());

        // account create (non-interactive args), list and balance arms
        let mut dispatcher = test_dispatcher();
        let create_account = AccountCommandBuilder::new()
            .create_account()
            .build()
            .get_matches_from([
                "accounts",
                "create",
                "--confirm-protected",
                "secret",
                "--name",
                "acc",
                "--description",
                "desc",
                "--environment",
                "invalid",
                "--taxes",
                "20",
                "--earnings",
                "10",
            ]);
        assert!(dispatcher.dispatch_accounts(&create_account).is_err());
        let list = AccountCommandBuilder::new()
            .list_accounts()
            .build()
            .get_matches_from(["accounts", "list"]);
        assert!(dispatcher.dispatch_accounts(&list).is_ok());
        let balance = AccountCommandBuilder::new()
            .balance_accounts()
            .build()
            .get_matches_from(["accounts", "balance"]);
        assert!(dispatcher.dispatch_accounts(&balance).is_ok());

        // transaction/rule guarded before interactive dialogs
        let mut dispatcher = test_dispatcher();
        let deposit = TransactionCommandBuilder::new()
            .deposit()
            .build()
            .get_matches_from(["transaction", "deposit"]);
        assert!(dispatcher.dispatch_transactions(&deposit).is_err());
        let create_rule = RuleCommandBuilder::new()
            .create_rule()
            .build()
            .get_matches_from(["rule", "create"]);
        let rule_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = dispatcher.dispatch_rules(&create_rule);
        }));
        assert!(rule_result.is_err());

        // trading vehicle create (from-alpaca path)
        let mut dispatcher = test_dispatcher();
        let vehicle = TradingVehicleCommandBuilder::new()
            .create_trading_vehicle()
            .build()
            .get_matches_from([
                "trading-vehicle",
                "create",
                "--from-alpaca",
                "--symbol",
                "AAPL",
            ]);
        assert!(dispatcher.dispatch_trading_vehicle(&vehicle).is_err());

        // trade non-interactive branches with invalid IDs
        let mut dispatcher = test_dispatcher();
        let fund = TradeCommandBuilder::new()
            .fund_trade()
            .build()
            .get_matches_from(["trade", "fund", "--trade-id", "not-a-uuid"]);
        assert!(dispatcher.dispatch_trade(&fund).is_err());
        let submit = TradeCommandBuilder::new()
            .submit_trade()
            .build()
            .get_matches_from(["trade", "submit", "--trade-id", "not-a-uuid"]);
        assert!(dispatcher.dispatch_trade(&submit).is_err());
        let cancel = TradeCommandBuilder::new()
            .cancel_trade()
            .build()
            .get_matches_from(["trade", "cancel", "--trade-id", "not-a-uuid"]);
        assert!(dispatcher.dispatch_trade(&cancel).is_err());
        let sync = TradeCommandBuilder::new()
            .sync_trade()
            .build()
            .get_matches_from(["trade", "sync", "--trade-id", "not-a-uuid"]);
        assert!(dispatcher.dispatch_trade(&sync).is_err());
        let search = TradeCommandBuilder::new()
            .search_trade()
            .build()
            .get_matches_from(["trade", "search", "--status", "nope", "--format", "json"]);
        assert!(dispatcher.dispatch_trade(&search).is_err());
        let list_open = TradeCommandBuilder::new()
            .list_open()
            .build()
            .get_matches_from([
                "trade",
                "list-open",
                "--account",
                "not-a-uuid",
                "--format",
                "json",
            ]);
        assert!(dispatcher.dispatch_trade(&list_open).is_err());
        let reconcile = TradeCommandBuilder::new()
            .reconcile()
            .build()
            .get_matches_from([
                "trade",
                "reconcile",
                "--trade-id",
                "not-a-uuid",
                "--format",
                "json",
            ]);
        assert!(dispatcher.dispatch_trade(&reconcile).is_err());

        // distribution/report/market-data/grade/level
        let mut dispatcher = test_dispatcher();
        let dist = DistributionCommandBuilder::new()
            .history()
            .build()
            .get_matches_from(["distribution", "history", "--account-id", "not-a-uuid"]);
        assert!(dispatcher.dispatch_distribution(&dist).is_err());
        let report = ReportCommandBuilder::new()
            .performance()
            .build()
            .get_matches_from([
                "report",
                "--format",
                "json",
                "performance",
                "--account",
                "bad",
            ]);
        assert!(dispatcher.dispatch_report(&report).is_err());
        let market = MarketDataCommandBuilder::new()
            .bars()
            .build()
            .get_matches_from([
                "market-data",
                "bars",
                "--account",
                "bad",
                "--symbol",
                "AAPL",
                "--timeframe",
                "1d",
                "--start",
                "not-a-date",
                "--end",
                "2026-01-02T00:00:00Z",
            ]);
        assert!(dispatcher.dispatch_market_data(&market).is_err());
        let market_quote = MarketDataCommandBuilder::new()
            .quote()
            .build()
            .get_matches_from([
                "market-data",
                "quote",
                "--account",
                "bad",
                "--symbol",
                "AAPL",
                "--format",
                "json",
            ]);
        assert!(dispatcher.dispatch_market_data(&market_quote).is_err());
        let market_trade = MarketDataCommandBuilder::new()
            .trade()
            .build()
            .get_matches_from([
                "market-data",
                "trade",
                "--account",
                "bad",
                "--symbol",
                "AAPL",
                "--format",
                "json",
            ]);
        assert!(dispatcher.dispatch_market_data(&market_trade).is_err());
        let market_session = MarketDataCommandBuilder::new()
            .session()
            .build()
            .get_matches_from([
                "market-data",
                "session",
                "--account",
                "bad",
                "--symbol",
                "AAPL",
                "--format",
                "json",
            ]);
        assert!(dispatcher.dispatch_market_data(&market_session).is_err());
        let grade = GradeCommandBuilder::new().show().build().get_matches_from([
            "grade",
            "show",
            "not-a-uuid",
        ]);
        assert!(dispatcher.dispatch_grade(&grade).is_err());
        let level = LevelCommandBuilder::new()
            .status()
            .build()
            .get_matches_from(["level", "status", "--account", "bad"]);
        assert!(dispatcher.dispatch_level(&level).is_err());

        // onboarding/policy/metrics/advisor
        let mut dispatcher = test_dispatcher();
        let onboarding = OnboardingCommandBuilder::new()
            .init()
            .build()
            .get_matches_from([
                "onboarding",
                "--format",
                "json",
                "init",
                "--protected-keyword",
                "secret2",
            ]);
        assert!(dispatcher.dispatch_onboarding(&onboarding).is_err());
        let policy = PolicyCommandBuilder::new()
            .build()
            .get_matches_from(["policy", "--format", "json"]);
        assert!(dispatcher.dispatch_policy(&policy).is_ok());
        let metrics = MetricsCommandBuilder::new()
            .compare()
            .build()
            .get_matches_from([
                "metrics",
                "compare",
                "--period1",
                "invalid",
                "--period2",
                "last-7-days",
                "--format",
                "json",
            ]);
        assert!(dispatcher.dispatch_metrics(&metrics).is_ok());
        let metrics_bond = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--face-value",
                "1000",
                "--market-price",
                "950",
                "--coupon-rate",
                "4",
                "--quantity",
                "3",
                "--years-to-maturity",
                "5",
                "--format",
                "json",
            ]);
        assert!(dispatcher.dispatch_metrics(&metrics_bond).is_ok());
        let advisor = AdvisorCommandBuilder::new()
            .configure()
            .build()
            .get_matches_from([
                "advisor",
                "configure",
                "--account",
                "bad",
                "--sector-limit",
                "30",
                "--asset-class-limit",
                "40",
                "--single-position-limit",
                "20",
                "--confirm-protected",
                "secret",
            ]);
        assert!(dispatcher.dispatch_advisor(&advisor).is_err());

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
        std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
    }

    #[test]
    fn test_reporting_policy_and_onboarding_handlers_cover_text_and_json_paths() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let mut dispatcher = test_dispatcher();
        let (account, vehicle) = seed_account_and_vehicle(&mut dispatcher);
        let trade = dispatcher
            .trust
            .create_trade(
                model::DraftTrade {
                    account: account.clone(),
                    trading_vehicle: vehicle,
                    quantity: 1,
                    category: model::TradeCategory::Long,
                    currency: Currency::USD,
                    thesis: Some("t".to_string()),
                    sector: Some("Tech".to_string()),
                    asset_class: Some("Stocks".to_string()),
                    context: None,
                },
                dec!(90),
                dec!(100),
                dec!(120),
            )
            .expect("seed trade");
        let _ = dispatcher.trust.fund_trade(&trade).expect("seed funding");

        let report_matches = Command::new("report")
            .arg(Arg::new("account").long("account").required(false))
            .arg(
                Arg::new("days")
                    .long("days")
                    .required(false)
                    .value_parser(clap::value_parser!(u32)),
            )
            .arg(
                Arg::new("open-only")
                    .long("open-only")
                    .action(clap::ArgAction::SetTrue),
            )
            .get_matches_from([
                "report",
                "--account",
                &account.id.to_string(),
                "--days",
                "30",
            ]);

        assert!(dispatcher
            .drawdown_report(&report_matches, ReportOutputFormat::Text)
            .is_ok());
        assert!(dispatcher
            .drawdown_report(&report_matches, ReportOutputFormat::Json)
            .is_ok());
        assert!(dispatcher
            .performance_report(&report_matches, ReportOutputFormat::Text)
            .is_ok());
        assert!(dispatcher
            .performance_report(&report_matches, ReportOutputFormat::Json)
            .is_ok());
        assert!(dispatcher
            .risk_report(&report_matches, ReportOutputFormat::Text)
            .is_ok());
        assert!(dispatcher
            .risk_report(&report_matches, ReportOutputFormat::Json)
            .is_ok());
        assert!(dispatcher
            .concentration_report(&report_matches, ReportOutputFormat::Text)
            .is_ok());
        assert!(dispatcher
            .concentration_report(&report_matches, ReportOutputFormat::Json)
            .is_ok());
        assert!(dispatcher
            .summary_report(&report_matches, ReportOutputFormat::Text)
            .is_ok());
        assert!(dispatcher
            .summary_report(&report_matches, ReportOutputFormat::Json)
            .is_ok());
        assert!(dispatcher
            .metrics_report(&report_matches, ReportOutputFormat::Text)
            .is_ok());
        assert!(dispatcher
            .metrics_report(&report_matches, ReportOutputFormat::Json)
            .is_ok());

        assert!(dispatcher.policy_show(ReportOutputFormat::Text).is_ok());
        assert!(dispatcher.policy_show(ReportOutputFormat::Json).is_ok());
        assert!(dispatcher
            .onboarding_status(ReportOutputFormat::Text)
            .is_ok());
        assert!(dispatcher
            .onboarding_status(ReportOutputFormat::Json)
            .is_ok());

        let onboarding_matches = Command::new("onboarding")
            .arg(
                Arg::new("protected-keyword")
                    .long("protected-keyword")
                    .required(false),
            )
            .arg(
                Arg::new("overwrite")
                    .long("overwrite")
                    .action(clap::ArgAction::SetTrue),
            )
            .get_matches_from(["onboarding", "--protected-keyword", "secret", "--overwrite"]);
        assert!(dispatcher
            .onboarding_init(&onboarding_matches, ReportOutputFormat::Json)
            .is_err());

        let grade_summary_matches = Command::new("grade")
            .arg(Arg::new("account").long("account").required(false))
            .arg(
                Arg::new("days")
                    .long("days")
                    .required(false)
                    .value_parser(clap::value_parser!(u32)),
            )
            .arg(Arg::new("weights").long("weights").required(false))
            .get_matches_from([
                "grade",
                "--account",
                &account.id.to_string(),
                "--days",
                "30",
            ]);
        assert!(dispatcher
            .grade_summary(&grade_summary_matches, ReportOutputFormat::Json)
            .is_ok());

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    }

    #[test]
    fn test_report_handlers_surface_read_failures() {
        let account_id = Uuid::new_v4().to_string();
        let account_matches = report_args_matches(&["--account", &account_id]);
        let no_account_matches = report_args_matches(&[]);

        let mut dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::transactions(),
        );
        let error = dispatcher
            .drawdown_report(&account_matches, ReportOutputFormat::Json)
            .expect_err("drawdown should surface account transaction read failures");
        assert_eq!(error.code, "transaction_data_unavailable");

        let mut dispatcher =
            test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
        let error = dispatcher
            .drawdown_report(&no_account_matches, ReportOutputFormat::Json)
            .expect_err("drawdown should surface account list failures");
        assert_eq!(error.code, "transaction_data_unavailable");

        let mut dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::drawdown_equity_overflow(),
        );
        let error = dispatcher
            .drawdown_report(&account_matches, ReportOutputFormat::Json)
            .expect_err("drawdown should surface equity curve calculation failures");
        assert_eq!(error.code, "drawdown_equity_curve_failed");

        let mut dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::drawdown_metrics_overflow(),
        );
        let error = dispatcher
            .drawdown_report(&account_matches, ReportOutputFormat::Json)
            .expect_err("drawdown should surface drawdown metric calculation failures");
        assert_eq!(error.code, "drawdown_calculation_failed");

        let mut dispatcher =
            test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
        let error = dispatcher
            .performance_report(&no_account_matches, ReportOutputFormat::Json)
            .expect_err("performance should surface account list failures");
        assert_eq!(error.code, "trading_data_unavailable");

        let mut dispatcher =
            test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
        let error = dispatcher
            .summary_report(&no_account_matches, ReportOutputFormat::Json)
            .expect_err("summary should surface account list failures");
        assert_eq!(error.code, "summary_generation_failed");

        let mut dispatcher =
            test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
        let error = dispatcher
            .metrics_report(&no_account_matches, ReportOutputFormat::Json)
            .expect_err("metrics should surface account list failures");
        assert_eq!(error.code, "trading_data_unavailable");

        let mut dispatcher =
            test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
        let error = dispatcher
            .risk_report(&no_account_matches, ReportOutputFormat::Json)
            .expect_err("risk should surface open-position read failures");
        assert_eq!(error.code, "open_positions_failed");

        let mut dispatcher =
            test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
        let error = dispatcher
            .concentration_report(&no_account_matches, ReportOutputFormat::Json)
            .expect_err("concentration should surface account list failures");
        assert_eq!(error.code, "accounts_fetch_failed");
        let open_trades = ArgDispatcher::open_trades_for_scope(&mut dispatcher.trust, None);
        assert!(open_trades.is_empty());

        let mut dispatcher = test_dispatcher();
        let account = dispatcher
            .trust
            .create_account(
                "missing-balance",
                "test",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("account without balance");
        let account_matches = report_args_matches(&["--account", &account.id.to_string()]);
        let error = dispatcher
            .risk_report(&account_matches, ReportOutputFormat::Json)
            .expect_err("risk should surface missing account balance");
        assert_eq!(error.code, "account_balance_unavailable");

        let mut dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::capital_at_risk_overflow(),
        );
        let error = dispatcher
            .risk_report(&account_matches, ReportOutputFormat::Json)
            .expect_err("risk should surface capital-at-risk overflow");
        assert_eq!(error.code, "capital_at_risk_failed");

        let mut dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::open_positions_then_accounts_fail(),
        );
        dispatcher
            .risk_report(&no_account_matches, ReportOutputFormat::Json)
            .expect("aggregate risk should fall back to zero equity when account totals fail");

        let grade_summary = GradeCommandBuilder::new()
            .summary()
            .build()
            .get_matches_from(["grade", "summary"]);
        let mut dispatcher =
            test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
        let error = dispatcher
            .grade_summary(
                grade_summary
                    .subcommand_matches("summary")
                    .expect("summary subcommand"),
                ReportOutputFormat::Json,
            )
            .expect_err("grade summary should surface closed-trade read failures");
        assert_eq!(error.code, "trading_data_unavailable");

        let grade_show = GradeCommandBuilder::new().show().build().get_matches_from([
            "grade",
            "show",
            &Uuid::new_v4().to_string(),
        ]);
        let mut dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::trade_grades(),
        );
        let error = dispatcher
            .grade_show(
                grade_show
                    .subcommand_matches("show")
                    .expect("show subcommand"),
                ReportOutputFormat::Json,
            )
            .expect_err("grade show should surface grade read failures");
        assert_eq!(error.code, "grade_read_failed");

        let mut dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::trades_after_account(),
        );
        let error = dispatcher
            .find_trade_by_id(
                Uuid::new_v4(),
                &[Status::Submitted],
                ReportOutputFormat::Json,
            )
            .expect_err("trade lookup should continue after per-status read failures");
        assert_eq!(error.code, "trade_not_found");

        let advanced = MetricsCommandBuilder::new()
            .advanced()
            .build()
            .get_matches_from(["metrics", "advanced"]);
        let mut dispatcher =
            test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
        dispatcher.metrics_advanced(
            advanced
                .subcommand_matches("advanced")
                .expect("advanced subcommand"),
        );

        let compare = MetricsCommandBuilder::new()
            .compare()
            .build()
            .get_matches_from([
                "metrics",
                "compare",
                "--period1",
                "all",
                "--period2",
                "last-7-days",
            ]);
        let mut dispatcher =
            test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
        dispatcher.metrics_compare(
            compare
                .subcommand_matches("compare")
                .expect("compare subcommand"),
        );

        let bond = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from([
                "metrics",
                "bond",
                "--symbol",
                "BOND",
                "--broker",
                "ibkr",
                "--market-price",
                "95",
                "--quantity",
                "1",
            ]);
        let mut dispatcher = test_dispatcher_with_read_failures(
            crate::test_support::ReadFailureFactory::trading_vehicles(),
        );
        let error = dispatcher
            .metrics_bond(bond.subcommand_matches("bond").expect("bond subcommand"))
            .expect_err("bond lookup should surface trading-vehicle read failures");
        assert_eq!(error.code, "bond_vehicle_lookup_failed");
    }

    #[test]
    fn test_report_handlers_cover_non_empty_and_zero_branches() {
        let mut dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (account, trade) = seed_new_trade(&mut dispatcher);
        let (funded, _, _, _) = dispatcher.trust.fund_trade(&trade).expect("fund trade");
        let (submitted, _) = dispatcher
            .trust
            .submit_trade(&funded)
            .expect("submit trade");
        let mut fill_ready = submitted.clone();
        fill_ready.entry.status = model::OrderStatus::Filled;
        fill_ready.entry.filled_quantity = fill_ready.entry.quantity;
        fill_ready.entry.average_filled_price = Some(fill_ready.entry.unit_price);
        fill_ready.entry.filled_at = Some(Utc::now().naive_utc());
        let (filled, _) = dispatcher
            .trust
            .fill_trade(&fill_ready, dec!(1.25))
            .expect("fill trade with opening fee");
        let mut target_ready = filled.clone();
        target_ready.target.status = model::OrderStatus::Filled;
        target_ready.target.filled_quantity = target_ready.target.quantity;
        target_ready.target.average_filled_price = Some(dec!(110));
        target_ready.target.filled_at = Some(Utc::now().naive_utc());
        dispatcher
            .trust
            .target_acquired(&target_ready, dec!(0.75))
            .expect("target close with closing fee");
        let loss_trade = dispatcher
            .trust
            .create_trade(
                model::DraftTrade {
                    account: account.clone(),
                    trading_vehicle: trade.trading_vehicle.clone(),
                    quantity: 1,
                    category: model::TradeCategory::Long,
                    currency: Currency::USD,
                    thesis: Some("loss sample".to_string()),
                    sector: Some("technology".to_string()),
                    asset_class: Some("equity".to_string()),
                    context: None,
                },
                dec!(95),
                dec!(100),
                dec!(110),
            )
            .expect("loss sample trade should be created");
        let (loss_funded, _, _, _) = dispatcher
            .trust
            .fund_trade(&loss_trade)
            .expect("loss sample trade should be funded");
        let (loss_submitted, _) = dispatcher
            .trust
            .submit_trade(&loss_funded)
            .expect("loss sample trade should be submitted");
        let mut loss_fill_ready = loss_submitted.clone();
        loss_fill_ready.entry.status = model::OrderStatus::Filled;
        loss_fill_ready.entry.filled_quantity = loss_fill_ready.entry.quantity;
        loss_fill_ready.entry.average_filled_price = Some(loss_fill_ready.entry.unit_price);
        loss_fill_ready.entry.filled_at = Some(Utc::now().naive_utc());
        let (loss_filled, _) = dispatcher
            .trust
            .fill_trade(&loss_fill_ready, Decimal::ZERO)
            .expect("loss sample trade should be filled");
        let mut stop_ready = loss_filled.clone();
        stop_ready.safety_stop.status = model::OrderStatus::Filled;
        stop_ready.safety_stop.filled_quantity = stop_ready.safety_stop.quantity;
        stop_ready.safety_stop.average_filled_price = Some(stop_ready.safety_stop.unit_price);
        stop_ready.safety_stop.filled_at = Some(Utc::now().naive_utc());
        dispatcher
            .trust
            .stop_trade(&stop_ready, Decimal::ZERO)
            .expect("loss sample trade should close at stop");
        let report_matches =
            report_args_matches(&["--account", &account.id.to_string(), "--days", "365"]);

        dispatcher
            .performance_report(&report_matches, ReportOutputFormat::Json)
            .expect("non-empty performance json should render");
        dispatcher
            .summary_report(&report_matches, ReportOutputFormat::Json)
            .expect("fee-aware summary json should render");
        dispatcher
            .metrics_report(&report_matches, ReportOutputFormat::Json)
            .expect("fee-aware metrics json should render");
        dispatcher
            .concentration_report(
                &report_args_matches(&["--account", &account.id.to_string(), "--open-only"]),
                ReportOutputFormat::Text,
            )
            .expect("open-only concentration should handle portfolios with only closed trades");
        let open_trade = dispatcher
            .trust
            .create_trade(
                model::DraftTrade {
                    account: account.clone(),
                    trading_vehicle: trade.trading_vehicle.clone(),
                    quantity: 1,
                    category: model::TradeCategory::Long,
                    currency: Currency::USD,
                    thesis: Some("open concentration".to_string()),
                    sector: Some("technology".to_string()),
                    asset_class: Some("equity".to_string()),
                    context: None,
                },
                dec!(95),
                dec!(100),
                dec!(110),
            )
            .expect("open risk trade should be created");
        dispatcher
            .trust
            .fund_trade(&open_trade)
            .expect("open risk trade should be funded");
        let open_trade = dispatcher
            .trust
            .search_trades(account.id, Status::Funded)
            .expect("funded open risk trade should load")
            .into_iter()
            .find(|candidate| candidate.id == open_trade.id)
            .expect("funded open risk trade should be persisted");
        let (open_submitted, _) = dispatcher
            .trust
            .submit_trade(&open_trade)
            .expect("open risk trade should be submitted");
        let mut open_fill_ready = open_submitted.clone();
        open_fill_ready.entry.status = model::OrderStatus::Filled;
        open_fill_ready.entry.filled_quantity = open_fill_ready.entry.quantity;
        open_fill_ready.entry.average_filled_price = Some(open_fill_ready.entry.unit_price);
        open_fill_ready.entry.filled_at = Some(Utc::now().naive_utc());
        dispatcher
            .trust
            .fill_trade(&open_fill_ready, Decimal::ZERO)
            .expect("open risk trade should be filled");
        dispatcher
            .summary_report(&report_matches, ReportOutputFormat::Json)
            .expect("summary json should render open concentration percentage");

        let mut zero_pnl_dispatcher =
            test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
        let (zero_account, filled) = seed_filled_trade(&mut zero_pnl_dispatcher);
        let mut target_ready = filled.clone();
        target_ready.target.status = model::OrderStatus::Filled;
        target_ready.target.filled_quantity = target_ready.target.quantity;
        target_ready.target.average_filled_price = Some(target_ready.entry.unit_price);
        target_ready.target.filled_at = Some(Utc::now().naive_utc());
        zero_pnl_dispatcher
            .trust
            .target_acquired(&target_ready, Decimal::ZERO)
            .expect("zero pnl target close should be persisted");
        let attribution = ReportCommandBuilder::new()
            .attribution()
            .build()
            .get_matches_from([
                "report",
                "attribution",
                "--account",
                &zero_account.id.to_string(),
                "--by",
                "symbol",
                "--from",
                "2020-01-01",
                "--to",
                "2030-01-01",
            ]);
        let attribution_matches = attribution.subcommand_matches("attribution").unwrap();
        zero_pnl_dispatcher
            .attribution_report(attribution_matches, ReportOutputFormat::Text)
            .expect("zero pnl attribution text should render");
        zero_pnl_dispatcher
            .attribution_report(attribution_matches, ReportOutputFormat::Json)
            .expect("zero pnl attribution json should render");

        let mut empty_dispatcher = test_dispatcher();
        empty_dispatcher
            .risk_report(&report_args_matches(&[]), ReportOutputFormat::Json)
            .expect("empty aggregate risk json should render with zero equity");
        let empty_account = empty_dispatcher
            .trust
            .create_account(
                "zero-equity",
                "test",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("empty account");
        empty_dispatcher
            .summary_report(
                &report_args_matches(&["--account", &empty_account.id.to_string()]),
                ReportOutputFormat::Json,
            )
            .expect("zero-equity summary json should render");

        let mut empty_bars = StubBroker::quote_trade_fixture();
        empty_bars.bars.clear();
        let mut dispatcher = test_dispatcher_with_broker(Box::new(empty_bars));
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);
        let benchmark = ReportCommandBuilder::new()
            .benchmark()
            .build()
            .get_matches_from([
                "report",
                "benchmark",
                "--account",
                &account.id.to_string(),
                "--benchmark",
                "SPY",
                "--from",
                "2020-01-01",
                "--to",
                "2030-01-01",
            ]);
        dispatcher
            .benchmark_report(
                benchmark
                    .subcommand_matches("benchmark")
                    .expect("benchmark subcommand"),
                ReportOutputFormat::Json,
            )
            .expect("benchmark report should render with no benchmark bars");

        let mut zero_open_bars = StubBroker::quote_trade_fixture();
        if let Some(first_bar) = zero_open_bars.bars.first_mut() {
            first_bar.open = Decimal::ZERO;
        }
        let mut dispatcher = test_dispatcher_with_broker(Box::new(zero_open_bars));
        let (account, _) = seed_account_and_vehicle(&mut dispatcher);
        let benchmark = ReportCommandBuilder::new()
            .benchmark()
            .build()
            .get_matches_from([
                "report",
                "benchmark",
                "--account",
                &account.id.to_string(),
                "--benchmark",
                "SPY",
                "--from",
                "2020-01-01",
                "--to",
                "2030-01-01",
            ]);
        dispatcher
            .benchmark_report(
                benchmark
                    .subcommand_matches("benchmark")
                    .expect("benchmark subcommand"),
                ReportOutputFormat::Text,
            )
            .expect("benchmark report should render when first benchmark open is zero");

        let timeline_missing_granularity = Command::new("timeline")
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("granularity").long("granularity"))
            .arg(Arg::new("from").long("from"))
            .arg(Arg::new("to").long("to"))
            .get_matches_from([
                "timeline",
                "--account",
                &account.id.to_string(),
                "--from",
                "2020-01-01",
                "--to",
                "2030-01-01",
            ]);
        let error = dispatcher
            .timeline_report(&timeline_missing_granularity, ReportOutputFormat::Json)
            .expect_err("timeline should require granularity");
        assert_eq!(error.code, "missing_argument");
    }

    #[test]
    fn test_onboarding_init_and_status_cover_missing_and_success_paths() {
        let _guard = env_guard();
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
        std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
        crate::protected_keyword::delete().expect("reset protected keyword");
        let mut dispatcher = test_dispatcher();

        dispatcher
            .onboarding_status(ReportOutputFormat::Text)
            .expect("missing protected keyword status should render text");
        dispatcher
            .onboarding_status(ReportOutputFormat::Json)
            .expect("missing protected keyword status should render json");

        let missing_keyword = Command::new("onboarding")
            .arg(Arg::new("protected-keyword").long("protected-keyword"))
            .get_matches_from(["onboarding"]);
        let error = dispatcher
            .onboarding_init(&missing_keyword, ReportOutputFormat::Json)
            .expect_err("missing onboarding keyword should fail");
        assert_eq!(error.code, "missing_keyword");

        let blank_keyword = Command::new("onboarding")
            .arg(Arg::new("protected-keyword").long("protected-keyword"))
            .get_matches_from(["onboarding", "--protected-keyword", "   "]);
        let error = dispatcher
            .onboarding_init(&blank_keyword, ReportOutputFormat::Json)
            .expect_err("blank onboarding keyword should fail");
        assert_eq!(error.code, "onboarding_store_failed");

        let text_matches = Command::new("onboarding")
            .arg(Arg::new("protected-keyword").long("protected-keyword"))
            .get_matches_from(["onboarding", "--protected-keyword", "first-secret"]);
        dispatcher
            .onboarding_init(&text_matches, ReportOutputFormat::Text)
            .expect("onboarding text init should store keyword");
        assert_eq!(
            crate::protected_keyword::read_expected().expect("keyword should be stored"),
            "first-secret"
        );

        crate::protected_keyword::delete().expect("clear protected keyword before json init");
        let json_matches = Command::new("onboarding")
            .arg(Arg::new("protected-keyword").long("protected-keyword"))
            .get_matches_from(["onboarding", "--protected-keyword", "second-secret"]);
        dispatcher
            .onboarding_init(&json_matches, ReportOutputFormat::Json)
            .expect("onboarding json init should store keyword");
        assert_eq!(
            crate::protected_keyword::read_expected().expect("keyword should be stored"),
            "second-secret"
        );

        crate::protected_keyword::delete().expect("cleanup protected keyword");
        std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
    }

    #[test]
    fn test_dispatch_trade_interactive_variants_are_reachable() {
        use std::panic::{catch_unwind, AssertUnwindSafe};

        let mut dispatcher = test_dispatcher();
        let cases: Vec<Vec<&str>> = vec![
            vec!["trade", "manually-fill"],
            vec!["trade", "manually-stop"],
            vec!["trade", "manually-target"],
            vec!["trade", "manually-close"],
            vec!["trade", "manually-close", "--auto-distribute"],
            vec!["trade", "watch"],
            vec!["trade", "search"],
            vec!["trade", "modify-stop"],
            vec!["trade", "modify-target"],
        ];

        for argv in cases {
            let matches = TradeCommandBuilder::new()
                .manually_fill()
                .manually_stop()
                .manually_target()
                .manually_close()
                .watch_trade()
                .search_trade()
                .modify_stop()
                .modify_target()
                .build()
                .get_matches_from(argv);

            let result = catch_unwind(AssertUnwindSafe(|| dispatcher.dispatch_trade(&matches)));
            let _ = result;
        }
    }

    #[test]
    fn test_dispatch_report_and_grade_remaining_subcommand_arms() {
        let mut dispatcher = test_dispatcher();
        let invalid_account = "not-a-uuid";
        let report_commands: Vec<Vec<&str>> = vec![
            vec!["report", "risk", "--account", invalid_account],
            vec!["report", "concentration", "--account", invalid_account],
            vec!["report", "summary", "--account", invalid_account],
            vec!["report", "metrics", "--account", invalid_account],
            vec![
                "report",
                "attribution",
                "--account",
                invalid_account,
                "--by",
                "symbol",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ],
            vec![
                "report",
                "benchmark",
                "--account",
                invalid_account,
                "--benchmark",
                "SPY",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ],
            vec![
                "report",
                "timeline",
                "--account",
                invalid_account,
                "--granularity",
                "day",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ],
            vec![
                "report",
                "bias-summary",
                "--account",
                invalid_account,
                "--days",
                "7",
            ],
        ];

        for argv in report_commands {
            let matches = ReportCommandBuilder::new()
                .risk()
                .concentration()
                .summary()
                .metrics()
                .attribution()
                .benchmark()
                .timeline()
                .bias_summary()
                .build()
                .get_matches_from(argv);
            let error = dispatcher
                .dispatch_report(&matches)
                .expect_err("invalid account should fail");
            assert!(
                error.to_string().contains("invalid_account_id")
                    || error.to_string().contains("account_not_found")
            );
        }

        let grade_summary = GradeCommandBuilder::new()
            .summary()
            .build()
            .get_matches_from(["grade", "summary", "--account", invalid_account]);
        let grade_error = dispatcher
            .dispatch_grade(&grade_summary)
            .expect_err("invalid grade summary account should fail");
        assert!(grade_error.to_string().contains("invalid_account_id"));
    }

    #[test]
    fn test_dispatch_report_new_subcommands_cover_success_and_failure_routes() {
        let mut dispatcher = test_dispatcher();
        let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);
        let account_id = account.id.to_string();

        let attribution_json = ReportCommandBuilder::new()
            .attribution()
            .build()
            .get_matches_from([
                "report",
                "--format",
                "json",
                "attribution",
                "--account",
                &account_id,
                "--by",
                "symbol",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ]);
        dispatcher
            .dispatch_report(&attribution_json)
            .expect("attribution json should dispatch");

        let timeline_text = ReportCommandBuilder::new()
            .timeline()
            .build()
            .get_matches_from([
                "report",
                "timeline",
                "--account",
                &account_id,
                "--granularity",
                "day",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ]);
        dispatcher
            .dispatch_report(&timeline_text)
            .expect("timeline text should dispatch");

        let benchmark = ReportCommandBuilder::new()
            .benchmark()
            .build()
            .get_matches_from([
                "report",
                "--format",
                "json",
                "benchmark",
                "--account",
                &account_id,
                "--benchmark",
                "SPY",
                "--from",
                "2026-01-01",
                "--to",
                "2026-01-31",
            ]);
        let err = dispatcher
            .dispatch_report(&benchmark)
            .expect_err("benchmark should fail with default test broker");
        assert!(err.to_string().contains("benchmark_data_failed"));
    }

    #[test]
    fn test_database_url_prefers_env_override() {
        let _guard = env_guard();
        std::env::set_var("TRUST_DB_URL", "/tmp/trust-custom.db");
        let value = ArgDispatcher::database_url();
        assert_eq!(value, "/tmp/trust-custom.db");
        std::env::remove_var("TRUST_DB_URL");
    }

    #[test]
    fn test_database_url_uses_default_when_env_missing() {
        let _guard = env_guard();
        std::env::remove_var("TRUST_DB_URL");
        let value = ArgDispatcher::database_url();
        assert!(
            value.ends_with("/.trust/debug.db") || value.ends_with("/.trust/production.db"),
            "unexpected database path: {value}"
        );
    }

    #[test]
    fn test_create_dir_if_necessary_creates_trust_directory() {
        let _guard = env_guard();
        let original_home = std::env::var_os("HOME");
        let home = std::env::temp_dir().join(format!("trust-home-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&home).expect("temp home should be created");
        std::env::set_var("HOME", &home);

        let trust_dir = home.join(".trust");
        assert!(!trust_dir.exists());
        super::create_dir_if_necessary();
        assert!(trust_dir.exists());
        super::create_dir_if_necessary();

        std::env::remove_var("HOME");
        let _ = std::fs::remove_dir_all(home);

        let home = std::env::temp_dir().join(format!("trust-home-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&home).expect("temp home should be created");
        std::env::set_var("HOME", &home);

        super::create_dir_if_necessary();

        std::env::remove_var("HOME");
        let _ = std::fs::remove_dir_all(home);

        if let Some(original_home) = original_home {
            std::env::set_var("HOME", original_home);
        }
    }

    #[test]
    fn test_new_sqlite_dispatcher_initializes_with_custom_db_url() {
        let _guard = env_guard();
        let db_path = format!("/tmp/trust-cli-{}.db", Uuid::new_v4());
        std::env::set_var("TRUST_DB_URL", &db_path);

        let _dispatcher = ArgDispatcher::new_sqlite();

        assert!(
            std::path::Path::new(&db_path).exists(),
            "sqlite database file should be created"
        );
        std::env::remove_var("TRUST_DB_URL");
        let _ = std::fs::remove_file(&db_path);
    }

    #[test]
    fn test_db_export_import_success_and_export_error_paths() {
        let _guard = env_guard();
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        let db_path = format!("/tmp/trust-db-dispatch-{}.db", Uuid::new_v4());
        let backup_path = format!("/tmp/trust-db-dispatch-{}.json", Uuid::new_v4());
        std::env::set_var("TRUST_DB_URL", &db_path);
        let mut dispatcher = test_dispatcher();

        let export = DbCommandBuilder::new().export().build().get_matches_from([
            "db",
            "export",
            "--output",
            &backup_path,
        ]);
        dispatcher
            .db_export(export.subcommand_matches("export").unwrap())
            .expect("database export should write backup");
        assert!(std::path::Path::new(&backup_path).exists());

        let import = DbCommandBuilder::new().import().build().get_matches_from([
            "db",
            "import",
            "--input",
            &backup_path,
            "--mode",
            "replace",
            "--dry-run",
            "--confirm-protected",
            "secret",
        ]);
        dispatcher
            .db_import(import.subcommand_matches("import").unwrap())
            .expect("database import dry run should validate backup");

        let export_to_directory = DbCommandBuilder::new()
            .export()
            .build()
            .get_matches_from(["db", "export", "--output", "/tmp"]);
        let error = dispatcher
            .db_export(export_to_directory.subcommand_matches("export").unwrap())
            .expect_err("exporting to a directory should fail");
        assert!(error.to_string().contains("db_export_failed"));

        std::env::remove_var("TRUST_DB_URL");
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(&backup_path);
    }

    #[test]
    fn test_top_level_dispatch_routes_all_command_families_and_external() {
        let _guard = env_guard();
        std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");

        let cases: Vec<(Vec<&str>, bool)> = vec![
            (
                vec![
                    "trust",
                    "db",
                    "import",
                    "--input",
                    "/tmp/does-not-exist.json",
                ],
                true,
            ),
            (vec!["trust", "keys", "protected-show"], false),
            (vec!["trust", "accounts", "list"], false),
            (
                vec![
                    "trust",
                    "transaction",
                    "deposit",
                    "--account",
                    "missing",
                    "--currency",
                    "usd",
                    "--amount",
                    "10",
                    "--confirm-protected",
                    "secret",
                ],
                true,
            ),
            (vec!["trust", "rule", "remove"], true),
            (
                vec![
                    "trust",
                    "trading-vehicle",
                    "create",
                    "--from-alpaca",
                    "--symbol",
                    "AAPL",
                ],
                true,
            ),
            (
                vec!["trust", "trade", "fund", "--trade-id", "not-a-uuid"],
                true,
            ),
            (
                vec![
                    "trust",
                    "distribution",
                    "history",
                    "--account-id",
                    "not-a-uuid",
                ],
                true,
            ),
            (
                vec![
                    "trust",
                    "report",
                    "--format",
                    "json",
                    "summary",
                    "--account",
                    "bad",
                ],
                true,
            ),
            (
                vec![
                    "trust",
                    "market-data",
                    "--format",
                    "json",
                    "snapshot",
                    "--account",
                    "bad",
                    "--symbol",
                    "AAPL",
                ],
                true,
            ),
            (vec!["trust", "grade", "show", "not-a-uuid"], true),
            (vec!["trust", "level", "status", "--account", "bad"], true),
            (
                vec!["trust", "onboarding", "--format", "json", "status"],
                false,
            ),
            (vec!["trust", "policy", "--format", "json"], false),
            (
                vec![
                    "trust",
                    "metrics",
                    "compare",
                    "--period1",
                    "invalid",
                    "--period2",
                    "last-7-days",
                    "--format",
                    "json",
                ],
                false,
            ),
            (
                vec![
                    "trust",
                    "advisor",
                    "configure",
                    "--account",
                    "bad",
                    "--sector-limit",
                    "30",
                    "--asset-class-limit",
                    "40",
                    "--single-position-limit",
                    "20",
                    "--confirm-protected",
                    "secret",
                ],
                true,
            ),
            (vec!["trust", "trade", "fund", "--unknown"], true),
            (vec!["trust", "external-cmd", "--foo", "bar"], false),
        ];

        for (argv, should_err) in cases {
            let observed_error = match root_command().try_get_matches_from(argv) {
                Ok(matches) => {
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        test_dispatcher().dispatch(matches)
                    }));
                    match outcome {
                        Ok(result) => result.is_err(),
                        Err(_) => true,
                    }
                }
                Err(_) => true,
            };
            assert_eq!(observed_error, should_err);
        }

        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
        std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
    }

    #[test]
    fn test_dispatch_level_advisor_and_keys_remaining_subcommands_are_reachable() {
        let _guard = env_guard();
        std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
        std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
        crate::protected_keyword::delete().expect("reset protected keyword");

        let mut dispatcher = test_dispatcher();

        let level_history = LevelCommandBuilder::new()
            .history()
            .build()
            .get_matches_from(["level", "history", "--account", "not-a-uuid"]);
        let level_history_error = dispatcher
            .dispatch_level(&level_history)
            .expect_err("history with invalid account should fail");
        assert!(level_history_error
            .to_string()
            .contains("invalid_account_id"));

        let level_change = LevelCommandBuilder::new()
            .change()
            .build()
            .get_matches_from([
                "level",
                "change",
                "--account",
                "not-a-uuid",
                "--to",
                "1",
                "--reason",
                "manual adjust",
                "--confirm-protected",
                "secret",
            ]);
        let level_change_error = dispatcher
            .dispatch_level(&level_change)
            .expect_err("change with invalid account should fail");
        assert!(level_change_error
            .to_string()
            .contains("invalid_account_id"));

        let level_evaluate = LevelCommandBuilder::new()
            .evaluate()
            .build()
            .get_matches_from([
                "level",
                "evaluate",
                "--account",
                "not-a-uuid",
                "--profitable-trades",
                "2",
                "--win-rate",
                "60",
                "--monthly-loss",
                "3",
                "--largest-loss",
                "1",
            ]);
        let level_evaluate_error = dispatcher
            .dispatch_level(&level_evaluate)
            .expect_err("evaluate with invalid account should fail");
        assert!(level_evaluate_error
            .to_string()
            .contains("invalid_account_id"));

        let level_progress = LevelCommandBuilder::new()
            .progress()
            .build()
            .get_matches_from([
                "level",
                "progress",
                "--account",
                "not-a-uuid",
                "--profitable-trades",
                "2",
                "--win-rate",
                "60",
                "--monthly-loss",
                "3",
                "--largest-loss",
                "1",
            ]);
        let level_progress_error = dispatcher
            .dispatch_level(&level_progress)
            .expect_err("progress with invalid account should fail");
        assert!(level_progress_error
            .to_string()
            .contains("invalid_account_id"));

        let level_rules_show = LevelCommandBuilder::new()
            .rules()
            .build()
            .get_matches_from(["level", "rules", "show", "--account", "not-a-uuid"]);
        let level_rules_show_error = dispatcher
            .dispatch_level(&level_rules_show)
            .expect_err("rules show with invalid account should fail");
        assert!(level_rules_show_error
            .to_string()
            .contains("invalid_account_id"));

        let level_rules_set = LevelCommandBuilder::new()
            .rules()
            .build()
            .get_matches_from([
                "level",
                "rules",
                "set",
                "--account",
                "not-a-uuid",
                "--rule",
                "monthly_loss_downgrade_pct",
                "--value",
                "8",
                "--confirm-protected",
                "secret",
            ]);
        let level_rules_set_error = dispatcher
            .dispatch_level(&level_rules_set)
            .expect_err("rules set with invalid account should fail");
        assert!(level_rules_set_error
            .to_string()
            .contains("invalid_account_id"));

        let advisor_status = AdvisorCommandBuilder::new()
            .status()
            .build()
            .get_matches_from(["advisor", "status", "--account", "not-a-uuid"]);
        let advisor_status_error = dispatcher
            .dispatch_advisor(&advisor_status)
            .expect_err("advisor status with invalid account should fail");
        assert!(advisor_status_error.to_string().contains("invalid_uuid"));

        let advisor_history = AdvisorCommandBuilder::new()
            .history()
            .build()
            .get_matches_from([
                "advisor",
                "history",
                "--account",
                "not-a-uuid",
                "--days",
                "14",
            ]);
        let advisor_history_error = dispatcher
            .dispatch_advisor(&advisor_history)
            .expect_err("advisor history with invalid account should fail");
        assert!(advisor_history_error.to_string().contains("invalid_uuid"));

        let advisor_account = dispatcher
            .trust
            .create_account(
                "advisor-warning-account",
                "advisory warnings",
                Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("advisor account should be created");
        let advisor_check = AdvisorCommandBuilder::new()
            .check()
            .build()
            .get_matches_from([
                "advisor",
                "check",
                "--account",
                &advisor_account.id.to_string(),
                "--symbol",
                "AAPL",
                "--entry",
                "100",
                "--quantity",
                "10",
                "--sector",
                "technology",
                "--asset-class",
                "equity",
            ]);
        dispatcher
            .dispatch_advisor(&advisor_check)
            .expect("advisor check should print warnings and recommendations");

        let keys_show = KeysCommandBuilder::new()
            .read_environment()
            .build()
            .get_matches_from(["keys", "show"]);
        let keys_show_outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = dispatcher.dispatch_keys(&keys_show);
        }));
        assert!(
            keys_show_outcome.is_err(),
            "keys show should reach dialog branch even when selection panics"
        );

        let keys_create = KeysCommandBuilder::new()
            .create_keys()
            .build()
            .get_matches_from(["keys", "create", "--confirm-protected", "secret"]);
        let keys_create_outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = dispatcher.dispatch_keys(&keys_create);
        }));
        assert!(
            keys_create_outcome.is_err(),
            "keys create should reach dialog branch even when account selection panics"
        );

        let keys_delete = KeysCommandBuilder::new()
            .delete_environment()
            .build()
            .get_matches_from(["keys", "delete", "--confirm-protected", "secret"]);
        let keys_delete_outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = dispatcher.dispatch_keys(&keys_delete);
        }));
        assert!(
            keys_delete_outcome.is_err(),
            "keys delete should reach dialog branch even when environment selection panics"
        );

        crate::protected_keyword::delete().expect("cleanup protected keyword");
        std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
        std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
    }
}
