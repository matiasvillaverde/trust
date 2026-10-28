//! SQLCipher control storage and offline copy operations.
use crate::{config::storage_error, error::Result};
use rusqlite::Connection;
use std::path::Path;
use trust_security::DatabaseKey;

pub(crate) fn unlock(connection: &Connection, key: &DatabaseKey) -> Result<()> {
    let version: String = connection
        .query_row("PRAGMA cipher_version", [], |row| row.get(0))
        .map_err(|_| storage_error())?;
    if !trust_security::supported_sqlcipher_version(&version) {
        return Err(storage_error());
    }
    connection
        .execute_batch("PRAGMA cipher_log_level=NONE;")
        .map_err(|_| storage_error())?;
    let command = zeroize::Zeroizing::new(format!("PRAGMA key = {};", key.sql_literal().as_str()));
    connection
        .execute_batch(&command)
        .map_err(|_| storage_error())?;
    connection.execute_batch("PRAGMA cipher_memory_security=ON; PRAGMA temp_store=MEMORY; PRAGMA synchronous=FULL; PRAGMA busy_timeout=5000;").map_err(|_| storage_error())?;
    connection
        .query_row("SELECT count(*) FROM sqlite_master", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|_| storage_error())?;
    Ok(())
}

fn verify(connection: &Connection, encrypted: bool) -> Result<()> {
    if encrypted {
        let mut statement = connection
            .prepare("PRAGMA cipher_integrity_check")
            .map_err(|_| storage_error())?;
        let mut rows = statement.query([]).map_err(|_| storage_error())?;
        if rows.next().map_err(|_| storage_error())?.is_some() {
            return Err(storage_error());
        }
    }
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|_| storage_error())?;
    if integrity != "ok" {
        return Err(storage_error());
    }
    let mut statement = connection
        .prepare("PRAGMA foreign_key_check")
        .map_err(|_| storage_error())?;
    if statement
        .query([])
        .map_err(|_| storage_error())?
        .next()
        .map_err(|_| storage_error())?
        .is_some()
    {
        return Err(storage_error());
    }
    Ok(())
}

/// Copy a stopped financial/control database into a new encrypted file. Source
/// locks match the service's locks. There is no in-place rekey or plaintext output.
pub fn encrypted_copy(
    source: &Path,
    destination: &Path,
    destination_key: &DatabaseKey,
    source_key: Option<&DatabaseKey>,
) -> Result<()> {
    let source_file = trust_security::read_private_file(source).map_err(|_| storage_error())?;
    fs2::FileExt::try_lock_exclusive(&source_file).map_err(|_| {
        crate::error::ServiceError::new(
            "storage_locked",
            "The source database is in use.",
            "Stop Trust before copying or rotating either database.",
        )
    })?;
    let flags =
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW;
    // SQLite propagates the main connection's open mode to ATTACH. It must permit
    // writing the new attached file; our fixed SQL only reads the locked source.
    let connection = Connection::open_with_flags(
        source,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| storage_error())?;
    connection
        .set_db_config(
            rusqlite::config::DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE,
            true,
        )
        .map_err(|_| storage_error())?;
    if let Some(key) = source_key {
        unlock(&connection, key)?;
    } else {
        let version: String = connection
            .query_row("PRAGMA cipher_version", [], |row| row.get(0))
            .map_err(|_| storage_error())?;
        if !trust_security::supported_sqlcipher_version(&version) {
            return Err(storage_error());
        }
        connection
            .execute_batch("PRAGMA cipher_memory_security=ON; PRAGMA temp_store=MEMORY;")
            .map_err(|_| storage_error())?;
    }
    verify(&connection, source_key.is_some())?;
    let file = trust_security::private_file(destination, true).map_err(|_| storage_error())?;
    let result = copy_into(&connection, destination, destination_key);
    drop(connection);
    let result = result.and_then(|()| {
        let destination_connection =
            Connection::open_with_flags(destination, flags).map_err(|_| storage_error())?;
        unlock(&destination_connection, destination_key)?;
        verify(&destination_connection, true)?;
        drop(destination_connection);
        file.sync_all().map_err(|_| storage_error())?;
        std::fs::File::open(destination.parent().ok_or_else(storage_error)?)
            .and_then(|file| file.sync_all())
            .map_err(|_| storage_error())
    });
    if result.is_err() {
        let _ = std::fs::remove_file(destination);
    }
    result
}

fn copy_into(connection: &Connection, destination: &Path, key: &DatabaseKey) -> Result<()> {
    connection
        .execute(
            "ATTACH DATABASE ?1 AS encrypted_copy KEY ?2",
            rusqlite::params![
                destination.to_str().ok_or_else(storage_error)?,
                key.sql_parameter().as_str()
            ],
        )
        .map_err(|_| storage_error())?;
    connection
        .execute_batch("BEGIN;")
        .map_err(|_| storage_error())?;
    let result = (|| {
        let version: i64 = connection
            .query_row("PRAGMA main.user_version", [], |row| row.get(0))
            .map_err(|_| storage_error())?;
        let application: i64 = connection
            .query_row("PRAGMA main.application_id", [], |row| row.get(0))
            .map_err(|_| storage_error())?;
        connection
            .query_row("SELECT sqlcipher_export('encrypted_copy')", [], |_| Ok(()))
            .map_err(|_| storage_error())?;
        connection.execute_batch(&format!("PRAGMA encrypted_copy.user_version={version}; PRAGMA encrypted_copy.application_id={application}; COMMIT; DETACH DATABASE encrypted_copy;")).map_err(|_| storage_error())
    })();
    if result.is_err() {
        let _ = connection.execute_batch("ROLLBACK;");
    }
    result
}
