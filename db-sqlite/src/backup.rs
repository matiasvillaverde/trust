//! Full-database backup/restore for Trust SQLite.
//!
//! This module exports and imports *all* rows from all application tables, including soft-deleted
//! rows (`deleted_at IS NOT NULL`). The backup format is JSON for portability and simple diffing.

use crate::schema;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use diesel::connection::Connection;
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::{BigInt, Integer, Text};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

const BACKUP_FORMAT: &str = "trust-backup";
const BACKUP_VERSION_V1: u32 = 1;

/// Errors returned by backup/export/import operations.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    #[error("db error: {0}")]
    Diesel(#[from] diesel::result::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

/// Backup import strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    /// Fail if the target DB is not empty or would create conflicts.
    Strict,
    /// Delete all known rows from known tables, then import.
    Replace,
}

/// Options controlling import behavior.
#[derive(Debug, Clone, Copy)]
pub struct ImportOptions {
    /// Import mode.
    pub mode: ImportMode,
    /// Validate only; do not write anything.
    pub dry_run: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            mode: ImportMode::Strict,
            dry_run: false,
        }
    }
}

/// Result summary of an import operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportReport {
    /// Number of rows inserted during import.
    pub inserted_rows: u64,
    /// Number of rows deleted during pre-import clearing (only in `replace` mode).
    pub cleared_rows: u64,
}

/// JSON backup envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupEnvelopeV1 {
    pub format: String,
    pub version: u32,
    pub exported_at: DateTime<Utc>,
    pub schema: BackupSchema,
    pub tables: BackupTablesV1,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupSchema {
    pub diesel_migrations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupTablesV1 {
    pub accounts: Vec<AccountRow>,
    pub accounts_balances: Vec<AccountBalanceRow>,
    pub rules: Vec<RuleRow>,
    pub transactions: Vec<TransactionRow>,
    pub trading_vehicles: Vec<TradingVehicleRow>,
    pub orders: Vec<OrderRow>,
    pub trades_balances: Vec<TradeBalanceRow>,
    pub trades: Vec<TradeRow>,
    pub logs: Vec<LogRow>,
    pub levels: Vec<LevelRow>,
    pub level_changes: Vec<LevelChangeRow>,
    pub trade_grades: Vec<TradeGradeRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::accounts)]
pub struct AccountRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub name: String,
    pub description: String,
    pub environment: String,
    pub taxes_percentage: String,
    pub earnings_percentage: String,
    #[serde(default = "default_account_type")]
    pub account_type: String,
    #[serde(default)]
    pub parent_account_id: Option<String>,
}

fn default_account_type() -> String {
    "primary".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::accounts_balances)]
pub struct AccountBalanceRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub account_id: String,
    pub total_balance: String,
    pub total_in_trade: String,
    pub total_available: String,
    pub taxed: String,
    pub currency: String,
    pub total_earnings: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::rules)]
pub struct RuleRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub name: String,
    pub risk: i32,
    pub description: String,
    pub priority: i32,
    pub level: String,
    pub account_id: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::transactions)]
pub struct TransactionRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub currency: String,
    pub category: String,
    pub amount: String,
    pub account_id: String,
    pub trade_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::trading_vehicles)]
pub struct TradingVehicleRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub symbol: String,
    pub isin: Option<String>,
    pub category: String,
    pub broker: String,
    pub broker_asset_id: Option<String>,
    pub exchange: Option<String>,
    pub broker_asset_class: Option<String>,
    pub broker_asset_status: Option<String>,
    pub tradable: Option<bool>,
    pub marginable: Option<bool>,
    pub shortable: Option<bool>,
    pub easy_to_borrow: Option<bool>,
    pub fractionable: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::orders)]
pub struct OrderRow {
    pub id: String,
    pub broker_order_id: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub unit_price: String,
    pub currency: String,
    pub quantity: i32,
    pub category: String,
    pub trading_vehicle_id: String,
    pub action: String,
    pub status: String,
    pub time_in_force: String,
    pub trailing_percentage: Option<String>,
    pub trailing_price: Option<String>,
    pub filled_quantity: Option<i32>,
    pub average_filled_price: Option<String>,
    pub extended_hours: bool,
    pub submitted_at: Option<NaiveDateTime>,
    pub filled_at: Option<NaiveDateTime>,
    pub expired_at: Option<NaiveDateTime>,
    pub cancelled_at: Option<NaiveDateTime>,
    pub closed_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::trades_balances)]
pub struct TradeBalanceRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub currency: String,
    pub funding: String,
    pub capital_in_market: String,
    pub capital_out_market: String,
    pub taxed: String,
    pub total_performance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::trades)]
pub struct TradeRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub category: String,
    pub status: String,
    pub currency: String,
    pub trading_vehicle_id: String,
    pub safety_stop_id: String,
    pub entry_id: String,
    pub target_id: String,
    pub account_id: String,
    pub balance_id: String,
    pub thesis: Option<String>,
    pub sector: Option<String>,
    pub asset_class: Option<String>,
    pub context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::logs)]
pub struct LogRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub log: String,
    pub trade_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::levels)]
pub struct LevelRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub account_id: String,
    pub current_level: i32,
    pub risk_multiplier: String,
    pub status: String,
    pub trades_at_level: i32,
    pub level_start_date: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::level_changes)]
pub struct LevelChangeRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub account_id: String,
    pub old_level: i32,
    pub new_level: i32,
    pub change_reason: String,
    pub trigger_type: String,
    pub changed_at: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::trade_grades)]
pub struct TradeGradeRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub trade_id: String,
    pub overall_score: i32,
    pub overall_grade: String,
    pub process_score: i32,
    pub risk_score: i32,
    pub execution_score: i32,
    pub documentation_score: i32,
    pub recommendations: Option<String>,
    pub graded_at: NaiveDateTime,
    pub process_weight_permille: i32,
    pub risk_weight_permille: i32,
    pub execution_weight_permille: i32,
    pub documentation_weight_permille: i32,
}

#[derive(QueryableByName)]
struct MigrationVersionRow {
    #[diesel(sql_type = Text)]
    version: String,
}

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

#[derive(QueryableByName, Debug)]
struct ForeignKeyCheckRow {
    #[diesel(sql_type = Text)]
    #[allow(dead_code)]
    table: String,
    #[diesel(sql_type = Integer)]
    #[allow(dead_code)]
    rowid: i32,
    #[diesel(sql_type = Text)]
    #[allow(dead_code)]
    parent: String,
    #[diesel(sql_type = Integer)]
    #[allow(dead_code)]
    fkid: i32,
}

fn read_applied_migrations(conn: &mut SqliteConnection) -> Result<Vec<String>, BackupError> {
    let versions: Vec<MigrationVersionRow> =
        sql_query("SELECT version FROM __diesel_schema_migrations ORDER BY version ASC")
            .load(conn)?;
    Ok(versions.into_iter().map(|row| row.version).collect())
}

fn read_table_count(conn: &mut SqliteConnection, table: &str) -> Result<i64, BackupError> {
    let sql = format!("SELECT COUNT(*) AS count FROM {table}");
    let rows: Vec<CountRow> = sql_query(sql).load(conn)?;
    rows.first()
        .map(|row| row.count)
        .ok_or_else(|| BackupError::Invalid("count query returned no rows".to_string()))
}

fn clear_all_tables(conn: &mut SqliteConnection) -> Result<u64, BackupError> {
    // Delete children before parents to satisfy FK constraints with FK checks enabled.
    let tables = [
        "trade_grades",
        "logs",
        "transactions",
        "trades",
        "orders",
        "trades_balances",
        "trading_vehicles",
        "accounts_balances",
        "rules",
        "level_changes",
        "levels",
        "accounts",
    ];
    let mut cleared: u64 = 0;
    for table in tables {
        let sql = format!("DELETE FROM {table}");
        let affected = sql_query(sql).execute(conn)?;
        cleared = cleared.saturating_add(affected as u64);
    }
    Ok(cleared)
}

fn insert_all(conn: &mut SqliteConnection, tables: &BackupTablesV1) -> Result<u64, BackupError> {
    let mut inserted: u64 = 0;

    macro_rules! insert_and_add {
        ($table:expr, $values:expr) => {{
            let n = diesel::insert_into($table).values($values).execute(conn)?;
            inserted = inserted.saturating_add(n as u64);
        }};
    }

    insert_and_add!(schema::accounts::table, &tables.accounts);
    insert_and_add!(schema::trading_vehicles::table, &tables.trading_vehicles);
    insert_and_add!(schema::orders::table, &tables.orders);
    insert_and_add!(schema::trades_balances::table, &tables.trades_balances);
    insert_and_add!(schema::trades::table, &tables.trades);
    insert_and_add!(schema::accounts_balances::table, &tables.accounts_balances);
    insert_and_add!(schema::rules::table, &tables.rules);
    insert_and_add!(schema::transactions::table, &tables.transactions);
    insert_and_add!(schema::logs::table, &tables.logs);
    insert_and_add!(schema::levels::table, &tables.levels);
    insert_and_add!(schema::level_changes::table, &tables.level_changes);
    insert_and_add!(schema::trade_grades::table, &tables.trade_grades);

    Ok(inserted)
}

fn validate_foreign_keys(conn: &mut SqliteConnection) -> Result<(), BackupError> {
    let violations: Vec<ForeignKeyCheckRow> = sql_query("PRAGMA foreign_key_check").load(conn)?;
    if !violations.is_empty() {
        return Err(BackupError::Invalid(format!(
            "foreign key violations detected: {violations:?}"
        )));
    }
    Ok(())
}

fn read_backup_at(
    conn: &mut SqliteConnection,
    exported_at: DateTime<Utc>,
) -> Result<BackupEnvelopeV1, BackupError> {
    Ok(BackupEnvelopeV1 {
        format: BACKUP_FORMAT.to_string(),
        version: BACKUP_VERSION_V1,
        exported_at,
        schema: BackupSchema {
            diesel_migrations: read_applied_migrations(conn)?,
        },
        tables: BackupTablesV1 {
            accounts: schema::accounts::table.load(conn)?,
            accounts_balances: schema::accounts_balances::table.load(conn)?,
            rules: schema::rules::table.load(conn)?,
            transactions: schema::transactions::table.load(conn)?,
            trading_vehicles: schema::trading_vehicles::table.load(conn)?,
            orders: schema::orders::table.load(conn)?,
            trades_balances: schema::trades_balances::table.load(conn)?,
            trades: schema::trades::table.load(conn)?,
            logs: schema::logs::table.load(conn)?,
            levels: schema::levels::table.load(conn)?,
            level_changes: schema::level_changes::table.load(conn)?,
            trade_grades: schema::trade_grades::table.load(conn)?,
        },
    })
}

/// Read a full backup snapshot from the given connection.
pub fn read_backup(conn: &mut SqliteConnection) -> Result<BackupEnvelopeV1, BackupError> {
    read_backup_at(conn, Utc::now())
}

/// Export a full backup to a writer as pretty JSON.
pub fn export_to_writer(
    conn: &mut SqliteConnection,
    mut writer: impl Write,
) -> Result<(), BackupError> {
    let backup = read_backup(conn)?;
    serde_json::to_writer_pretty(&mut writer, &backup)?;
    writer.write_all(b"\n")?;
    Ok(())
}

/// Export a full backup to a JSON file.
pub fn export_to_path(conn: &mut SqliteConnection, path: &Path) -> Result<(), BackupError> {
    let file = File::create(path)?;
    export_to_writer(conn, file)
}

/// Load a backup envelope from a reader.
pub fn read_backup_from_reader(mut reader: impl Read) -> Result<BackupEnvelopeV1, BackupError> {
    let mut buf = String::new();
    reader.read_to_string(&mut buf)?;
    let backup: BackupEnvelopeV1 = serde_json::from_str(&buf)?;
    Ok(backup)
}

/// Load a backup envelope from a JSON file.
pub fn read_backup_from_path(path: &Path) -> Result<BackupEnvelopeV1, BackupError> {
    let file = File::open(path)?;
    read_backup_from_reader(file)
}

fn validate_backup_metadata(
    conn: &mut SqliteConnection,
    backup: &BackupEnvelopeV1,
) -> Result<(), BackupError> {
    if backup.format != BACKUP_FORMAT {
        return Err(BackupError::Invalid(format!(
            "unsupported backup format: {}",
            backup.format
        )));
    }
    if backup.version != BACKUP_VERSION_V1 {
        return Err(BackupError::Invalid(format!(
            "unsupported backup version: {}",
            backup.version
        )));
    }

    let current = read_applied_migrations(conn)?;
    if current != backup.schema.diesel_migrations {
        return Err(BackupError::Invalid(format!(
            "migration mismatch: target={current:?} backup={:?}",
            backup.schema.diesel_migrations
        )));
    }
    Ok(())
}

/// Import a backup into the connected DB.
///
/// This operation is atomic: it either fully succeeds or rolls back without partial state.
pub fn import_backup(
    conn: &mut SqliteConnection,
    backup: &BackupEnvelopeV1,
    options: ImportOptions,
) -> Result<ImportReport, BackupError> {
    validate_backup_metadata(conn, backup)?;

    if options.dry_run {
        return Ok(ImportReport {
            inserted_rows: 0,
            cleared_rows: 0,
        });
    }

    conn.transaction(|tx| {
        let mut cleared_rows: u64 = 0;
        match options.mode {
            ImportMode::Strict => {
                // Fail early if any known table contains rows.
                let tables = [
                    "accounts",
                    "accounts_balances",
                    "rules",
                    "transactions",
                    "trading_vehicles",
                    "orders",
                    "trades_balances",
                    "trades",
                    "logs",
                    "levels",
                    "level_changes",
                    "trade_grades",
                ];
                for table in tables {
                    let count = read_table_count(tx, table)?;
                    if count != 0 {
                        return Err(BackupError::Invalid(format!(
                            "strict import requires empty DB; table '{table}' has {count} rows"
                        )));
                    }
                }
            }
            ImportMode::Replace => {
                cleared_rows = clear_all_tables(tx)?;
            }
        }

        let inserted_rows = insert_all(tx, &backup.tables)?;
        validate_foreign_keys(tx)?;
        Ok(ImportReport {
            inserted_rows,
            cleared_rows,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn establish() -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        sql_query("PRAGMA foreign_keys=ON;")
            .execute(&mut conn)
            .unwrap();
        conn
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn roundtrip_export_import_preserves_all_rows() {
        let mut conn1 = establish();

        let t0 = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let dt0 = t0.and_hms_opt(0, 0, 0).unwrap();

        let account_id = "00000000-0000-0000-0000-000000000001".to_string();
        let tv_id = "00000000-0000-0000-0000-000000000002".to_string();
        let order_stop_id = "00000000-0000-0000-0000-000000000003".to_string();
        let order_entry_id = "00000000-0000-0000-0000-000000000004".to_string();
        let order_target_id = "00000000-0000-0000-0000-000000000005".to_string();
        let trade_balance_id = "00000000-0000-0000-0000-000000000006".to_string();
        let trade_id = "00000000-0000-0000-0000-000000000007".to_string();
        let log_id = "00000000-0000-0000-0000-000000000008".to_string();
        let account_balance_id = "00000000-0000-0000-0000-000000000009".to_string();
        let rule_id = "00000000-0000-0000-0000-000000000010".to_string();
        let tx_id = "00000000-0000-0000-0000-000000000011".to_string();
        let level_id = "00000000-0000-0000-0000-000000000012".to_string();
        let level_change_id = "00000000-0000-0000-0000-000000000013".to_string();
        let trade_grade_id = "00000000-0000-0000-0000-000000000014".to_string();

        diesel::insert_into(schema::accounts::table)
            .values(AccountRow {
                id: account_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: None,
                name: "acct".to_string(),
                description: "acct".to_string(),
                environment: "paper".to_string(),
                taxes_percentage: "0".to_string(),
                earnings_percentage: "0".to_string(),
                account_type: "primary".to_string(),
                parent_account_id: None,
            })
            .execute(&mut conn1)
            .unwrap();

        diesel::insert_into(schema::trading_vehicles::table)
            .values(TradingVehicleRow {
                id: tv_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: None,
                symbol: "AAPL".to_string(),
                isin: Some("US0378331005".to_string()),
                category: "stock".to_string(),
                broker: "alpaca".to_string(),
                broker_asset_id: Some("asset-1".to_string()),
                exchange: Some("NASDAQ".to_string()),
                broker_asset_class: Some("us_equity".to_string()),
                broker_asset_status: Some("active".to_string()),
                tradable: Some(true),
                marginable: Some(true),
                shortable: Some(false),
                easy_to_borrow: Some(false),
                fractionable: Some(true),
            })
            .execute(&mut conn1)
            .unwrap();

        let mk_order = |id: String, category: &str| OrderRow {
            id,
            broker_order_id: None,
            created_at: dt0,
            updated_at: dt0,
            deleted_at: None,
            unit_price: "10.0".to_string(),
            currency: "USD".to_string(),
            quantity: 1,
            category: category.to_string(),
            trading_vehicle_id: tv_id.clone(),
            action: "buy".to_string(),
            status: "new".to_string(),
            time_in_force: "day".to_string(),
            trailing_percentage: None,
            trailing_price: None,
            filled_quantity: None,
            average_filled_price: None,
            extended_hours: false,
            submitted_at: None,
            filled_at: None,
            expired_at: None,
            cancelled_at: None,
            closed_at: None,
        };

        diesel::insert_into(schema::orders::table)
            .values(vec![
                mk_order(order_stop_id.clone(), "stop"),
                mk_order(order_entry_id.clone(), "limit"),
                mk_order(order_target_id.clone(), "limit"),
            ])
            .execute(&mut conn1)
            .unwrap();

        diesel::insert_into(schema::trades_balances::table)
            .values(TradeBalanceRow {
                id: trade_balance_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: None,
                currency: "USD".to_string(),
                funding: "0".to_string(),
                capital_in_market: "0".to_string(),
                capital_out_market: "0".to_string(),
                taxed: "0".to_string(),
                total_performance: "0".to_string(),
            })
            .execute(&mut conn1)
            .unwrap();

        diesel::insert_into(schema::trades::table)
            .values(TradeRow {
                id: trade_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: None,
                category: "long".to_string(),
                status: "new".to_string(),
                currency: "USD".to_string(),
                trading_vehicle_id: tv_id.clone(),
                safety_stop_id: order_stop_id.clone(),
                entry_id: order_entry_id.clone(),
                target_id: order_target_id.clone(),
                account_id: account_id.clone(),
                balance_id: trade_balance_id.clone(),
                thesis: Some("thesis".to_string()),
                sector: Some("tech".to_string()),
                asset_class: Some("equity".to_string()),
                context: Some("context".to_string()),
            })
            .execute(&mut conn1)
            .unwrap();

        diesel::insert_into(schema::logs::table)
            .values(LogRow {
                id: log_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: None,
                log: "hello".to_string(),
                trade_id: trade_id.clone(),
            })
            .execute(&mut conn1)
            .unwrap();

        diesel::insert_into(schema::accounts_balances::table)
            .values(AccountBalanceRow {
                id: account_balance_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: None,
                account_id: account_id.clone(),
                total_balance: "100".to_string(),
                total_in_trade: "0".to_string(),
                total_available: "100".to_string(),
                taxed: "0".to_string(),
                currency: "USD".to_string(),
                total_earnings: "0".to_string(),
            })
            .execute(&mut conn1)
            .unwrap();

        diesel::insert_into(schema::rules::table)
            .values(RuleRow {
                id: rule_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: Some(dt0),
                name: "risk_per_trade".to_string(),
                risk: 2,
                description: "desc".to_string(),
                priority: 1,
                level: "error".to_string(),
                account_id: account_id.clone(),
                active: true,
            })
            .execute(&mut conn1)
            .unwrap();

        diesel::insert_into(schema::transactions::table)
            .values(TransactionRow {
                id: tx_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: Some(dt0),
                currency: "USD".to_string(),
                category: "deposit".to_string(),
                amount: "100".to_string(),
                account_id: account_id.clone(),
                trade_id: Some(trade_id.clone()),
            })
            .execute(&mut conn1)
            .unwrap();

        diesel::insert_into(schema::levels::table)
            .values(LevelRow {
                id: level_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: None,
                account_id: account_id.clone(),
                current_level: 3,
                risk_multiplier: "1.0".to_string(),
                status: "normal".to_string(),
                trades_at_level: 0,
                level_start_date: t0,
            })
            .execute(&mut conn1)
            .unwrap();

        diesel::insert_into(schema::level_changes::table)
            .values(LevelChangeRow {
                id: level_change_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: None,
                account_id: account_id.clone(),
                old_level: 3,
                new_level: 2,
                change_reason: "reason".to_string(),
                trigger_type: "manual_override".to_string(),
                changed_at: dt0,
            })
            .execute(&mut conn1)
            .unwrap();

        diesel::insert_into(schema::trade_grades::table)
            .values(TradeGradeRow {
                id: trade_grade_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: None,
                trade_id: trade_id.clone(),
                overall_score: 80,
                overall_grade: "B-".to_string(),
                process_score: 80,
                risk_score: 80,
                execution_score: 80,
                documentation_score: 80,
                recommendations: Some("[\"a\",\"b\"]".to_string()),
                graded_at: dt0,
                process_weight_permille: 400,
                risk_weight_permille: 300,
                execution_weight_permille: 200,
                documentation_weight_permille: 100,
            })
            .execute(&mut conn1)
            .unwrap();

        let fixed_exported_at = DateTime::<Utc>::from_naive_utc_and_offset(dt0, Utc);
        let backup1 = read_backup_at(&mut conn1, fixed_exported_at).unwrap();

        let mut json = Vec::new();
        serde_json::to_writer_pretty(&mut json, &backup1).unwrap();

        let backup_from_json = read_backup_from_reader(&json[..]).unwrap();
        assert_eq!(backup_from_json.format, BACKUP_FORMAT);
        assert_eq!(backup_from_json.version, BACKUP_VERSION_V1);

        let mut conn2 = establish();
        let report = import_backup(
            &mut conn2,
            &backup_from_json,
            ImportOptions {
                mode: ImportMode::Strict,
                dry_run: false,
            },
        )
        .unwrap();
        assert!(report.inserted_rows > 0);

        let backup2 = read_backup_at(&mut conn2, fixed_exported_at).unwrap();
        assert_eq!(backup1.schema, backup2.schema);
        assert_eq!(backup1.tables, backup2.tables);
    }
}
