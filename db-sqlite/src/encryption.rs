//! SQLCipher setup occurs before the first schema read or migration.
use diesel::{connection::SimpleConnection, prelude::*, sql_types::Text};
use trust_security::DatabaseKey;

/// Redacted storage errors never expose encryption SQL, key bytes or row contents.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Opening requires a secure file and a readable, compatible encrypted database.
    #[error("Cannot open protected database; check file permissions, key and SQLCipher format. Existing plaintext databases require offline encrypted-copy migration")]
    Open,
    /// A build accidentally linked to ordinary SQLite must never ignore PRAGMA key.
    #[error("SQLCipher 4.19 or later stable 4.x is required; ordinary SQLite cannot open protected storage")]
    Backend,
    /// Schema, I/O and backup failures do not reveal potentially sensitive errors.
    #[error("Protected database operation failed; check storage, schema compatibility and recovery evidence")]
    Database,
}

#[derive(QueryableByName)]
struct Version {
    #[diesel(sql_type = Text)]
    cipher_version: String,
}

pub(crate) fn unlock(
    connection: &mut SqliteConnection,
    key: &DatabaseKey,
) -> Result<(), StorageError> {
    // Global Diesel query loggers must not receive key-bearing SQL.
    connection.set_instrumentation(None::<fn(diesel::connection::InstrumentationEvent<'_>)>);
    let version = diesel::sql_query("PRAGMA cipher_version")
        .get_result::<Version>(connection)
        .map_err(|_| StorageError::Backend)?;
    if !trust_security::supported_sqlcipher_version(&version.cipher_version) {
        return Err(StorageError::Backend);
    }
    connection
        .batch_execute("PRAGMA cipher_log_level=NONE;")
        .map_err(|_| StorageError::Open)?;
    let command = zeroize::Zeroizing::new(format!("PRAGMA key = {};", key.sql_literal().as_str()));
    connection
        .batch_execute(&command)
        .map_err(|_| StorageError::Open)?;
    connection.batch_execute("PRAGMA cipher_memory_security = ON; PRAGMA temp_store = MEMORY; PRAGMA synchronous = FULL; PRAGMA busy_timeout = 5000;")
        .map_err(|_| StorageError::Open)?;
    // PRAGMA key alone does not validate the key. This read must succeed first.
    diesel::select(diesel::dsl::sql::<diesel::sql_types::BigInt>(
        "(SELECT count(*) FROM sqlite_master)",
    ))
    .get_result::<i64>(connection)
    .map_err(|_| StorageError::Open)?;
    Ok(())
}
