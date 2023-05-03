use crate::commands::account_command::AccountCommandBuilder;
use crate::commands::transaction_command::TransactionCommandBuilder;
use crate::dispatcher::ArgDispatcher;
use clap::Command;
mod commands;
mod dialogs;
mod dispatcher;
mod views;

fn main() {
    let matches = Command::new("trust")
        .about("A tool for managing tradings")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            AccountCommandBuilder::new()
                .create_account()
                .read_account()
                .build(),
        )
        .subcommand(
            TransactionCommandBuilder::new()
                .deposit()
                .withdraw()
                .build(),
        )
        .get_matches();

    let dispatcher = ArgDispatcher::new_sqlite();
    dispatcher.dispatch(matches);
}
