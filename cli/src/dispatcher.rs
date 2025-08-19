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

    fn performance_report(&mut self, sub_matches: &ArgMatches) {
        use crate::views::PerformanceView;
        use model::trade::Status;
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

        // Note: Days filtering is supported in the CLI but not yet implemented
        // Future enhancement: Filter trades by date range based on --days parameter
        let mut all_trades = Vec::new();

        if let Some(account_id) = account_id {
            // Get trades for specific account
            match self.trust.search_trades(account_id, Status::ClosedTarget) {
                Ok(mut trades) => all_trades.append(&mut trades),
                Err(e) => {
                    eprintln!("Error retrieving closed target trades: {e}");
                    return;
                }
            }

            match self.trust.search_trades(account_id, Status::ClosedStopLoss) {
                Ok(mut trades) => all_trades.append(&mut trades),
                Err(e) => {
                    eprintln!("Error retrieving closed stop loss trades: {e}");
                    return;
                }
            }
        } else {
            // Get all accounts first, then get their trades
            let accounts = match self.trust.search_all_accounts() {
                Ok(accounts) => accounts,
                Err(e) => {
                    eprintln!("Error retrieving accounts: {e}");
                    return;
                }
            };

            for account in accounts {
                // Get closed target trades
                if let Ok(mut trades) = self.trust.search_trades(account.id, Status::ClosedTarget) {
                    all_trades.append(&mut trades);
                }

                // Get closed stop loss trades
                if let Ok(mut trades) = self.trust.search_trades(account.id, Status::ClosedStopLoss)
                {
                    all_trades.append(&mut trades);
                }
            }
        }

        PerformanceView::display(all_trades);
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
