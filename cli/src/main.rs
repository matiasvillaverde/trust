use crate::commands::{
    AccountCommandBuilder, KeysCommandBuilder, TradeCommandBuilder, TradingVehicleCommandBuilder,
    TransactionCommandBuilder,
};
use crate::dispatcher::ArgDispatcher;
use clap::Command;
use commands::RuleCommandBuilder;
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
            KeysCommandBuilder::new()
                .create_keys()
                .read_environment()
                .delete_environment()
                .build(),
        )
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
        .subcommand(
            RuleCommandBuilder::new()
                .create_rule()
                .remove_rule()
                .build(),
        )
        .subcommand(
            TradingVehicleCommandBuilder::new()
                .create_trading_vehicle()
                .search_trading_vehicle()
                .build(),
        )
        .subcommand(
            TradeCommandBuilder::new()
                .create_trade()
                .search_trade()
                .fund_trade()
                .cancel_trade()
                .submit_trade()
                .sync_trade()
                .manually_fill()
                .manually_stop()
                .manually_target()
                .manually_close()
                .modify_stop()
                .modify_target()
                .build(),
        )
        .get_matches();

    let dispatcher = ArgDispatcher::new_sqlite();
    dispatcher.dispatch(matches);
}
