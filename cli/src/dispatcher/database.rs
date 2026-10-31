//! Legacy database command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_db_subcommand;
use crate::command_routing::DbSubcommand;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::output::ReportOutputFormat;
use crate::dispatcher::ArgDispatcher;
use clap::ArgMatches;
use core::TrustFacade;
use db_sqlite::ImportMode;
use db_sqlite::ImportOptions;
use db_sqlite::SqliteDatabase;
use std::path::Path;

pub(in crate::dispatcher) fn dispatch_db(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    match parse_db_subcommand(sub_matches) {
        DbSubcommand::Export(sub_sub_matches) => db_export(trust, sub_sub_matches)?,
        DbSubcommand::Import(sub_sub_matches) => db_import(trust, sub_sub_matches)?,
    }
    Ok(())
}

pub(in crate::dispatcher) fn db_export(
    _trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let output = sub_matches
        .get_one::<String>("output")
        .map(String::as_str)
        .unwrap_or("trust-backup.json");

    let db = SqliteDatabase::new(ArgDispatcher::database_url().as_str());
    db.export_backup_to_path(Path::new(output))
        .map_err(|error| {
            CliError::new(
                "db_export_failed",
                format!("Unable to export DB backup to {output}: {error}"),
            )
        })?;

    // Print the output path so scripts can capture it.
    println!("{output}");
    Ok(())
}

pub(in crate::dispatcher) fn db_import(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let input = sub_matches
        .get_one::<String>("input")
        .ok_or_else(|| CliError::new("missing_input", "Missing --input for db import"))?;

    // DB import mutates persisted state, so keep it protected.
    context::ensure_protected_keyword(
        trust,
        sub_matches,
        ReportOutputFormat::Text,
        "database import",
    )?;

    let mode = match sub_matches.get_one::<String>("mode").map(String::as_str) {
        Some("replace") => ImportMode::Replace,
        _ => ImportMode::Strict,
    };
    let dry_run = sub_matches.get_flag("dry-run");

    let mut db = SqliteDatabase::new(ArgDispatcher::database_url().as_str());
    let report = db
        .import_backup_from_path(Path::new(input), ImportOptions { mode, dry_run })
        .map_err(|error| {
            CliError::new(
                "db_import_failed",
                format!("Unable to import DB backup from {input}: {error}"),
            )
        })?;

    println!(
        "cleared_rows={} inserted_rows={}",
        report.cleared_rows, report.inserted_rows
    );
    Ok(())
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct DbCommand;
impl super::Command for DbCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_db(trust, args)
    }
}
