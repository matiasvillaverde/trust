use crate::command::AccountCommandBuilder;
use crate::dialog::AccountDialogBuilder;
use clap::{ArgMatches, Command};
use std::ffi::OsString;
use trust_db_sqlite::SqliteDatabase;
use trust_model::Database;
mod command;
mod dialog;
mod view;

fn main() {
    let matches = Command::new("trust")
        .about("A tool for managing tradings")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommands(AccountCommandBuilder::new().create_account().build())
        .get_matches();

    let dispatcher = ArgDispatcher::new_sqlite();
    dispatcher.dispatch(matches);
}

struct ArgDispatcher {
    database: Box<dyn Database>,
}
impl ArgDispatcher {
    fn new_sqlite() -> Self {
        let database = SqliteDatabase::new("sqlite://test.db");
        ArgDispatcher {
            database: Box::new(database),
        }
    }

    fn dispatch(mut self, matches: ArgMatches) {
        match matches.subcommand() {
            Some(("account", sub_matches)) => match sub_matches.subcommand() {
                Some(("create", _)) => self.create_account(),
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

    fn create_account(&mut self) {
        AccountDialogBuilder::new()
            .name()
            .description()
            .build(&mut self.database)
            .display();
    }
}
