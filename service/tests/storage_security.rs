#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use rusqlite::Connection;
use std::{
    io::{Seek, SeekFrom, Write},
    os::unix::fs::PermissionsExt,
};
use trust_security::DatabaseKey;
use trust_service::{journal::Journal, storage::encrypted_copy};

fn read(path: &std::path::Path, key: &DatabaseKey) -> Connection {
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(&format!("PRAGMA key={};", key.sql_literal().as_str()))
        .unwrap();
    connection
}

#[test]
fn native_storage_uses_pinned_sqlcipher_and_creates_private_control_files() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.path().join("control.db");
    let key = DatabaseKey::generate().unwrap();
    Journal::open_encrypted(&path, &key).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let connection = read(&path, &key);
    let version: String = connection
        .query_row("PRAGMA cipher_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version.split_whitespace().next(), Some("4.19.0"));
    let sqlite: String = connection
        .query_row("SELECT sqlite_version()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(sqlite, "3.53.4");
    assert_eq!(
        connection
            .query_row(
                "SELECT sqlite_compileoption_used('TEMP_STORE=3')",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(Journal::open_encrypted(&path, &key).is_err());
}

#[test]
fn encrypted_copy_and_rotation_preserve_source_schema_rows_and_control_version() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let plain = dir.path().join("plain.db");
    trust_security::private_file(&plain, true).unwrap();
    let source = Connection::open(&plain).unwrap();
    source.execute_batch("CREATE TABLE records(id INTEGER PRIMARY KEY, value TEXT); INSERT INTO records VALUES (1,'protected-money-record'); CREATE INDEX records_value ON records(value); PRAGMA user_version=2; PRAGMA application_id=867;").unwrap();
    drop(source);
    let original = std::fs::read(&plain).unwrap();
    let first = DatabaseKey::generate().unwrap();
    let encrypted = dir.path().join("encrypted.db");
    encrypted_copy(&plain, &encrypted, &first, None).unwrap();
    assert_eq!(std::fs::read(&plain).unwrap(), original);
    assert!(!std::fs::read(&encrypted)
        .unwrap()
        .windows(22)
        .any(|window| window == b"protected-money-record"));
    assert!(Connection::open(&encrypted)
        .unwrap()
        .query_row("SELECT count(*) FROM records", [], |row| row
            .get::<_, i64>(0))
        .is_err());
    assert_eq!(
        read(&encrypted, &first)
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        2
    );
    let second = DatabaseKey::generate().unwrap();
    let rotated = dir.path().join("rotated.db");
    encrypted_copy(&encrypted, &rotated, &second, Some(&first)).unwrap();
    let db = read(&rotated, &second);
    assert_eq!(
        db.query_row("SELECT value FROM records", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "protected-money-record"
    );
    assert_eq!(
        db.query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        867
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='records_value'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert!(read(&rotated, &first)
        .query_row("SELECT count(*) FROM records", [], |row| row
            .get::<_, i64>(0))
        .is_err());
    assert!(encrypted_copy(&plain, &rotated, &second, None).is_err());
    assert!(encrypted_copy(
        &encrypted,
        &dir.path().join("wrong.db"),
        &second,
        Some(&second)
    )
    .is_err());
    assert!(!dir.path().join("wrong.db").exists());
    let lock = trust_security::read_private_file(&plain).unwrap();
    fs2::FileExt::try_lock_exclusive(&lock).unwrap();
    assert_eq!(
        encrypted_copy(&plain, &dir.path().join("locked.db"), &first, None)
            .unwrap_err()
            .code,
        "storage_locked"
    );
}

#[test]
fn sqlcipher_financial_backup_restores_and_rejects_wrong_keys_plaintext_and_corruption() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let key = DatabaseKey::generate().unwrap();
    let backup_key = DatabaseKey::generate().unwrap();
    let path = dir.path().join("financial.db");
    let database = db_sqlite::SqliteDatabase::open_encrypted(&path, &key).unwrap();
    let backup = dir.path().join("backup.db");
    database
        .export_encrypted_to_path(&backup, &backup_key)
        .unwrap();
    assert!(database
        .export_encrypted_to_path(&backup, &backup_key)
        .is_err());
    assert!(db_sqlite::SqliteDatabase::open_encrypted(&path, &backup_key).is_err());
    let restored_path = dir.path().join("restored.db");
    let mut restored = db_sqlite::SqliteDatabase::open_encrypted(&restored_path, &key).unwrap();
    assert!(restored.import_encrypted_from_path(&backup, &key).is_err());
    restored
        .import_encrypted_from_path(&backup, &backup_key)
        .unwrap();
    let plain = dir.path().join("plain.db");
    trust_security::private_file(&plain, true).unwrap();
    let _plain_db = db_sqlite::SqliteDatabase::new(plain.to_str().unwrap());
    assert!(db_sqlite::SqliteDatabase::open_encrypted(&plain, &key).is_err());
    assert!(restored.import_encrypted_from_path(&plain, &key).is_err());
    let corrupt = dir.path().join("corrupt.db");
    std::fs::copy(&backup, &corrupt).unwrap();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(&corrupt)
        .unwrap();
    file.seek(SeekFrom::Start(512)).unwrap();
    file.write_all(&[0; 32]).unwrap();
    file.sync_all().unwrap();
    assert!(restored
        .import_encrypted_from_path(&corrupt, &backup_key)
        .is_err());
    assert_eq!(
        std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn encrypted_control_copy_preserves_audit_chain_and_reopens_after_rotation() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let plain = dir.path().join("plain-control.db");
    trust_security::private_file(&plain, true).unwrap();
    let before = Journal::open(&plain).unwrap().verify().unwrap();
    let key = DatabaseKey::generate().unwrap();
    let encrypted = dir.path().join("encrypted-control.db");
    encrypted_copy(&plain, &encrypted, &key, None).unwrap();
    let after = Journal::open_encrypted(&encrypted, &key)
        .unwrap()
        .verify()
        .unwrap();
    assert_eq!(before, after);
    assert!(Journal::open(&encrypted).is_err());
    assert!(Journal::open_encrypted(&encrypted, &DatabaseKey::generate().unwrap()).is_err());
}
