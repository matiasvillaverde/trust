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
    AccountCommandBuilder, AdvisorCommandBuilder, DbCommandBuilder, DistributionCommandBuilder,
    GradeCommandBuilder, KeysCommandBuilder, LevelCommandBuilder, MarketDataCommandBuilder,
    MetricsCommandBuilder, OnboardingCommandBuilder, PolicyCommandBuilder, ReportCommandBuilder,
    TradeCommandBuilder, TradingVehicleCommandBuilder, TransactionCommandBuilder,
};
use crate::dispatcher::ArgDispatcher;
use clap::Command;
use commands::RuleCommandBuilder;
mod command_routing;
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

#[allow(clippy::too_many_lines)]
fn build_cli() -> Command {
    Command::new("trust")
        .about("A tool for managing tradings")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(DbCommandBuilder::new().export().import().build())
        .subcommand(build_keys_subcommand())
        .subcommand(
            AccountCommandBuilder::new()
                .create_account()
                .read_account()
                .list_accounts()
                .balance_accounts()
                .transfer_account()
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
                .watch_trade()
                .manually_fill()
                .manually_stop()
                .manually_target()
                .manually_close()
                .modify_stop()
                .modify_target()
                .list_open()
                .reconcile()
                .size_preview()
                .build(),
        )
        .subcommand(
            DistributionCommandBuilder::new()
                .configure_distribution()
                .execute_distribution()
                .history()
                .show_rules()
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
                .attribution()
                .benchmark()
                .timeline()
                .build(),
        )
        .subcommand(
            MarketDataCommandBuilder::new()
                .snapshot()
                .bars()
                .stream()
                .quote()
                .trade()
                .session()
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
                .progress()
                .rules()
                .build(),
        )
        .subcommand(MetricsCommandBuilder::new().advanced().compare().build())
        .subcommand(
            AdvisorCommandBuilder::new()
                .configure()
                .check()
                .status()
                .history()
                .build(),
        )
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

#[cfg(test)]
mod tests {
    use super::build_cli;

    #[test]
    fn cli_requires_subcommand() {
        let result = build_cli().try_get_matches_from(["trust"]);
        assert!(result.is_err());
    }

    #[test]
    fn cli_registers_expected_top_level_subcommands() {
        let names: Vec<String> = build_cli()
            .get_subcommands()
            .map(|sc| sc.get_name().to_string())
            .collect();

        for expected in [
            "db",
            "keys",
            "accounts",
            "transaction",
            "rule",
            "trading-vehicle",
            "trade",
            "distribution",
            "report",
            "market-data",
            "grade",
            "level",
            "metrics",
            "advisor",
            "onboarding",
            "policy",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "missing subcommand: {expected}"
            );
        }
    }

    #[test]
    fn cli_parses_known_command_shape() {
        let matches = build_cli()
            .try_get_matches_from(["trust", "report", "summary", "--format", "json"])
            .expect("valid report command should parse");

        let (sub, report_matches) = matches
            .subcommand()
            .expect("top-level subcommand should exist");
        assert_eq!(sub, "report");
        let (nested, _) = report_matches
            .subcommand()
            .expect("nested report subcommand should exist");
        assert_eq!(nested, "summary");
    }
}
