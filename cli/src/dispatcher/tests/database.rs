#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::DbCommandBuilder;
use crate::dispatcher::error::CliError;
use clap::Command;
use uuid::Uuid;

#[test]
fn test_dispatchers_require_nested_subcommands() {
    let mut dispatcher = test_dispatcher();
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let mut check_panics_without_subcommand =
        |f: fn(&mut core::TrustFacade, &clap::ArgMatches) -> Result<(), CliError>,
         name: &'static str| {
            let matches = Command::new(name).get_matches_from([name]);
            let result = catch_unwind(AssertUnwindSafe(|| f(&mut dispatcher.trust, &matches)));
            assert!(
                result.is_err(),
                "{name} dispatcher should panic when subcommand is absent"
            );
        };

    check_panics_without_subcommand(crate::dispatcher::database::dispatch_db, "db");
    check_panics_without_subcommand(crate::dispatcher::keys::dispatch_keys, "keys");
    check_panics_without_subcommand(crate::dispatcher::accounts::dispatch_accounts, "accounts");
    check_panics_without_subcommand(
        crate::dispatcher::capital::dispatch_transactions,
        "transaction",
    );
    check_panics_without_subcommand(crate::dispatcher::rules::dispatch_rules, "rule");
    check_panics_without_subcommand(
        crate::dispatcher::instruments::dispatch_trading_vehicle,
        "trading-vehicle",
    );
    check_panics_without_subcommand(crate::dispatcher::trade_commands::dispatch_trade, "trade");
    check_panics_without_subcommand(
        crate::dispatcher::distribution::dispatch_distribution,
        "distribution",
    );
    check_panics_without_subcommand(crate::dispatcher::reports::dispatch_report, "report");
    check_panics_without_subcommand(
        crate::dispatcher::market_data::dispatch_market_data,
        "market-data",
    );
    check_panics_without_subcommand(crate::dispatcher::grades::dispatch_grade, "grade");
    check_panics_without_subcommand(crate::dispatcher::levels::dispatch_level, "level");
    check_panics_without_subcommand(
        crate::dispatcher::onboarding::dispatch_onboarding,
        "onboarding",
    );
    check_panics_without_subcommand(
        crate::dispatcher::advanced_metrics::dispatch_metrics,
        "metrics",
    );
    check_panics_without_subcommand(
        crate::dispatcher::advisor_config::dispatch_advisor,
        "advisor",
    );
    check_panics_without_subcommand(crate::dispatcher::sessions::dispatch_session, "session");
}

#[test]
fn test_db_export_import_success_and_export_error_paths() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let db_path = format!("/tmp/trust-db-dispatch-{}.db", Uuid::new_v4());
    let backup_path = format!("/tmp/trust-db-dispatch-{}.json", Uuid::new_v4());
    std::env::set_var("TRUST_DB_URL", &db_path);
    let mut dispatcher = test_dispatcher();

    let export = DbCommandBuilder::new().export().build().get_matches_from([
        "db",
        "export",
        "--output",
        &backup_path,
    ]);
    crate::dispatcher::database::db_export(
        &mut dispatcher.trust,
        export.subcommand_matches("export").unwrap(),
    )
    .expect("database export should write backup");
    assert!(std::path::Path::new(&backup_path).exists());

    let import = DbCommandBuilder::new().import().build().get_matches_from([
        "db",
        "import",
        "--input",
        &backup_path,
        "--mode",
        "replace",
        "--dry-run",
        "--confirm-protected",
        "secret",
    ]);
    crate::dispatcher::database::db_import(
        &mut dispatcher.trust,
        import.subcommand_matches("import").unwrap(),
    )
    .expect("database import dry run should validate backup");

    let export_to_directory = DbCommandBuilder::new()
        .export()
        .build()
        .get_matches_from(["db", "export", "--output", "/tmp"]);
    let error = crate::dispatcher::database::db_export(
        &mut dispatcher.trust,
        export_to_directory.subcommand_matches("export").unwrap(),
    )
    .expect_err("exporting to a directory should fail");
    assert!(error.to_string().contains("db_export_failed"));

    std::env::remove_var("TRUST_DB_URL");
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(&backup_path);
}
