use crate::dialogs::{
    AccountDialogBuilder, AccountSearchDialog, CancelDialogBuilder, CloseDialogBuilder,
    ExitDialogBuilder, FillTradeDialogBuilder, FundingDialogBuilder, KeysDeleteDialogBuilder,
    KeysReadDialogBuilder, KeysWriteDialogBuilder, ModifyDialogBuilder, SubmitDialogBuilder,
    SyncTradeDialogBuilder, TradeDialogBuilder, TradeSearchDialogBuilder,
    TradingVehicleDialogBuilder, TradingVehicleSearchDialogBuilder, TransactionDialogBuilder,
};
use crate::dialogs::{RuleDialogBuilder, RuleRemoveDialogBuilder};
use crate::protected_keyword;
use alpaca_broker::AlpacaBroker;
use clap::ArgMatches;
use core::services::leveling::{
    LevelCriterionProgress, LevelEvaluationOutcome, LevelPathProgress, LevelPerformanceSnapshot,
    LevelProgressReport,
};
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    database::TradingVehicleUpsert, Currency, Level, LevelAdjustmentRules, LevelTrigger, Trade,
    TradingVehicleCategory, TransactionCategory,
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
    fn is_valid_protected_keyword(expected: &str, provided: &str) -> bool {
        provided == expected
    }

    pub fn new_sqlite() -> Self {
        create_dir_if_necessary();
        let database = SqliteDatabase::new(ArgDispatcher::database_url().as_str());
        let mut trust = TrustFacade::new(Box::new(database), Box::<AlpacaBroker>::default());
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

    #[allow(clippy::too_many_lines)]
    pub fn dispatch(mut self, matches: ArgMatches) -> Result<(), CliError> {
        match matches.subcommand() {
            Some(("keys", sub_matches)) => match sub_matches.subcommand() {
                Some(("create", sub_sub_matches)) => self.create_keys(sub_sub_matches)?,
                Some(("show", _)) => self.show_keys(),
                Some(("delete", sub_sub_matches)) => self.delete_keys(sub_sub_matches)?,
                Some(("protected-set", sub_sub_matches)) => {
                    self.set_protected_keyword(sub_sub_matches)?
                }
                Some(("protected-show", _)) => self.show_protected_keyword(),
                Some(("protected-delete", sub_sub_matches)) => {
                    self.delete_protected_keyword(sub_sub_matches)?
                }
                _ => unreachable!("No subcommand provided"),
            },
            Some(("account", sub_matches)) => match sub_matches.subcommand() {
                Some(("create", sub_sub_matches)) => self.create_account(sub_sub_matches)?,
                Some(("search", _)) => self.search_account(),
                _ => unreachable!("No subcommand provided"),
            },
            Some(("transaction", sub_matches)) => match sub_matches.subcommand() {
                Some(("deposit", sub_sub_matches)) => self.deposit(sub_sub_matches)?,
                Some(("withdraw", sub_sub_matches)) => self.withdraw(sub_sub_matches)?,
                _ => unreachable!("No subcommand provided"),
            },
            Some(("rule", sub_matches)) => match sub_matches.subcommand() {
                Some(("create", sub_sub_matches)) => {
                    self.create_rule(sub_sub_matches, Self::parse_report_format(sub_matches))?
                }
                Some(("remove", sub_sub_matches)) => {
                    self.remove_rule(sub_sub_matches, Self::parse_report_format(sub_matches))?
                }
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
                Some(("watch", sub_sub_matches)) => self.watch_trade(sub_sub_matches)?,
                Some(("search", _)) => self.search_trade(),
                Some(("modify-stop", _)) => self.modify_stop(),
                Some(("modify-target", _)) => self.modify_target(),
                Some(("size-preview", sub_sub_matches)) => self.trade_size_preview(
                    sub_sub_matches,
                    Self::parse_report_format(sub_sub_matches),
                )?,
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
            Some(("level", sub_matches)) => {
                match sub_matches.subcommand() {
                    Some(("status", sub_sub_matches)) => {
                        self.level_status(sub_sub_matches, Self::parse_report_format(sub_matches))?
                    }
                    Some(("triggers", sub_sub_matches)) => self
                        .level_triggers(sub_sub_matches, Self::parse_report_format(sub_matches))?,
                    Some(("history", sub_sub_matches)) => {
                        self.level_history(sub_sub_matches, Self::parse_report_format(sub_matches))?
                    }
                    Some(("change", sub_sub_matches)) => {
                        self.level_change(sub_sub_matches, Self::parse_report_format(sub_matches))?
                    }
                    Some(("evaluate", sub_sub_matches)) => self
                        .level_evaluate(sub_sub_matches, Self::parse_report_format(sub_matches))?,
                    Some(("progress", sub_sub_matches)) => self
                        .level_progress(sub_sub_matches, Self::parse_report_format(sub_matches))?,
                    Some(("rules", sub_sub_matches)) => {
                        match sub_sub_matches.subcommand() {
                            Some(("show", nested)) => self
                                .level_rules_show(nested, Self::parse_report_format(sub_matches))?,
                            Some(("set", nested)) => self
                                .level_rules_set(nested, Self::parse_report_format(sub_matches))?,
                            _ => unreachable!("No subcommand provided"),
                        }
                    }
                    _ => unreachable!("No subcommand provided"),
                }
            }
            Some(("onboarding", sub_matches)) => match sub_matches.subcommand() {
                Some(("init", sub_sub_matches)) => {
                    self.onboarding_init(sub_sub_matches, Self::parse_report_format(sub_matches))?
                }
                Some(("status", _)) => {
                    self.onboarding_status(Self::parse_report_format(sub_matches))?
                }
                _ => unreachable!("No subcommand provided"),
            },
            Some(("policy", sub_matches)) => {
                self.policy_show(Self::parse_report_format(sub_matches))?
            }
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

        self.trust.authorize_protected_mutation();
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

        if accounts.len() == 1 {
            let account = accounts.first().ok_or_else(|| {
                Self::report_error(format, "accounts_unavailable", "No accounts available")
            })?;
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
    fn create_account(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        self.ensure_protected_keyword(sub_matches, ReportOutputFormat::Text, "account creation")?;
        AccountDialogBuilder::new()
            .name()
            .description()
            .environment()
            .tax_percentage()
            .earnings_percentage()
            .build(&mut self.trust)
            .display();
        Ok(())
    }

    fn search_account(&mut self) {
        AccountSearchDialog::new()
            .search(&mut self.trust)
            .display(&mut self.trust);
    }
}

// Transaction
impl ArgDispatcher {
    fn deposit(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        self.ensure_protected_keyword(sub_matches, ReportOutputFormat::Text, "deposit")?;
        TransactionDialogBuilder::new(TransactionCategory::Deposit)
            .account(&mut self.trust)
            .currency(&mut self.trust)
            .amount(&mut self.trust)
            .build(&mut self.trust)
            .display();
        Ok(())
    }

    fn withdraw(&mut self, sub_matches: &ArgMatches) -> Result<(), CliError> {
        self.ensure_protected_keyword(sub_matches, ReportOutputFormat::Text, "withdrawal")?;
        TransactionDialogBuilder::new(TransactionCategory::Withdrawal)
            .account(&mut self.trust)
            .currency(&mut self.trust)
            .amount(&mut self.trust)
            .build(&mut self.trust)
            .display();
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

    #[allow(clippy::too_many_lines)]
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
    #[allow(clippy::too_many_lines)]
    fn trade_size_preview(
        &mut self,
        sub_matches: &ArgMatches,
        format: ReportOutputFormat,
    ) -> Result<(), CliError> {
        let account_id = self.resolve_level_account_id(sub_matches, format)?;
        let entry_price = Self::parse_decimal_arg(sub_matches, "entry", format)?;
        let stop_price = Self::parse_decimal_arg(sub_matches, "stop", format)?;
        let currency_raw = sub_matches
            .get_one::<String>("currency")
            .map(String::as_str)
            .unwrap_or("usd");
        let currency = Currency::from_str(&currency_raw.to_ascii_uppercase()).map_err(|_| {
            Self::report_error(
                format,
                "invalid_currency",
                format!("Unsupported currency '{currency_raw}'. Use USD, EUR, or BTC."),
            )
        })?;

        let sizing = self
            .trust
            .calculate_level_adjusted_quantity(account_id, entry_price, stop_price, &currency)
            .map_err(|error| {
                Self::report_error(
                    format,
                    "size_preview_failed",
                    format!("Unable to calculate level-adjusted quantity: {error}"),
                )
            })?;

        let current_level = self.trust.level_for_account(account_id).map_err(|error| {
            Self::report_error(
                format,
                "size_preview_level_load_failed",
                format!("Unable to read account level: {error}"),
            )
        })?;

        let risk_per_share = entry_price
            .checked_sub(stop_price)
            .map(|value| value.abs())
            .unwrap_or(Decimal::ZERO);

        let levels: Vec<Value> = (0_u8..=4_u8)
            .map(|level| {
                let multiplier = Level::multiplier_for_level(level).unwrap_or(Decimal::ZERO);
                let quantity = Decimal::from(sizing.base_quantity)
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
                    "current": level == current_level.current_level,
                })
            })
            .collect();

        match format {
            ReportOutputFormat::Text => {
                println!("Position Size Preview");
                println!("====================");
                println!("Account: {account_id}");
                println!(
                    "Entry: {} {} | Stop: {} {} | Risk/Share: {} {}",
                    Self::decimal_string(entry_price),
                    currency,
                    Self::decimal_string(stop_price),
                    currency,
                    Self::decimal_string(risk_per_share),
                    currency
                );
                println!(
                    "Base quantity (rules only): {} | Current level-adjusted: {} ({}x)",
                    sizing.base_quantity,
                    sizing.final_quantity,
                    Self::decimal_string(sizing.level_multiplier)
                );
                println!();
                println!("Level ladder:");
                for item in &levels {
                    let current_marker = if item["current"].as_bool().unwrap_or(false) {
                        "  <- current"
                    } else {
                        ""
                    };
                    println!(
                        "L{} ({}x): qty {} | risk {} {}{}",
                        item["level"].as_u64().unwrap_or(0),
                        item["multiplier"].as_str().unwrap_or("0"),
                        item["quantity"].as_i64().unwrap_or(0),
                        item["risk_amount"].as_str().unwrap_or("0"),
                        currency,
                        current_marker
                    );
                }
            }
            ReportOutputFormat::Json => {
                let payload = json!({
                    "report": "trade_size_preview",
                    "format_version": 1,
                    "generated_at": chrono::Utc::now().to_rfc3339(),
                    "scope": {
                        "account_id": account_id.to_string(),
                    },
                    "data": {
                        "currency": currency.to_string(),
                        "entry_price": Self::decimal_string(entry_price),
                        "stop_price": Self::decimal_string(stop_price),
                        "risk_per_share": Self::decimal_string(risk_per_share),
                        "base_quantity": sizing.base_quantity,
                        "current_level": current_level.current_level,
                        "current_multiplier": Self::decimal_string(sizing.level_multiplier),
                        "current_quantity": sizing.final_quantity,
                        "levels": levels,
                    }
                });
                Self::print_json(&payload)?;
            }
        }

        Ok(())
    }

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

    fn watch_trade(&mut self, matches: &ArgMatches) -> Result<(), CliError> {
        use model::{Status as TradeStatus, WatchControl, WatchOptions};
        use std::time::Duration;

        let account = if let Some(name) = matches.get_one::<String>("account") {
            self.trust
                .search_account(name)
                .map_err(|e| CliError::new("watch_account_not_found", format!("{e}")))?
        } else {
            AccountSearchDialog::new()
                .search(&mut self.trust)
                .build()
                .map_err(|e| CliError::new("watch_account_select_failed", format!("{e}")))?
        };

        let trade = if let Some(id) = matches.get_one::<String>("trade-id") {
            let trade_id = Uuid::from_str(id)
                .map_err(|e| CliError::new("watch_invalid_trade_id", format!("{e}")))?;
            self.trust
                .read_trade(trade_id)
                .map_err(|e| CliError::new("watch_trade_read_failed", format!("{e}")))?
        } else {
            // Fallback: watch first open-ish trade for that account.
            let mut trades = self
                .trust
                .search_trades(account.id, TradeStatus::Submitted)
                .unwrap_or_default();
            trades.append(
                &mut self
                    .trust
                    .search_trades(account.id, TradeStatus::PartiallyFilled)
                    .unwrap_or_default(),
            );
            trades.append(
                &mut self
                    .trust
                    .search_trades(account.id, TradeStatus::Filled)
                    .unwrap_or_default(),
            );

            trades
                .into_iter()
                .next()
                .ok_or_else(|| CliError::new("watch_no_trade", "No trade found to watch"))?
        };

        let reconcile_secs = matches
            .get_one::<String>("reconcile-secs")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(20);

        let timeout_secs = matches
            .get_one::<String>("timeout-secs")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let options = WatchOptions {
            reconcile_every: Duration::from_secs(reconcile_secs),
            timeout: if timeout_secs == 0 {
                None
            } else {
                Some(Duration::from_secs(timeout_secs))
            },
        };

        let json_mode = matches.get_flag("json");
        let mut last_market: Option<(String, String)> = None;

        self.trust
            .watch_trade(&trade, &account, options, |trade_now, evt| {
                if let (Some(price), Some(ts)) = (evt.market_price, evt.market_timestamp) {
                    last_market = Some((price.to_string(), ts.to_rfc3339()));
                }

                if json_mode {
                    let out = json!({
                        "type": "trade_watch_tick",
                        "trade_id": trade_now.id.to_string(),
                        "trade_status": trade_now.status.to_string(),
                        "broker_source": evt.broker_source.clone(),
                        "broker_stream": evt.broker_stream.clone(),
                        "broker_event_type": evt.broker_event_type,
                        "broker_order_id": evt.broker_order_id.map(|x| x.to_string()),
                        "market": last_market.as_ref().map(|(price, ts)| json!({
                            "symbol": evt.market_symbol.clone().unwrap_or_else(|| trade_now.trading_vehicle.symbol.clone()),
                            "price": price,
                            "timestamp": ts,
                        })),
                        "updated_orders": evt.updated_orders.iter().map(|o| json!({
                            "id": o.id.to_string(),
                            "broker_order_id": o.broker_order_id.map(|x| x.to_string()),
                            "status": o.status.to_string(),
                            "filled_quantity": o.filled_quantity,
                            "average_filled_price": o.average_filled_price.map(|x| x.to_string()),
                        })).collect::<Vec<_>>(),
                    });
                    println!("{out}");
                } else {
                    // GH-like: refresh screen (best-effort).
                    print!("\u{001b}[2J\u{001b}[H");
                    println!("trust trade watch");
                    println!("Trade:   {}", trade_now.id);
                    println!("Account: {}", account.name);
                    println!("Symbol:  {}", trade_now.trading_vehicle.symbol);
                    println!("Status:  {}", trade_now.status);
                    if let Some((price, ts)) = &last_market {
                        println!("Last:    {} @ {}", price, ts);
                    }
                    println!();
                    println!(
                        "Entry:  {} avg_fill={:?} filled_qty={:?}",
                        trade_now.entry.status,
                        trade_now.entry.average_filled_price,
                        trade_now.entry.filled_quantity
                    );
                    println!(
                        "Stop:   {} avg_fill={:?} filled_qty={:?}",
                        trade_now.safety_stop.status,
                        trade_now.safety_stop.average_filled_price,
                        trade_now.safety_stop.filled_quantity
                    );
                    println!(
                        "Target: {} avg_fill={:?} filled_qty={:?}",
                        trade_now.target.status,
                        trade_now.target.average_filled_price,
                        trade_now.target.filled_quantity
                    );
                    println!();
                    println!(
                        "Last broker event: {} {:?}",
                        evt.broker_event_type, evt.broker_order_id
                    );
                }

                Ok(WatchControl::Continue)
            })
            .map_err(|e| CliError::new("watch_failed", format!("{e}")))?;

        Ok(())
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
        let exposure =
            AdvancedMetricsCalculator::calculate_exposure_metrics(&open_trades_for_scope);
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
                total
                    .checked_div(Decimal::from(closed_trades.len()))
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
            let mut txs = if let Some(id) = account_id {
                self.trust
                    .get_account_transactions(id)
                    .unwrap_or_else(|_| Vec::new())
            } else {
                self.trust
                    .get_all_transactions()
                    .unwrap_or_else(|_| Vec::new())
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
        if parts.len() != 4 {
            return Err(Self::report_error(
                format,
                "invalid_weights",
                "Weights must have 4 comma-separated numbers",
            ));
        }

        let parse_u16 = |s: &str| -> Result<u16, CliError> {
            s.parse::<u16>().map_err(|_| {
                Self::report_error(format, "invalid_weights", "Weights must be valid integers")
            })
        };

        let a = parse_u16(parts.first().ok_or_else(|| {
            Self::report_error(
                format,
                "invalid_weights",
                "Weights must have 4 comma-separated numbers",
            )
        })?)?;
        let b = parse_u16(parts.get(1).ok_or_else(|| {
            Self::report_error(
                format,
                "invalid_weights",
                "Weights must have 4 comma-separated numbers",
            )
        })?)?;
        let c = parse_u16(parts.get(2).ok_or_else(|| {
            Self::report_error(
                format,
                "invalid_weights",
                "Weights must have 4 comma-separated numbers",
            )
        })?)?;
        let d = parse_u16(parts.get(3).ok_or_else(|| {
            Self::report_error(
                format,
                "invalid_weights",
                "Weights must have 4 comma-separated numbers",
            )
        })?)?;

        let sum = u32::from(a)
            .checked_add(u32::from(b))
            .and_then(|v| v.checked_add(u32::from(c)))
            .and_then(|v| v.checked_add(u32::from(d)))
            .ok_or_else(|| Self::report_error(format, "invalid_weights", "Weights sum overflow"))?;

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
            Self::report_error(format, "invalid_weights", format!("Invalid weights: {e}"))
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

#[cfg(test)]
mod tests {
    use super::ArgDispatcher;

    #[test]
    fn test_protected_keyword_validator() {
        assert!(ArgDispatcher::is_valid_protected_keyword("abc", "abc"));
        assert!(!ArgDispatcher::is_valid_protected_keyword("abc", "abcd"));
        assert!(!ArgDispatcher::is_valid_protected_keyword("abc", "ABC"));
    }
}
