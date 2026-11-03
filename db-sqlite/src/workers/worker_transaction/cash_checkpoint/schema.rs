//! Connection-local derived state. TEMP writes and their invalidation triggers
//! participate in the same savepoints as the authoritative ledger.
use diesel::{
    prelude::*,
    sql_types::{BigInt, Integer, Text},
};
use std::error::Error;
use std::sync::OnceLock;

const TABLE: &str = "CREATE TEMP TABLE trust_verified_cash (
 account_id TEXT NOT NULL, currency TEXT NOT NULL, data_version INTEGER NOT NULL,
 schema_version INTEGER NOT NULL,
 total TEXT, last_created_at TEXT, last_id TEXT, pending_id TEXT,
 PRIMARY KEY (account_id, currency),
 CHECK ((last_created_at IS NULL) = (last_id IS NULL))
)";
const INSERT: &str =
    "CREATE TEMP TRIGGER trust_verified_cash_insert BEFORE INSERT ON main.transactions
BEGIN
 DELETE FROM trust_verified_cash WHERE
   EXISTS (SELECT 1 FROM main.transactions WHERE id = NEW.id OR rowid = NEW.rowid)
   OR (account_id = NEW.account_id AND currency = NEW.currency
       AND (pending_id IS NOT NULL OR (NEW.created_at, NEW.id) <= (last_created_at, last_id)));
 UPDATE trust_verified_cash SET pending_id = NEW.id
 WHERE account_id = NEW.account_id AND currency = NEW.currency
   AND NEW.deleted_at IS NULL AND NEW.category IN (
    'deposit', 'withdrawal', 'withdrawal_earnings', 'fee_open', 'fee_close',
    'fund_trade', 'payment_from_trade', 'payment_earnings');
END";
// All prefixes are discarded for rare edits/deletions, including UPDATE OR
// REPLACE conflicts. Normal appends retain only strictly earlier prefixes.
const UPDATE: &str =
    "CREATE TEMP TRIGGER trust_verified_cash_update BEFORE UPDATE ON main.transactions
BEGIN DELETE FROM trust_verified_cash; END";
const DELETE: &str =
    "CREATE TEMP TRIGGER trust_verified_cash_delete BEFORE DELETE ON main.transactions
BEGIN DELETE FROM trust_verified_cash; END";
const APPEND: &str =
    "CREATE TEMP TRIGGER trust_verified_cash_append AFTER INSERT ON main.transactions
WHEN NEW.deleted_at IS NULL AND NEW.category IN (
 'deposit', 'withdrawal', 'withdrawal_earnings', 'fee_open', 'fee_close',
 'fund_trade', 'payment_from_trade', 'payment_earnings')
BEGIN
 UPDATE trust_verified_cash SET
  total = trust_cash_append(total, NEW.category, NEW.amount, NEW.trade_id,
    CAST(NEW.created_at AS BLOB), CAST(NEW.id AS BLOB),
    typeof(NEW.created_at) = 'text' AND typeof(NEW.id) = 'text'),
  last_created_at = NEW.created_at, last_id = NEW.id, pending_id = NULL
 WHERE account_id = NEW.account_id AND currency = NEW.currency
   AND pending_id = NEW.id
   AND data_version = (SELECT data_version FROM pragma_data_version);
END";
pub(super) const OBJECTS: [(&str, &str); 5] = [
    ("trust_verified_cash", TABLE),
    ("trust_verified_cash_insert", INSERT),
    ("trust_verified_cash_update", UPDATE),
    ("trust_verified_cash_delete", DELETE),
    ("trust_verified_cash_append", APPEND),
];

pub(super) fn stored_definitions() -> &'static [(&'static str, String); 5] {
    static DEFINITIONS: OnceLock<[(&str, String); 5]> = OnceLock::new();
    DEFINITIONS.get_or_init(|| OBJECTS.map(|(name, sql)| (name, sql.replacen(" TEMP ", " ", 1))))
}

#[derive(QueryableByName)]
struct SchemaVersion {
    #[diesel(sql_type = BigInt)]
    schema_version: i64,
}

pub(super) fn version(conn: &mut SqliteConnection) -> QueryResult<i64> {
    // SQLite's table-valued pragma_schema_version reports main only. Use the
    // explicit TEMP pragma; qualifying that virtual table with temp is not enough.
    Ok(diesel::sql_query("PRAGMA temp.schema_version")
        .get_result::<SchemaVersion>(conn)?
        .schema_version)
}

#[derive(QueryableByName)]
struct Settings {
    #[diesel(sql_type = Integer)]
    enabled: i32,
}

pub(super) fn supported(conn: &mut SqliteConnection) -> Result<bool, Box<dyn Error>> {
    // Never put cash checkpoints in plaintext temporary files. Read-only
    // connections and unsupported SQLite builds retain full replay.
    let settings: Settings = diesel::sql_query(
        "SELECT
      (SELECT temp_store FROM pragma_temp_store) = 2
      AND NOT sqlite_compileoption_used('TEMP_STORE=0')
      AND (SELECT query_only FROM pragma_query_only) = 0 AS enabled",
    )
    .get_result(conn)?;
    Ok(settings.enabled != 0)
}

#[derive(QueryableByName)]
struct Definition {
    #[diesel(sql_type = Text)]
    name: String,
    #[diesel(sql_type = Text)]
    sql: String,
}

pub(super) fn ensure(conn: &mut SqliteConnection) -> Result<(), Box<dyn Error>> {
    super::append::register(conn)?;
    // Check actual definitions, not an in-memory installed flag: TEMP DDL can
    // itself roll back, and a replaced/missing trigger cannot validate a prefix.
    let definitions = diesel::sql_query(
        "SELECT name, sql FROM temp.sqlite_schema WHERE name IN (
      'trust_verified_cash', 'trust_verified_cash_insert',
      'trust_verified_cash_update', 'trust_verified_cash_delete', 'trust_verified_cash_append')",
    )
    .load::<Definition>(conn)?;
    let valid = definitions.len() == OBJECTS.len()
        && definitions.iter().all(|row| {
            stored_definitions()
                .iter()
                .any(|(name, sql)| row.name == *name && row.sql == *sql)
        });
    if valid {
        return Ok(());
    }
    for sql in [
        "DROP TRIGGER IF EXISTS temp.trust_verified_cash_insert",
        "DROP TRIGGER IF EXISTS temp.trust_verified_cash_update",
        "DROP TRIGGER IF EXISTS temp.trust_verified_cash_delete",
        "DROP TRIGGER IF EXISTS temp.trust_verified_cash_append",
        "DROP TABLE IF EXISTS temp.trust_verified_cash",
    ] {
        diesel::sql_query(sql).execute(conn)?;
    }
    for (_, sql) in OBJECTS {
        diesel::sql_query(sql).execute(conn)?;
    }
    Ok(())
}
