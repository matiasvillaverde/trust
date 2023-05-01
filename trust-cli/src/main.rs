use crate::command::AccountCommandBuilder;
use crate::dispatcher::ArgDispatcher;
use clap::Command;
mod command;
mod dialog;
mod dispatcher;
mod view;

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
        .get_matches();

    let dispatcher = ArgDispatcher::new_sqlite();
    dispatcher.dispatch(matches);
}
