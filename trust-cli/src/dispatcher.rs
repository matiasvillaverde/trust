use crate::dialogs::account_dialog::{AccountDialogBuilder, AccountSearchDialog};
use crate::dialogs::transaction_dialog::TransactionDialogBuilder;
use clap::ArgMatches;
use std::ffi::OsString;
use trust_core::Trust;
use trust_db_sqlite::SqliteDatabase;
use trust_model::TransactionCategory;

pub struct ArgDispatcher {
    trust: Trust,
}

impl ArgDispatcher {
    pub fn new_sqlite() -> Self {
        let database = SqliteDatabase::new("sqlite://production.db");
        ArgDispatcher {
            trust: Trust::new(Box::new(database)),
        }
    }

    pub fn dispatch(mut self, matches: ArgMatches) {
        match matches.subcommand() {
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
