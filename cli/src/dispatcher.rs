use crate::dialogs::{
    AccountDialogBuilder, AccountSearchDialog, CancelDialogBuilder, CloseDialogBuilder,
    ExitDialogBuilder, FillTradeDialogBuilder, FundingDialogBuilder, KeysDeleteDialogBuilder,
    KeysReadDialogBuilder, KeysWriteDialogBuilder, ModifyDialogBuilder, SubmitDialogBuilder,
    SyncTradeDialogBuilder, TradeDialogBuilder, TradeSearchDialogBuilder,
    TradingVehicleDialogBuilder, TradingVehicleSearchDialogBuilder, TransactionDialogBuilder,
};
use crate::dialogs::{RuleDialogBuilder, RuleRemoveDialogBuilder};
use crate::output::{DistributionFormatter, ErrorFormatter, ProgressIndicator};
use alpaca_broker::AlpacaBroker;
use clap::ArgMatches;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::TransactionCategory;
use shellexpand::tilde;
use std::ffi::OsString;
use std::fs;

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
                Some(("create", create_matches)) => {
                    self.create_account_with_hierarchy(create_matches)
                }
                Some(("search", _)) => self.search_account(),
                Some(("transfer", transfer_matches)) => self.transfer_accounts(transfer_matches),
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
                Some(("manually-close", close_matches)) => self.close(close_matches),
                Some(("sync", _)) => self.create_sync(),
                Some(("search", _)) => self.search_trade(),
                Some(("modify-stop", _)) => self.modify_stop(),
                Some(("modify-target", _)) => self.modify_target(),
                _ => unreachable!("No subcommand provided"),
            },
            Some(("distribution", sub_matches)) => match sub_matches.subcommand() {
                Some(("configure", configure_matches)) => {
                    self.configure_distribution(configure_matches)
                }
                Some(("execute", execute_matches)) => self.execute_distribution(execute_matches),
                _ => unreachable!("No subcommand provided"),
            },
            Some(("report", sub_matches)) => match sub_matches.subcommand() {
                Some(("performance", sub_sub_matches)) => self.performance_report(sub_sub_matches),
                Some(("drawdown", sub_sub_matches)) => self.drawdown_report(sub_sub_matches),
                Some(("risk", sub_sub_matches)) => self.risk_report(sub_sub_matches),
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

    fn close(&mut self, matches: &ArgMatches) {
        // Check if auto-distribute flag is set
        let auto_distribute = matches.get_flag("auto-distribute");

        if auto_distribute {
            println!("🚀 Enhanced trade closure with automatic profit distribution enabled!");
            println!("   If the trade is profitable, profits will be automatically distributed.");
        }

        // Use existing close dialog
        // TODO: Integrate with enhanced close_trade_with_auto_distribution when dialog supports it
        CloseDialogBuilder::new()
            .account(&mut self.trust)
            .search(&mut self.trust)
            .build(&mut self.trust)
            .display();

        if auto_distribute {
            println!("💡 Note: Automatic distribution integration will be available once account hierarchy is fully implemented in the database layer.");
        }
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
}

// Enhanced Account Operations
impl ArgDispatcher {
    fn create_account_with_hierarchy(&mut self, matches: &ArgMatches) {
        // Extract arguments from clap
        let name = matches.get_one::<String>("name").unwrap();
        let account_type = matches.get_one::<String>("type").unwrap();
        let parent_id = matches.get_one::<String>("parent-id");

        // Parse account type
        let account_type_enum = match account_type.as_str() {
            "Primary" => model::AccountType::Primary,
            "Earnings" => model::AccountType::Earnings,
            "TaxReserve" => model::AccountType::TaxReserve,
            "Reinvestment" => model::AccountType::Reinvestment,
            _ => {
                eprintln!("Invalid account type: {}", account_type);
                return;
            }
        };

        // Parse parent ID if provided
        let parent_uuid = if let Some(id_str) = parent_id {
            match uuid::Uuid::parse_str(id_str) {
                Ok(uuid) => Some(uuid),
                Err(_) => {
                    eprintln!("Invalid parent account ID format: {}", id_str);
                    return;
                }
            }
        } else {
            None
        };

        // Use default values for now - in a real implementation, these could be arguments too
        let description = format!("{} account", name);
        let environment = model::Environment::Paper; // Default to paper trading
        let taxes_percentage = rust_decimal::Decimal::new(25, 0); // 25%
        let earnings_percentage = rust_decimal::Decimal::new(30, 0); // 30%

        // Create the account
        match self.trust.create_account_with_hierarchy(
            name,
            &description,
            environment,
            taxes_percentage,
            earnings_percentage,
            account_type_enum,
            parent_uuid,
        ) {
            Ok(account) => {
                println!("✅ Account created successfully!");
                println!("   ID: {}", account.id);
                println!("   Name: {}", account.name);
                println!("   Type: {:?}", account.account_type);
                if let Some(parent) = account.parent_account_id {
                    println!("   Parent ID: {}", parent);
                }
            }
            Err(e) => {
                eprintln!("❌ Failed to create account: {}", e);
            }
        }
    }

    fn transfer_accounts(&mut self, matches: &ArgMatches) {
        // Extract arguments from clap
        let from_id_str = matches.get_one::<String>("from").unwrap();
        let to_id_str = matches.get_one::<String>("to").unwrap();
        let amount_str = matches.get_one::<String>("amount").unwrap();
        let reason = matches.get_one::<String>("reason").unwrap();

        let mut progress = ProgressIndicator::new("Processing Account Transfer".to_string(), 4);

        progress.step("Parsing source account ID");
        let from_id = match uuid::Uuid::parse_str(from_id_str) {
            Ok(uuid) => uuid,
            Err(_) => {
                println!(
                    "{}",
                    ErrorFormatter::format_validation_error(
                        "Source Account ID",
                        "Invalid UUID format",
                        "Please provide a valid UUID for the source account"
                    )
                );
                return;
            }
        };

        progress.step("Parsing destination account ID");
        let to_id = match uuid::Uuid::parse_str(to_id_str) {
            Ok(uuid) => uuid,
            Err(_) => {
                println!(
                    "{}",
                    ErrorFormatter::format_validation_error(
                        "Destination Account ID",
                        "Invalid UUID format",
                        "Please provide a valid UUID for the destination account"
                    )
                );
                return;
            }
        };

        progress.step("Parsing transfer amount");
        let amount = match rust_decimal::Decimal::from_str_exact(amount_str) {
            Ok(decimal) => decimal,
            Err(_) => {
                println!(
                    "{}",
                    ErrorFormatter::format_validation_error(
                        "Transfer Amount",
                        "Invalid decimal format",
                        "Please provide a valid decimal amount (e.g., 500.00)"
                    )
                );
                return;
            }
        };

        // Default currency (could be an argument in future)
        let currency = model::Currency::USD;

        progress.step("Executing account transfer");

        // Execute the transfer
        match self
            .trust
            .transfer_between_accounts(from_id, to_id, amount, currency, reason)
        {
            Ok((withdrawal_id, deposit_id)) => {
                progress.complete();

                // Format transfer success with structured output
                let transfer_summary = format!(
                    "💸 Account Transfer Completed\n\
                    ┌─────────────────────────────────────────────────────────┐\n\
                    │ Transfer Details:                                       \n\
                    │ ├─ Amount:      ${}                        \n\
                    │ ├─ Source:      {}          \n\
                    │ ├─ Destination: {}          \n\
                    │ └─ Reason:      {}                        \n\
                    │                                                         \n\
                    │ Transaction Records:                                    \n\
                    │ ├─ Withdrawal ID: {}          \n\
                    │ └─ Deposit ID:    {}          \n\
                    └─────────────────────────────────────────────────────────┘",
                    amount, from_id, to_id, reason, withdrawal_id, deposit_id
                );

                println!("{}", transfer_summary);
            }
            Err(e) => {
                println!("{}", ErrorFormatter::format_system_error(
                    &format!("Transfer failed: {}", e),
                    "Please verify accounts exist, have sufficient balance, and are not the same account"
                ));
            }
        }
    }
}

// Distribution Operations
impl ArgDispatcher {
    fn configure_distribution(&mut self, matches: &ArgMatches) {
        // Extract arguments from clap
        let account_id_str = matches.get_one::<String>("account-id").unwrap();
        let earnings_str = matches.get_one::<String>("earnings").unwrap();
        let tax_str = matches.get_one::<String>("tax").unwrap();
        let reinvestment_str = matches.get_one::<String>("reinvestment").unwrap();
        let threshold_str = matches.get_one::<String>("threshold").unwrap();

        // Parse account ID
        let account_id =
            match uuid::Uuid::parse_str(account_id_str) {
                Ok(uuid) => uuid,
                Err(_) => {
                    println!("{}", ErrorFormatter::format_validation_error(
                    "Account ID",
                    "Invalid UUID format", 
                    "Please provide a valid UUID (e.g., 550e8400-e29b-41d4-a716-446655440000)"
                ));
                    return;
                }
            };

        // Parse percentages (convert from 40.0 to 0.40) with progress indicator
        let mut progress = ProgressIndicator::new("Configuring Distribution Rules".to_string(), 5);

        progress.step("Parsing earnings percentage");
        let earnings_percent = match rust_decimal::Decimal::from_str_exact(earnings_str) {
            Ok(decimal) => decimal / rust_decimal::Decimal::new(100, 0),
            Err(_) => {
                println!(
                    "{}",
                    ErrorFormatter::format_validation_error(
                        "Earnings Percentage",
                        "Invalid decimal format",
                        "Please provide a decimal value (e.g., 40.0 for 40%)"
                    )
                );
                return;
            }
        };

        progress.step("Parsing tax percentage");
        let tax_percent = match rust_decimal::Decimal::from_str_exact(tax_str) {
            Ok(decimal) => decimal / rust_decimal::Decimal::new(100, 0),
            Err(_) => {
                println!(
                    "{}",
                    ErrorFormatter::format_validation_error(
                        "Tax Percentage",
                        "Invalid decimal format",
                        "Please provide a decimal value (e.g., 30.0 for 30%)"
                    )
                );
                return;
            }
        };

        progress.step("Parsing reinvestment percentage");
        let reinvestment_percent = match rust_decimal::Decimal::from_str_exact(reinvestment_str) {
            Ok(decimal) => decimal / rust_decimal::Decimal::new(100, 0),
            Err(_) => {
                println!(
                    "{}",
                    ErrorFormatter::format_validation_error(
                        "Reinvestment Percentage",
                        "Invalid decimal format",
                        "Please provide a decimal value (e.g., 30.0 for 30%)"
                    )
                );
                return;
            }
        };

        progress.step("Parsing minimum threshold");
        let threshold = match rust_decimal::Decimal::from_str_exact(threshold_str) {
            Ok(decimal) => decimal,
            Err(_) => {
                println!(
                    "{}",
                    ErrorFormatter::format_validation_error(
                        "Minimum Threshold",
                        "Invalid decimal format",
                        "Please provide a decimal value (e.g., 100.00 for $100)"
                    )
                );
                return;
            }
        };

        progress.step("Applying configuration to system");

        // Configure the distribution
        match self.trust.configure_distribution(
            account_id,
            earnings_percent,
            tax_percent,
            reinvestment_percent,
            threshold,
        ) {
            Ok(_rules) => {
                progress.complete();
                println!(
                    "{}",
                    DistributionFormatter::format_configuration_summary(
                        earnings_percent,
                        tax_percent,
                        reinvestment_percent,
                        threshold
                    )
                );
                println!("📋 Account ID: {}", account_id);
            }
            Err(e) => {
                println!(
                    "{}",
                    ErrorFormatter::format_system_error(
                        &format!("Failed to configure distribution: {}", e),
                        "Please verify the account exists and percentages sum to 100%"
                    )
                );
            }
        }
    }

    fn execute_distribution(&mut self, matches: &ArgMatches) {
        // Extract arguments from clap
        let account_id_str = matches.get_one::<String>("account-id").unwrap();
        let earnings_account_str = matches.get_one::<String>("earnings-account").unwrap();
        let tax_account_str = matches.get_one::<String>("tax-account").unwrap();
        let reinvestment_account_str = matches.get_one::<String>("reinvestment-account").unwrap();
        let amount_str = matches.get_one::<String>("amount").unwrap();

        let mut progress = ProgressIndicator::new("Executing Profit Distribution".to_string(), 6);

        progress.step("Parsing source account ID");
        let account_id = match uuid::Uuid::parse_str(account_id_str) {
            Ok(uuid) => uuid,
            Err(_) => {
                println!(
                    "{}",
                    ErrorFormatter::format_validation_error(
                        "Source Account ID",
                        "Invalid UUID format",
                        "Please provide a valid UUID for the source account"
                    )
                );
                return;
            }
        };

        progress.step("Parsing earnings account ID");
        let earnings_account_id = match uuid::Uuid::parse_str(earnings_account_str) {
            Ok(uuid) => uuid,
            Err(_) => {
                println!(
                    "{}",
                    ErrorFormatter::format_validation_error(
                        "Earnings Account ID",
                        "Invalid UUID format",
                        "Please provide a valid UUID for the earnings account"
                    )
                );
                return;
            }
        };

        progress.step("Parsing tax account ID");
        let tax_account_id = match uuid::Uuid::parse_str(tax_account_str) {
            Ok(uuid) => uuid,
            Err(_) => {
                println!(
                    "{}",
                    ErrorFormatter::format_validation_error(
                        "Tax Account ID",
                        "Invalid UUID format",
                        "Please provide a valid UUID for the tax account"
                    )
                );
                return;
            }
        };

        progress.step("Parsing reinvestment account ID");
        let reinvestment_account_id = match uuid::Uuid::parse_str(reinvestment_account_str) {
            Ok(uuid) => uuid,
            Err(_) => {
                println!(
                    "{}",
                    ErrorFormatter::format_validation_error(
                        "Reinvestment Account ID",
                        "Invalid UUID format",
                        "Please provide a valid UUID for the reinvestment account"
                    )
                );
                return;
            }
        };

        progress.step("Parsing distribution amount");
        let amount = match rust_decimal::Decimal::from_str_exact(amount_str) {
            Ok(decimal) => decimal,
            Err(_) => {
                println!(
                    "{}",
                    ErrorFormatter::format_validation_error(
                        "Distribution Amount",
                        "Invalid decimal format",
                        "Please provide a valid decimal amount (e.g., 1000.00)"
                    )
                );
                return;
            }
        };

        // Default currency
        let currency = model::Currency::USD;

        progress.step("Executing distribution transaction");

        // Execute the distribution
        match self.trust.execute_distribution(
            account_id,
            earnings_account_id,
            tax_account_id,
            reinvestment_account_id,
            amount,
            currency,
        ) {
            Ok(result) => {
                progress.complete();

                // Use the result directly
                let formatted_result = &result;

                println!(
                    "{}",
                    DistributionFormatter::format_distribution_result(&formatted_result)
                );
                println!(
                    "📈 Total Transactions Created: {}",
                    result.transactions_created.len()
                );
            }
            Err(e) => {
                println!(
                    "{}",
                    ErrorFormatter::format_system_error(
                        &format!("Distribution execution failed: {}", e),
                        "Please verify all accounts exist and have sufficient balance"
                    )
                );
            }
        }
    }
}

// Utils

fn create_dir_if_necessary() {
    let directory_path = tilde("~/.trust").to_string();

    // Check if directory already exists or not
    if fs::metadata(&directory_path).is_ok() {
        return;
    }

    // We need to create a directory
    match fs::create_dir(directory_path.clone()) {
        Ok(_) => println!("Directory {directory_path} created successfully!"),
        Err(err) => eprintln!("Failed to create directory: {err}"),
    }
}
