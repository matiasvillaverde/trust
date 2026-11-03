#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::DbCommandBuilder;
use uuid::Uuid;

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
