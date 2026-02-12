use crate::dialogs::{
    AccountDialogBuilder, AccountSearchDialog, CancelDialogBuilder, CloseDialogBuilder,
    ExitDialogBuilder, FillTradeDialogBuilder, FundingDialogBuilder, KeysDeleteDialogBuilder,
    KeysReadDialogBuilder, KeysWriteDialogBuilder, ModifyDialogBuilder, SubmitDialogBuilder,
    SyncTradeDialogBuilder, TradeDialogBuilder, TradeSearchDialogBuilder,
    TradingVehicleDialogBuilder, TradingVehicleSearchDialogBuilder, TransactionDialogBuilder,
};
use crate::dialogs::{RuleDialogBuilder, RuleRemoveDialogBuilder};
use alpaca_broker::AlpacaBroker;
use clap::ArgMatches;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{database::TradingVehicleUpsert, Trade, TradingVehicleCategory, TransactionCategory};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use shellexpand::tilde;
use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::fs::create_dir_all;
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReportOutputFormat {
    Text,
    Json,
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

pub struct ArgDispatcher {
    trust: TrustFacade,
}

impl ArgDispatcher {
    pub fn new_sqlite() -> Self {
        create_dir_if_necessary();
        let database = SqliteDatabase::new(ArgDispatcher::database_url().as_str());

        ArgDispatcher {
            trust: TrustFacade::new(Box::new(database), Box::<AlpacaBroker>::default()),
        }
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

    #[allow(clippy::too_many_lines)]
    pub fn dispatch(mut self, matches: ArgMatches) -> Result<(), CliError> {
        match matches.subcommand() {
            Some(("keys", sub_matches)) => match sub_matches.subcommand() {
                Some(("create", _)) => self.create_keys(),
                Some(("show", _)) => self.show_keys(),
                Some(("delete", _)) => self.delete_keys(),
                _ => unreachable!("No subcommand provided"),
            },
            Some(("account", sub_matches)) => match sub_matches.subcommand() {
                Some(("create", _)) => self.create_account(),
                Some(("search", _)) => self.search_account(),
                _ => unreachable!("No subcommand provided"),
            },
            Some(("transaction", sub_matches)) => match sub_matches.subcommand() {
                Some(("deposit", _)) => self.deposit(),
                Some(("withdraw", _)) => self.withdraw(),
                _ => unreachable!("No subcommand provided"),
            },
            Some(("rule", sub_matches)) => match sub_matches.subcommand() {
                Some(("create", _)) => self.create_rule(),
                Some(("remove", _)) => self.remove_rule(),
                _ => unreachable!("No subcommand provided"),
            },
            Some(("trading-vehicle", sub_matches)) => match sub_matches.subcommand() {
                Some(("create", sub_sub_matches)) => {
                    self.create_trading_vehicle(sub_sub_matches)?
                }
                Some(("search", _)) => self.search_trading_vehicle(),
                _ => unreachable!("No subcommand provided"),
            },
            Some(("trade", sub_matches)) => match sub_matches.subcommand() {
                Some(("create", _)) => self.create_trade(),
                Some(("fund", _)) => self.create_funding(),
                Some(("cancel", _)) => self.create_cancel(),
                Some(("submit", _)) => self.create_submit(),
                Some(("manually-fill", _)) => self.create_fill(),
                Some(("manually-stop", _)) => self.create_stop(),
                Some(("manually-target", _)) => self.create_target(),
                Some(("manually-close", _)) => self.close(),
                Some(("sync", _)) => self.create_sync(),
                Some(("search", _)) => self.search_trade(),
                Some(("modify-stop", _)) => self.modify_stop(),
                Some(("modify-target", _)) => self.modify_target(),
                _ => unreachable!("No subcommand provided"),
            },
            Some(("report", sub_matches)) => match sub_matches.subcommand() {
                Some(("performance", sub_sub_matches)) => self
                    .performance_report(sub_sub_matches, Self::parse_report_format(sub_matches))?,
                Some(("drawdown", sub_sub_matches)) => {
                    self.drawdown_report(sub_sub_matches, Self::parse_report_format(sub_matches))?
                }
                Some(("risk", sub_sub_matches)) => {
                    self.risk_report(sub_sub_matches, Self::parse_report_format(sub_matches))?
                }
                Some(("concentration", sub_sub_matches)) => self.concentration_report(
                    sub_sub_matches,
                    Self::parse_report_format(sub_matches),
                )?,
                Some(("summary", sub_sub_matches)) => {
                    self.summary_report(sub_sub_matches, Self::parse_report_format(sub_matches))?
                }
                Some(("metrics", sub_sub_matches)) => {
                    self.metrics_report(sub_sub_matches, Self::parse_report_format(sub_matches))?
                }
                _ => unreachable!("No subcommand provided"),
            },
            Some(("grade", sub_matches)) => match sub_matches.subcommand() {
                Some(("show", sub_sub_matches)) => {
                    self.grade_show(sub_sub_matches, Self::parse_report_format(sub_matches))?
                }
                Some(("summary", sub_sub_matches)) => {
                    self.grade_summary(sub_sub_matches, Self::parse_report_format(sub_matches))?
                }
                _ => unreachable!("No subcommand provided"),
            },
            Some(("metrics", sub_matches)) => match sub_matches.subcommand() {
                Some(("advanced", sub_sub_matches)) => self.metrics_advanced(sub_sub_matches),
                Some(("compare", sub_sub_matches)) => self.metrics_compare(sub_sub_matches),
                _ => unreachable!("No subcommand provided"),
            },
            Some((ext, sub_matches)) => {
                let args = sub_matches
                    .get_many::<OsString>("")
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                println!("Calling out to {ext:?} with {args:?}");
            }
            _ => unreachable!(), // If all subcommands are defined above, anything else is unreachable!()
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
        let content = serde_json::to_string_pretty(payload).map_err(|error| {
            CliError::new(
                "json_serialization_failed",
                format!("Failed to serialize report payload: {error}"),
            )
        })?;
        println!("{content}");
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
    fn create_account(&mut self) {
        AccountDialogBuilder::new()
            .name()
            .description()
            .environment()
            .tax_percentage()
            .earnings_percentage()
            .build(&mut self.trust)
            .display();
    }

    fn search_account(&mut self) {
        AccountSearchDialog::new()
            .search(&mut self.trust)
            .display(&mut self.trust);
    }
}

// Transaction
impl ArgDispatcher {
    fn deposit(&mut self) {
        TransactionDialogBuilder::new(TransactionCategory::Deposit)
            .account(&mut self.trust)
            .currency(&mut self.trust)
            .amount(&mut self.trust)
            .build(&mut self.trust)
            .display();
    }

    fn withdraw(&mut self) {
        TransactionDialogBuilder::new(TransactionCategory::Withdrawal)
            .account(&mut self.trust)
            .currency(&mut self.trust)
            .amount(&mut self.trust)
            .build(&mut self.trust)
            .display();
    }
}

// Rules
impl ArgDispatcher {
    fn create_rule(&mut self) {
        RuleDialogBuilder::new()
            .account(&mut self.trust)
            .name()
            .risk()
            .description()
            .level()
            .build(&mut self.trust)
            .display();
    }

    fn remove_rule(&mut self) {
        RuleRemoveDialogBuilder::new()
            .account(&mut self.trust)
            .select_rule(&mut self.trust)
            .build(&mut self.trust)
            .display();
    }
}

// Trading Vehicle
impl ArgDispatcher {
    fn create_trading_vehicle(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        if matches.get_flag("from-alpaca") {
            return self.create_trading_vehicle_from_alpaca(matches);
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

    fn create_trading_vehicle_from_alpaca(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        let account_name = match matches.get_one::<String>("account") {
            Some(value) if !value.trim().is_empty() => value.trim(),
            _ => {
                return Err(CliError::new(
                    "alpaca_import_invalid_args",
                    "--account is required when using --from-alpaca",
                ));
            }
        };

        let symbol = match matches.get_one::<String>("symbol") {
            Some(value) if !value.trim().is_empty() => value.trim(),
            _ => {
                return Err(CliError::new(
                    "alpaca_import_invalid_args",
                    "--symbol is required when using --from-alpaca",
                ));
            }
        };

        let account = match self.trust.search_account(account_name) {
            Ok(account) => account,
            Err(error) => {
                return Err(CliError::new(
                    "alpaca_import_account_not_found",
                    format!("{error}"),
                ));
            }
        };

        let metadata = match AlpacaBroker::fetch_asset_metadata(&account, symbol) {
            Ok(metadata) => metadata,
            Err(error) => {
                return Err(CliError::new("alpaca_import_failed", format!("{error}")));
            }
        };

        if !metadata.is_active {
            return Err(CliError::new(
                "alpaca_import_unavailable",
                format!("symbol '{}' is inactive", metadata.symbol),
            ));
        }

        if !metadata.tradable {
            return Err(CliError::new(
                "alpaca_import_unavailable",
                format!("symbol '{}' is not tradable", metadata.symbol),
            ));
        }

        let upsert = TradingVehicleUpsert {
            symbol: metadata.symbol.clone(),
            isin: None,
            category: metadata.category,
            broker: "alpaca".to_string(),
            broker_asset_id: Some(metadata.broker_identifier.clone()),
            exchange: Some(metadata.exchange.clone()),
            broker_asset_class: None,
            broker_asset_status: Some(if metadata.is_active {
                "active".to_string()
            } else {
                "inactive".to_string()
            }),
            tradable: Some(metadata.tradable),
            marginable: Some(metadata.marginable),
            shortable: Some(metadata.shortable),
            easy_to_borrow: Some(metadata.easy_to_borrow),
            fractionable: Some(metadata.fractionable),
        };

        let result = self.trust.upsert_trading_vehicle(upsert);

        match result {
            Ok(tv) => {
                println!(
                    "Imported from Alpaca: symbol={}, category={}, exchange={}, tradable={}, marginable={}, shortable={}, fractionable={}, broker_id={}",
                    tv.symbol,
                    category_to_str(tv.category),
                    metadata.exchange,
                    metadata.tradable,
                    metadata.marginable,
                    metadata.shortable,
                    metadata.fractionable,
                    metadata.broker_identifier,
                );
                crate::views::TradingVehicleView::display(tv);
            }
            Err(error) => {
                return Err(CliError::new(
                    "alpaca_import_store_failed",
                    format!("{error}"),
                ));
            }
        }
        Ok(())
    }

    fn search_trading_vehicle(&mut self) {
        TradingVehicleSearchDialogBuilder::new()
            .search(&mut self.trust)
            .display();
    }
}

fn category_to_str(category: TradingVehicleCategory) -> &'static str {
    match category {
        TradingVehicleCategory::Crypto => "crypto",
        TradingVehicleCategory::Fiat => "fiat",
        TradingVehicleCategory::Stock => "stock",
        _ => "unknown",
    }
}

// Trade
impl ArgDispatcher {
    fn create_trade(&mut self) {
        TradeDialogBuilder::new()
            .account(&mut self.trust)
            .trading_vehicle(&mut self.trust)
            .category()
            .entry_price()
            .stop_price()
            .currency(&mut self.trust)
            .quantity(&mut self.trust)
            .target_price()
            .thesis()
            .sector()
            .asset_class()
            .context()
            .build(&mut self.trust)
            .display();
    }

    fn create_cancel(&mut self) {
        CancelDialogBuilder::new()
            .account(&mut self.trust)
            .search(&mut self.trust)
            .build(&mut self.trust)
            .display();
    }

    fn create_funding(&mut self) {
        FundingDialogBuilder::new()
            .account(&mut self.trust)
            .search(&mut self.trust)
            .build(&mut self.trust)
            .display();
    }

    fn create_submit(&mut self) {
        SubmitDialogBuilder::new()
            .account(&mut self.trust)
            .search(&mut self.trust)
            .build(&mut self.trust)
            .display();
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

    fn create_sync(&mut self) {
        SyncTradeDialogBuilder::new()
            .account(&mut self.trust)
            .search(&mut self.trust)
            .build(&mut self.trust)
            .display();
    }

    fn search_trade(&mut self) {
        TradeSearchDialogBuilder::new()
            .account(&mut self.trust)
            .status()
            .show_balance()
            .search(&mut self.trust)
            .display();
    }

    fn close(&mut self) {
        CloseDialogBuilder::new()
            .account(&mut self.trust)
            .search(&mut self.trust)
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

    fn create_keys(&mut self) {
        KeysWriteDialogBuilder::new()
            .account(&mut self.trust)
            .environment()
            .url()
            .key_id()
            .key_secret()
            .build()
            .display();
    }

    fn show_keys(&mut self) {
        KeysReadDialogBuilder::new()
            .account(&mut self.trust)
            .environment()
            .build()
            .display();
    }

    fn delete_keys(&mut self) {
        KeysDeleteDialogBuilder::new()
            .account(&mut self.trust)
            .environment()
            .build()
            .display();
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
        use core::calculators_drawdown::RealizedDrawdownCalculator;
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
            println!("Equity: ${:.2}", summary.equity);
            println!(
                "Performance: {} trades ({}W/{}L), win rate {:.1}%, avg R {:.2}",
                performance.total_trades,
                performance.winning_trades,
                performance.losing_trades,
                performance.win_rate,
                performance.average_r_multiple
            );
            println!(
                "Risk: ${:.2} at risk ({:.2}% of equity), {} open positions",
                total_risk,
                risk_pct,
                summary.capital_at_risk.len()
            );
            println!("Concentration groups: {}", summary.concentration.len());
            return Ok(());
        }

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
            "sortino_ratio": AdvancedMetricsCalculator::calculate_sortino_ratio(&closed_trades, dec!(0.05)).map(Self::decimal_string),
            "calmar_ratio": AdvancedMetricsCalculator::calculate_calmar_ratio(&closed_trades).map(Self::decimal_string),
            "value_at_risk_95": AdvancedMetricsCalculator::calculate_value_at_risk(&closed_trades, dec!(0.95)).map(Self::decimal_string),
            "expected_shortfall_95": AdvancedMetricsCalculator::calculate_expected_shortfall(&closed_trades, dec!(0.95)).map(Self::decimal_string),
            "kelly_criterion": AdvancedMetricsCalculator::calculate_kelly_criterion(&closed_trades).map(Self::decimal_string),
            "max_consecutive_wins": AdvancedMetricsCalculator::calculate_max_consecutive_wins(&closed_trades),
            "max_consecutive_losses": AdvancedMetricsCalculator::calculate_max_consecutive_losses(&closed_trades),
            "ulcer_index": AdvancedMetricsCalculator::calculate_ulcer_index(&closed_trades).map(Self::decimal_string),
            "risk_of_ruin_proxy_100t_8l": AdvancedMetricsCalculator::calculate_risk_of_ruin_proxy(&closed_trades, 100, 8).map(Self::decimal_string)
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
        let exposure = AdvancedMetricsCalculator::calculate_exposure_metrics(&open_trades_for_scope);
        let bootstrap = AdvancedMetricsCalculator::calculate_bootstrap_confidence_intervals(
            &closed_trades,
            200,
            dec!(0.05),
        );

        let transactions = if let Some(id) = account_id {
            self.trust
                .get_account_transactions(id)
                .unwrap_or_else(|_| Vec::new())
        } else {
            self.trust
                .get_all_transactions()
                .unwrap_or_else(|_| Vec::new())
        };

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
                total.checked_div(Decimal::from(closed_trades.len()))
                    .unwrap_or(dec!(0))
            };
            (fee_open, fee_close, total, per_trade)
        };

        let drawdown_metrics = {
            let curve = RealizedDrawdownCalculator::calculate_equity_curve(&transactions)
                .unwrap_or(core::calculators_drawdown::RealizedEquityCurve { points: Vec::new() });
            RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve).unwrap_or(
                core::calculators_drawdown::DrawdownMetrics {
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
                },
            )
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

        if format == ReportOutputFormat::Text {
            AdvancedMetricsView::display(all_trades);
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
        let sortino = AdvancedMetricsCalculator::calculate_sortino_ratio(&all_trades, dec!(0.05));
        let calmar = AdvancedMetricsCalculator::calculate_calmar_ratio(&all_trades);
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
        let exposure = AdvancedMetricsCalculator::calculate_exposure_metrics(&open_trades_for_scope);
        let bootstrap = AdvancedMetricsCalculator::calculate_bootstrap_confidence_intervals(
            &all_trades,
            200,
            dec!(0.05),
        );
        let agent_signals =
            Self::metrics_agent_signals(expectancy, win_rate, risk_of_ruin, max_losses);

        let (fee_open_total, fee_close_total, fees_total, fees_per_closed_trade) = {
            let mut txs = if let Some(id) = account_id {
                self.trust
                    .get_account_transactions(id)
                    .unwrap_or_else(|_| Vec::new())
            } else {
                self.trust.get_all_transactions().unwrap_or_else(|_| Vec::new())
            };

            if let Some(days) = days_filter {
                use chrono::{Duration, Utc};
                let now = Utc::now().naive_utc();
                let cutoff = now
                    .checked_sub_signed(Duration::days(i64::from(days)))
                    .unwrap_or(now);
                txs.retain(|tx| tx.created_at >= cutoff);
            }

            let mut fee_open = dec!(0);
            let mut fee_close = dec!(0);
            for tx in &txs {
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
                total.checked_div(Decimal::from(all_trades.len()))
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
                "risk_adjusted_performance": {
                    "sharpe_ratio": sharpe.map(Self::decimal_string),
                    "sortino_ratio": sortino.map(Self::decimal_string),
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

        let existing = self
            .trust
            .latest_trade_grade(trade_id)
            .map_err(|e| {
                Self::report_error(
                    format,
                    "grade_read_failed",
                    format!("Failed to read existing grade: {e}"),
                )
            })?;

        let grade = if let Some(existing) = existing {
            if regrade {
                let weights = requested_weights.unwrap_or(
                    core::services::grading::GradingWeightsPermille {
                        process: existing.process_weight_permille,
                        risk: existing.risk_weight_permille,
                        execution: existing.execution_weight_permille,
                        documentation: existing.documentation_weight_permille,
                    },
                );
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

        let mut closed_trades = self
            .trust
            .search_closed_trades(account_id)
            .map_err(|e| {
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
                Some(g) => g.process_weight_permille != weights.process
                    || g.risk_weight_permille != weights.risk
                    || g.execution_weight_permille != weights.execution
                    || g.documentation_weight_permille != weights.documentation,
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
        let count = grades.len() as u32;

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
            *dist.entry(g.overall_grade.to_string()).or_insert(0) += 1;
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

    fn parse_grade_weights(
        matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<core::services::grading::GradingWeightsPermille, CliError> {
        use core::services::grading::GradingWeightsPermille;

        let Some(raw) = matches.get_one::<String>("weights") else {
            return Ok(GradingWeightsPermille::default());
        };

        let parts: Vec<&str> = raw.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        if parts.len() != 4 {
            return Err(Self::report_error(
                format,
                "invalid_weights",
                "Weights must have 4 comma-separated numbers",
            ));
        }

        let parse_u16 = |s: &str| -> Result<u16, CliError> {
            s.parse::<u16>().map_err(|_| {
                Self::report_error(
                    format,
                    "invalid_weights",
                    "Weights must be valid integers",
                )
            })
        };

        let a = parse_u16(parts[0])?;
        let b = parse_u16(parts[1])?;
        let c = parse_u16(parts[2])?;
        let d = parse_u16(parts[3])?;

        let sum = u32::from(a)
            .checked_add(u32::from(b))
            .and_then(|v| v.checked_add(u32::from(c)))
            .and_then(|v| v.checked_add(u32::from(d)))
            .ok_or_else(|| {
                Self::report_error(format, "invalid_weights", "Weights sum overflow")
            })?;

        let (process, risk, execution, documentation) = if sum == 100 {
            (
                a.checked_mul(10).ok_or_else(|| {
                    Self::report_error(format, "invalid_weights", "Weight overflow")
                })?,
                b.checked_mul(10).ok_or_else(|| {
                    Self::report_error(format, "invalid_weights", "Weight overflow")
                })?,
                c.checked_mul(10).ok_or_else(|| {
                    Self::report_error(format, "invalid_weights", "Weight overflow")
                })?,
                d.checked_mul(10).ok_or_else(|| {
                    Self::report_error(format, "invalid_weights", "Weight overflow")
                })?,
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
        weights.validate().map_err(|e| {
            Self::report_error(
                format,
                "invalid_weights",
                format!("Invalid weights: {e}"),
            )
        })?;
        Ok(weights)
    }

    fn metrics_advanced(&mut self, sub_matches: &ArgMatches) {
        use crate::views::AdvancedMetricsView;
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

        AdvancedMetricsView::display(all_trades);
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

    fn metrics_compare(&mut self, sub_matches: &ArgMatches) {
        // For now, provide a placeholder implementation
        let _period1 = sub_matches.get_one::<String>("period1");
        let _period2 = sub_matches.get_one::<String>("period2");
        let _account_id = sub_matches.get_one::<String>("account");

        println!("Performance Comparison");
        println!("=====================");
        println!("Feature coming soon: Period-over-period performance analysis");
        println!("This will compare metrics across different time periods.");
        println!();
        println!("Currently working on implementing:");
        println!("  • Time period parsing (last-30-days, previous-30-days, etc.)");
        println!("  • Comparative metric calculations");
        println!("  • Trend analysis and direction indicators");
        println!("  • Export capabilities");
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
