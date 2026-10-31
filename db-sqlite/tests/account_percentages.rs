use db_sqlite::{ImportOptions, SqliteDatabase};
use diesel::{Connection, RunQueryDsl};
use diesel_migrations::{embed_migrations, MigrationHarness};
use model::{DatabaseFactory, Environment};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
const MIGRATIONS: diesel_migrations::EmbeddedMigrations = embed_migrations!("migrations");

#[test]
fn new_accounts_use_points_and_reject_invalid_reserves_without_rows() {
    let db = SqliteDatabase::new_in_memory();
    for (tax, earnings) in [
        (dec!(-1), dec!(0)),
        (dec!(0), dec!(-1)),
        (dec!(101), dec!(0)),
        (dec!(0), dec!(101)),
        (dec!(80), dec!(80)),
        (Decimal::MAX, Decimal::MAX),
    ] {
        assert!(db
            .account_write()
            .create("invalid", "", Environment::Paper, tax, earnings)
            .is_err());
        assert!(db.account_read().all().unwrap().is_empty());
    }
    for tax in [dec!(0), dec!(0.25), dec!(1), dec!(2), dec!(100)] {
        let account = db
            .account_write()
            .create(&tax.to_string(), "", Environment::Paper, tax, dec!(0))
            .unwrap();
        assert_eq!(
            db.account_read().id(account.id).unwrap().taxes_percentage,
            tax
        );
    }
}

#[test]
fn legacy_upgrade_preserves_old_tax_liability_and_new_values_survive_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("accounts.db");
    let url = path.to_str().unwrap();
    let db = SqliteDatabase::new(url);
    let account = db
        .account_write()
        .create("legacy", "", Environment::Paper, dec!(25), dec!(0))
        .unwrap();
    drop(db);
    let mut connection = diesel::SqliteConnection::establish(url).unwrap();
    diesel::sql_query(
        "UPDATE accounts SET tax_percentage_units = 'legacy', taxes_percentage = '0.25'",
    )
    .execute(&mut connection)
    .unwrap();
    connection.revert_last_migration(MIGRATIONS).unwrap();
    connection.revert_last_migration(MIGRATIONS).unwrap();
    drop(connection);
    let db = SqliteDatabase::new(url);
    assert_eq!(
        db.account_read().id(account.id).unwrap().taxes_percentage,
        dec!(25)
    );
    let new = db
        .account_write()
        .create("new", "", Environment::Paper, dec!(1), dec!(0))
        .unwrap();
    drop(db);
    let reopened = SqliteDatabase::new(url);
    assert_eq!(
        reopened.account_read().id(new.id).unwrap().taxes_percentage,
        dec!(1)
    );
    assert_eq!(
        reopened
            .account_read()
            .id(account.id)
            .unwrap()
            .taxes_percentage,
        dec!(25)
    );
}

#[test]
fn new_units_cannot_be_downgraded_into_ambiguous_values() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("downgrade.db");
    let url = path.to_str().unwrap();
    let db = SqliteDatabase::new(url);
    let account = db
        .account_write()
        .create("one percent", "", Environment::Paper, dec!(1), dec!(0))
        .unwrap();
    drop(db);
    let mut connection = diesel::SqliteConnection::establish(url).unwrap();
    connection.revert_last_migration(MIGRATIONS).unwrap(); // history indexes
    assert!(connection.revert_last_migration(MIGRATIONS).is_err());
    drop(connection);
    let reopened = SqliteDatabase::new(url);
    assert_eq!(
        reopened
            .account_read()
            .id(account.id)
            .unwrap()
            .taxes_percentage,
        dec!(1)
    );
}

#[test]
fn backups_roundtrip_explicit_units_and_restore_legacy_v2_without_guessing_new_values() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("backup.json");
    let db = SqliteDatabase::new_in_memory();
    let account = db
        .account_write()
        .create("tax", "", Environment::Paper, dec!(1), dec!(0))
        .unwrap();
    db.export_backup_to_path(&path).unwrap();
    let mut restored = SqliteDatabase::new_in_memory();
    restored
        .import_backup_from_path(&path, ImportOptions::default())
        .unwrap();
    assert_eq!(
        restored
            .account_read()
            .id(account.id)
            .unwrap()
            .taxes_percentage,
        dec!(1)
    );
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    legacy["version"] = 2.into();
    legacy["tables"]["accounts"][0]
        .as_object_mut()
        .unwrap()
        .remove("tax_percentage_units");
    legacy["schema"]["diesel_migrations"]
        .as_array_mut()
        .unwrap()
        .retain(|value| !value.as_str().unwrap().starts_with("20261031"));
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let mut restored = SqliteDatabase::new_in_memory();
    restored
        .import_backup_from_path(&path, ImportOptions::default())
        .unwrap();
    assert_eq!(
        restored
            .account_read()
            .id(account.id)
            .unwrap()
            .taxes_percentage,
        dec!(100)
    );
}

#[test]
fn malformed_percentage_units_or_negative_legacy_values_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("backup.json");
    let db = SqliteDatabase::new_in_memory();
    db.account_write()
        .create("tax", "", Environment::Paper, dec!(1), dec!(0))
        .unwrap();
    db.export_backup_to_path(&path).unwrap();
    let valid: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut missing = valid.clone();
    missing["tables"]["accounts"][0]
        .as_object_mut()
        .unwrap()
        .remove("tax_percentage_units");
    std::fs::write(&path, serde_json::to_vec(&missing).unwrap()).unwrap();
    assert!(SqliteDatabase::new_in_memory()
        .import_backup_from_path(&path, ImportOptions::default())
        .is_err());
    for (units, tax) in [
        ("unknown", "1"),
        ("legacy", "-1"),
        ("percentage_points", "101"),
    ] {
        let mut invalid = valid.clone();
        invalid["tables"]["accounts"][0]["tax_percentage_units"] = units.into();
        invalid["tables"]["accounts"][0]["taxes_percentage"] = tax.into();
        std::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        let mut target = SqliteDatabase::new_in_memory();
        assert!(target
            .import_backup_from_path(&path, ImportOptions::default())
            .is_err());
        assert!(target.account_read().all().unwrap().is_empty());
    }
}
