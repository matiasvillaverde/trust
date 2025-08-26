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
use model::TransactionCategory;
use shellexpand::tilde;
use std::ffi::OsString;
use std::fs::{self, create_dir_all};
use std::path::Path;

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

    #[cfg(debug_assertions)]
    fn database_url() -> String {
        tilde("~/.trust/debug.db").to_string()
    }

    #[cfg(not(debug_assertions))]
    fn database_url() -> String {
        tilde("~/.trust/production.db").to_string()
    }

    pub fn dispatch(mut self, matches: ArgMatches) {
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
                Some(("create", _)) => self.create_trading_vehicle(),
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
                Some(("performance", sub_sub_matches)) => self.performance_report(sub_sub_matches),
                Some(("drawdown", sub_sub_matches)) => self.drawdown_report(sub_sub_matches),
                Some(("risk", sub_sub_matches)) => self.risk_report(sub_sub_matches),
                Some(("concentration", sub_sub_matches)) => {
                    self.concentration_report(sub_sub_matches)
                }
                Some(("summary", sub_sub_matches)) => self.summary_report(sub_sub_matches),
                Some(("metrics", sub_sub_matches)) => self.metrics_report(sub_sub_matches),
                _ => unreachable!("No subcommand provided"),
            },
            Some(("metrics", sub_matches)) => match sub_matches.subcommand() {
                Some(("advanced", sub_sub_matches)) => self.metrics_advanced(sub_sub_matches),
                Some(("compare", sub_sub_matches)) => self.metrics_compare(sub_sub_matches),
                _ => unreachable!("No subcommand provided"),
            },
            Some(("level", sub_matches)) => match sub_matches.subcommand() {
                Some(("status", sub_sub_matches)) => self.level_status(sub_sub_matches),
                Some(("history", sub_sub_matches)) => self.level_history(sub_sub_matches),
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
    fn create_trading_vehicle(&mut self) {
        TradingVehicleDialogBuilder::new()
            .category()
            .symbol()
            .broker()
            .isin()
            .build(&mut self.trust)
            .display();
    }

    fn search_trading_vehicle(&mut self) {
        TradingVehicleSearchDialogBuilder::new()
            .search(&mut self.trust)
            .display();
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

    fn drawdown_report(&mut self, sub_matches: &ArgMatches) {
        use crate::views::DrawdownView;
        use core::calculators_drawdown::RealizedDrawdownCalculator;
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

        // Fetch transactions for the account
        let transactions = if let Some(account_id) = account_id {
            match self.trust.get_account_transactions(account_id) {
                Ok(txns) => txns,
                Err(_) => {
                    eprintln!("Unable to retrieve transaction data. Please check your account settings and try again.");
                    return;
                }
            }
        } else {
            // If no account specified, get all transactions for all accounts
            match self.trust.get_all_transactions() {
                Ok(txns) => txns,
                Err(_) => {
                    eprintln!("Unable to retrieve transaction data. Please check your settings and try again.");
                    return;
                }
            }
        };

        // Calculate equity curve
        let curve = match RealizedDrawdownCalculator::calculate_equity_curve(&transactions) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error calculating equity curve: {e}");
                return;
            }
        };

        // Calculate drawdown metrics
        let metrics = match RealizedDrawdownCalculator::calculate_drawdown_metrics(&curve) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Error calculating drawdown metrics: {e}");
                return;
            }
        };

        // Display the results
        DrawdownView::display(metrics);
    }

    fn performance_report(&mut self, sub_matches: &ArgMatches) {
        use crate::views::PerformanceView;
        use core::calculators_performance::PerformanceCalculator;
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

        // Get days filter if provided
        let days_filter = sub_matches.get_one::<u32>("days").copied();

        // Use the new helper method to get all closed trades
        let mut all_trades = match self.trust.search_closed_trades(account_id) {
            Ok(trades) => trades,
            Err(_) => {
                eprintln!("Unable to retrieve trading data. Please check your account settings and try again.");
                return;
            }
        };

        // Apply days filter if specified
        if let Some(days) = days_filter {
            all_trades = PerformanceCalculator::filter_trades_by_days(&all_trades, days);
        }

        PerformanceView::display(all_trades);
    }

    fn risk_report(&mut self, sub_matches: &ArgMatches) {
        use crate::views::RiskView;
        use core::calculators_risk::CapitalAtRiskCalculator;
        use model::Currency;
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

        // If no account specified, require it for now (since we can't query all accounts easily)
        if account_id.is_none() {
            eprintln!("Error: Please specify an account ID using --account");
            return;
        }

        // Calculate open positions
        let positions = match self.trust.calculate_open_positions(account_id) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Error calculating open positions: {e}");
                return;
            }
        };

        // Calculate total capital at risk
        let total_capital_at_risk =
            match CapitalAtRiskCalculator::calculate_total_capital_at_risk(&positions) {
                Ok(total) => total,
                Err(e) => {
                    eprintln!("Error calculating total capital at risk: {e}");
                    return;
                }
            };

        // Get account balance for equity calculation
        let account_balance = if let Some(id) = account_id {
            match self.trust.search_balance(id, &Currency::USD) {
                Ok(balance) => balance.total_balance,
                Err(_) => {
                    eprintln!("Unable to retrieve account balance");
                    return;
                }
            }
        } else {
            rust_decimal_macros::dec!(0)
        };

        // Display the results
        RiskView::display(positions, total_capital_at_risk, account_balance);
    }

    fn concentration_report(&mut self, sub_matches: &ArgMatches) {
        use crate::views::ConcentrationView;
        use core::calculators_concentration::{ConcentrationCalculator, MetadataField};
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
                    eprintln!("Error fetching accounts: {e}");
                    return;
                }
            }
        };

        if all_trades.is_empty() {
            println!("\nNo trades found for the specified criteria.\n");
            return;
        }

        // Filter for open positions if requested
        let trades_to_analyze = if open_only {
            ConcentrationCalculator::filter_open_trades(&all_trades)
        } else {
            all_trades
        };

        if trades_to_analyze.is_empty() {
            println!("\nNo open trades found for the specified criteria.\n");
            return;
        }

        // Analyze concentration by sector
        let sector_analysis =
            ConcentrationCalculator::analyze_by_metadata(&trades_to_analyze, MetadataField::Sector);

        // Analyze concentration by asset class
        let asset_class_analysis = ConcentrationCalculator::analyze_by_metadata(
            &trades_to_analyze,
            MetadataField::AssetClass,
        );

        // Display the results
        ConcentrationView::display(sector_analysis, asset_class_analysis, open_only);
    }

    fn summary_report(&mut self, sub_matches: &ArgMatches) {
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

        // Get trading summary
        let summary = match self.trust.get_trading_summary(account_id) {
            Ok(summary) => summary,
            Err(e) => {
                eprintln!("Error generating trading summary: {e}");
                return;
            }
        };

        // Display comprehensive summary
        println!("Trust Trading Summary");
        println!("==================");
        println!(
            "Account: {} (${:.2} equity)",
            summary.account_id, summary.equity
        );
        println!();

        if let Some(ref performance) = summary.performance {
            println!("Performance:");
            println!(
                "├─ Trades: {} total ({}W, {}L) - {:.1}% win rate",
                performance.total_trades,
                performance.winning_trades,
                performance.losing_trades,
                performance.win_rate
            );
            println!(
                "└─ Avg R-Multiple: +{:.2}R per trade",
                performance.average_r_multiple
            );
        } else {
            println!("Performance: No closed trades available");
        }

        println!();
        println!("Risk Management:");

        if summary.capital_at_risk.is_empty() {
            println!("├─ Capital at Risk: $0.00 (0.0% of account) ✅");
            println!("└─ Open Positions: 0 trades active");
        } else {
            let total_risk: rust_decimal::Decimal = summary
                .capital_at_risk
                .iter()
                .map(|pos| pos.capital_at_risk)
                .sum();
            println!("├─ Capital at Risk: ${:.2} ✅", total_risk);
            println!(
                "└─ Open Positions: {} trades active",
                summary.capital_at_risk.len()
            );
        }

        if summary.concentration.is_empty() {
            println!();
            println!("Concentration: Diversified portfolio ✅");
        } else {
            println!();
            println!(
                "Concentration: Analysis available (run 'trust report concentration' for details)"
            );
        }
    }

    fn metrics_report(&mut self, sub_matches: &ArgMatches) {
        use crate::views::AdvancedMetricsView;
        use core::calculators_performance::PerformanceCalculator;
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

        // Get days filter if provided
        let days_filter = sub_matches.get_one::<u32>("days").copied();

        // Get all closed trades
        let mut all_trades = match self.trust.search_closed_trades(account_id) {
            Ok(trades) => trades,
            Err(_) => {
                eprintln!("Unable to retrieve trading data. Please check your account settings and try again.");
                return;
            }
        };

        // Apply days filter if specified
        if let Some(days) = days_filter {
            all_trades = PerformanceCalculator::filter_trades_by_days(&all_trades, days);
        }

        AdvancedMetricsView::display(all_trades);
    }

    fn level_status(&mut self, sub_matches: &ArgMatches) {
        let account_id = sub_matches.get_one::<String>("account").map(|s| s.as_str());

        match self.facade.get_account_level(account_id) {
            Ok(level) => println!("{}", level),
            Err(e) => eprintln!("Failed to get level status: {}", e),
        }
    }

    fn level_history(&mut self, sub_matches: &ArgMatches) {
        let account_id = sub_matches.get_one::<String>("account").map(|s| s.as_str());
        let days = sub_matches.get_one::<u32>("days").copied().unwrap_or(90);

        match self.facade.get_level_history(account_id, days) {
            Ok(history) => {
                if history.is_empty() {
                    println!("No level changes found");
                } else {
                    println!("Level History (last {} days):", days);
                    println!("{:<20} {:<8} {:<8} {:<30}", "Date", "From", "To", "Reason");
                    println!("{}", "-".repeat(70));
                    for change in history {
                        println!(
                            "{:<20} {:<8} {:<8} {:<30}",
                            change.changed_at.format("%Y-%m-%d %H:%M"),
                            change.old_level,
                            change.new_level,
                            change.change_reason
                        );
                    }
                }
            }
            Err(e) => eprintln!("Failed to get level history: {}", e),
        }
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
            println!("Advanced Trading Metrics (Last {} days)", days_filter);
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

        AdvancedMetricsView::display(all_trades);
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
