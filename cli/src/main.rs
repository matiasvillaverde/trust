//! Trust CLI - Command Line Interface for Financial Trading
//!
//! This binary provides the command-line interface for the Trust financial
//! trading application with comprehensive risk management and safety features.

// === FINANCIAL APPLICATION SAFETY LINTS ===
// These lint rules are critical for financial applications where precision,
// safety, and reliability are paramount. Violations can lead to financial losses.

#![deny(
    // Error handling safety - force proper error handling
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,

    // Financial precision safety - prevent calculation errors
    clippy::float_arithmetic,
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,

    // Code quality enforcement
    clippy::cognitive_complexity,
    clippy::too_many_lines,
)]
// Allow unwrap and expect in test code only
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
// Standard Rust lints for code quality
#![warn(missing_docs, rust_2018_idioms, missing_debug_implementations)]

use crate::commands::{
    AccountCommandBuilder, GradeCommandBuilder, KeysCommandBuilder, LevelCommandBuilder,
    MetricsCommandBuilder, OnboardingCommandBuilder, PolicyCommandBuilder, ReportCommandBuilder,
    TradeCommandBuilder, TradingVehicleCommandBuilder, TransactionCommandBuilder,
};
use crate::dispatcher::ArgDispatcher;
use clap::Command;
use commands::RuleCommandBuilder;
mod commands;
mod dialogs;
mod dispatcher;
mod exporters;
mod protected_keyword;
mod views;

fn build_keys_subcommand() -> Command {
    KeysCommandBuilder::new()
        .create_keys()
        .read_environment()
        .delete_environment()
        .protected_set()
        .protected_show()
        .protected_delete()
        .build()
}

fn build_onboarding_subcommand() -> Command {
    OnboardingCommandBuilder::new().init().status().build()
}

fn build_policy_subcommand() -> Command {
    PolicyCommandBuilder::new().build()
}

fn build_cli() -> Command {
    Command::new("trust")
        .about("A tool for managing tradings")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(build_keys_subcommand())
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
        .subcommand(
            ReportCommandBuilder::new()
                .performance()
                .drawdown()
                .risk()
                .concentration()
                .summary()
                .metrics()
                .build(),
        )
        .subcommand(GradeCommandBuilder::new().show().summary().build())
        .subcommand(
            LevelCommandBuilder::new()
                .status()
                .triggers()
                .history()
                .change()
                .evaluate()
                .build(),
        )
        .subcommand(MetricsCommandBuilder::new().advanced().compare().build())
        .subcommand(build_onboarding_subcommand())
        .subcommand(build_policy_subcommand())
}

fn main() {
    let matches = build_cli().get_matches();

    let dispatcher = ArgDispatcher::new_sqlite();
    if let Err(error) = dispatcher.dispatch(matches) {
        if !error.already_printed() {
            eprintln!("{error}");
        }
        std::process::exit(1);
    }
}
