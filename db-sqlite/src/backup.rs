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
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use uuid::Uuid;

const BACKUP_FORMAT: &str = "trust-backup";
// Version 2 requires every financial and risk-policy table; incomplete v1 exports are rejected.
const BACKUP_VERSION: u32 = 2;
const MAX_BACKUP_BYTES: usize = 16 * 1024 * 1024;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::fee_activities)]
pub struct FeeActivityRow {
    pub id: String,
    pub broker: String,
    pub account_id: String,
    pub broker_activity_id: String,
    pub broker_order_id: Option<String>,
    pub amount: String,
    pub occurred_at: chrono::NaiveDateTime,
    pub trade_id: Option<String>,
}

/// Complete version 2 JSON backup envelope. The Rust type name is retained for source compatibility.
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
    pub fee_activities: Vec<FeeActivityRow>,
    pub advisory_thresholds: Vec<AdvisoryThresholdRow>,
    pub level_adjustment_rules: Vec<LevelAdjustmentRuleRow>,
    pub distribution_history: Vec<DistributionHistoryRow>,
    pub distribution_rules: Vec<DistributionRuleRow>,
    pub executions: Vec<ExecutionRow>,
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
    pub session_plans: Vec<SessionPlanRow>,
    pub mistakes: Vec<MistakeRow>,
    pub trade_events: Vec<TradeEventRow>,
    pub trade_grades: Vec<TradeGradeRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::executions)]
pub struct ExecutionRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub broker: String,
    pub source: String,
    pub account_id: String,
    pub trade_id: Option<String>,
    pub order_id: Option<String>,
    pub broker_execution_id: String,
    pub broker_order_id: Option<String>,
    pub symbol: String,
    pub side: String,
    pub qty: String,
    pub price: String,
    pub executed_at: NaiveDateTime,
    pub raw_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::distribution_rules)]
pub struct DistributionRuleRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub account_id: String,
    pub earnings_percent: String,
    pub tax_percent: String,
    pub reinvestment_percent: String,
    pub minimum_threshold: String,
    pub configuration_password_hash: String,
    pub insurance_percent: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::distribution_history)]
pub struct DistributionHistoryRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub source_account_id: String,
    pub trade_id: Option<String>,
    pub original_amount: String,
    pub distribution_date: NaiveDateTime,
    pub earnings_amount: Option<String>,
    pub tax_amount: Option<String>,
    pub reinvestment_amount: Option<String>,
    pub insurance_amount: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::level_adjustment_rules)]
pub struct LevelAdjustmentRuleRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub account_id: String,
    pub monthly_loss_downgrade_pct: String,
    pub single_loss_downgrade_pct: String,
    pub upgrade_profitable_trades: i32,
    pub upgrade_win_rate_pct: String,
    pub upgrade_consecutive_wins: i32,
    pub cooldown_profitable_trades: i32,
    pub cooldown_win_rate_pct: String,
    pub cooldown_consecutive_wins: i32,
    pub recovery_profitable_trades: i32,
    pub recovery_win_rate_pct: String,
    pub recovery_consecutive_wins: i32,
    pub min_trades_at_level_for_upgrade: i32,
    pub max_changes_in_30_days: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::advisory_thresholds)]
pub struct AdvisoryThresholdRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub account_id: String,
    pub sector_limit_pct: String,
    pub asset_class_limit_pct: String,
    pub single_position_limit_pct: String,
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
    #[serde(default = "default_broker_kind")]
    pub broker_kind: String,
    #[serde(default)]
    pub broker_account_id: Option<String>,
}

fn default_account_type() -> String {
    "primary".to_string()
}

fn default_broker_kind() -> String {
    "alpaca".to_string()
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
    #[serde(default)]
    pub fixed_income_face_value: Option<String>,
    #[serde(default)]
    pub fixed_income_coupon_rate_pct: Option<String>,
    #[serde(default)]
    pub fixed_income_maturity_date: Option<chrono::NaiveDate>,
    #[serde(default)]
    pub fixed_income_coupon_frequency_per_year: Option<i32>,
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
    pub quantity: String,
    pub category: String,
    pub trading_vehicle_id: String,
    pub action: String,
    pub status: String,
    pub time_in_force: String,
    pub trailing_percentage: Option<String>,
    pub trailing_price: Option<String>,
    pub filled_quantity: Option<String>,
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
    pub settlement_date: Option<NaiveDate>,
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
#[diesel(table_name = schema::session_plans)]
pub struct SessionPlanRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub account_id: String,
    pub opened_at: NaiveDateTime,
    pub closed_at: Option<NaiveDateTime>,
    pub regime: String,
    pub permitted_setups: String,
    pub max_positions: i32,
    pub hypothesis: String,
    pub success_criteria: String,
    pub failure_criteria: String,
    pub session_grade: Option<String>,
    pub adherence_notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::mistakes)]
pub struct MistakeRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub trade_id: String,
    pub bias_tags: String,
    pub lollapalooza: bool,
    pub error_type: String,
    pub rule_violated: Option<String>,
    pub counterfactual_r: String,
    pub lesson: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Queryable, Insertable)]
#[diesel(table_name = schema::trade_events)]
pub struct TradeEventRow {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub trade_id: String,
    pub symbol: String,
    pub event_type: String,
    pub event_date: NaiveDate,
    pub severity: String,
    pub notes: Option<String>,
    pub source: String,
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

const ALLOWED_TABLES: &[&str] = &[
    "fee_activities",
    "advisory_thresholds",
    "level_adjustment_rules",
    "distribution_history",
    "distribution_rules",
    "executions",
    "accounts",
    "accounts_balances",
    "logs",
    "level_changes",
    "levels",
    "mistakes",
    "orders",
    "rules",
    "session_plans",
    "trade_events",
    "trade_grades",
    "trades",
    "trades_balances",
    "trading_vehicles",
    "transactions",
];

fn validate_table_name(table: &str) -> Result<(), BackupError> {
    if ALLOWED_TABLES.contains(&table) {
        Ok(())
    } else {
        Err(BackupError::Invalid(format!(
            "unknown or disallowed table name: '{table}'"
        )))
    }
}

fn read_table_count(conn: &mut SqliteConnection, table: &str) -> Result<i64, BackupError> {
    validate_table_name(table)?;
    let sql = format!("SELECT COUNT(*) AS count FROM {table}");
    let rows: Vec<CountRow> = sql_query(sql).load(conn)?;
    rows.first()
        .map(|row| row.count)
        .ok_or_else(|| BackupError::Invalid("count query returned no rows".to_string()))
}

fn clear_all_tables(conn: &mut SqliteConnection) -> Result<u64, BackupError> {
    // Delete children before parents to satisfy FK constraints with FK checks enabled.
    let tables = [
        "fee_activities",
        "advisory_thresholds",
        "level_adjustment_rules",
        "distribution_history",
        "distribution_rules",
        "executions",
        "mistakes",
        "session_plans",
        "trade_events",
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
        validate_table_name(table)?;
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
    insert_and_add!(schema::session_plans::table, &tables.session_plans);
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
    insert_and_add!(schema::mistakes::table, &tables.mistakes);
    insert_and_add!(schema::trade_events::table, &tables.trade_events);
    insert_and_add!(schema::trade_grades::table, &tables.trade_grades);

    insert_and_add!(schema::executions::table, &tables.executions);
    insert_and_add!(
        schema::distribution_rules::table,
        &tables.distribution_rules
    );
    insert_and_add!(
        schema::distribution_history::table,
        &tables.distribution_history
    );
    insert_and_add!(
        schema::level_adjustment_rules::table,
        &tables.level_adjustment_rules
    );
    insert_and_add!(
        schema::advisory_thresholds::table,
        &tables.advisory_thresholds
    );
    insert_and_add!(schema::fee_activities::table, &tables.fee_activities);
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
        version: BACKUP_VERSION,
        exported_at,
        schema: BackupSchema {
            diesel_migrations: read_applied_migrations(conn)?,
        },
        tables: BackupTablesV1 {
            fee_activities: schema::fee_activities::table.load(conn)?,
            advisory_thresholds: schema::advisory_thresholds::table.load(conn)?,
            level_adjustment_rules: schema::level_adjustment_rules::table.load(conn)?,
            distribution_history: schema::distribution_history::table.load(conn)?,
            distribution_rules: schema::distribution_rules::table.load(conn)?,
            executions: schema::executions::table.load(conn)?,
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
            session_plans: schema::session_plans::table.load(conn)?,
            mistakes: schema::mistakes::table.load(conn)?,
            trade_events: schema::trade_events::table.load(conn)?,
            trade_grades: schema::trade_grades::table.load(conn)?,
        },
    })
}

/// Read a full backup snapshot from the given connection.
pub fn read_backup(conn: &mut SqliteConnection) -> Result<BackupEnvelopeV1, BackupError> {
    conn.transaction(|tx| read_backup_at(tx, Utc::now()))
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
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary = directory.join(format!(".trust-backup-{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        export_to_writer(conn, &mut file)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Load a backup envelope from a reader.
pub fn read_backup_from_reader(mut reader: impl Read) -> Result<BackupEnvelopeV1, BackupError> {
    let max_backup_bytes = u64::try_from(MAX_BACKUP_BYTES)
        .map_err(|_| BackupError::Invalid("backup size limit is invalid".to_string()))?;
    let read_limit = max_backup_bytes
        .checked_add(1)
        .ok_or_else(|| BackupError::Invalid("backup size limit is invalid".to_string()))?;
    let mut buf = String::new();
    reader.by_ref().take(read_limit).read_to_string(&mut buf)?;
    if buf.len() > MAX_BACKUP_BYTES {
        return Err(BackupError::Invalid(format!(
            "backup JSON exceeds maximum size of {MAX_BACKUP_BYTES} bytes"
        )));
    }
    let backup: BackupEnvelopeV1 = serde_json::from_str(&buf)?;
    validate_backup_rows(&backup)?;
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
    if backup.version != BACKUP_VERSION {
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
    validate_backup_rows(backup)?;
    Ok(())
}

fn validate_uuid_field(table: &str, field: &str, value: &str) -> Result<(), BackupError> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| BackupError::Invalid(format!("invalid UUID in {table}.{field}: '{value}'")))
}

fn validate_optional_uuid_field(
    table: &str,
    field: &str,
    value: Option<&str>,
) -> Result<(), BackupError> {
    if let Some(value) = value {
        validate_uuid_field(table, field, value)?;
    }
    Ok(())
}

fn validate_backup_rows(backup: &BackupEnvelopeV1) -> Result<(), BackupError> {
    validate_account_uuid_rows(&backup.tables)?;
    validate_trade_uuid_rows(&backup.tables)?;
    validate_review_uuid_rows(&backup.tables)?;
    validate_financial_fields(&backup.tables)?;
    validate_policy_rows(&backup.tables)?;
    Ok(())
}

const DECIMAL_FIELDS: &[&str] = &[
    "amount",
    "qty",
    "price",
    "quantity",
    "unit_price",
    "filled_quantity",
    "average_filled_price",
    "funding",
    "capital_in_market",
    "capital_out_market",
    "taxed",
    "total_performance",
    "total_balance",
    "total_in_trade",
    "total_available",
    "taxes_percentage",
    "earnings_percentage",
    "trailing_percentage",
    "trailing_price",
    "risk_multiplier",
    "earnings_percent",
    "tax_percent",
    "reinvestment_percent",
    "insurance_percent",
    "minimum_threshold",
    "original_amount",
    "earnings_amount",
    "tax_amount",
    "reinvestment_amount",
    "insurance_amount",
    "sector_limit_pct",
    "asset_class_limit_pct",
    "single_position_limit_pct",
    "monthly_loss_downgrade_pct",
    "single_loss_downgrade_pct",
    "upgrade_win_rate_pct",
    "cooldown_win_rate_pct",
    "recovery_win_rate_pct",
    "fixed_income_face_value",
    "fixed_income_coupon_rate_pct",
];

fn validate_decimal_field(table: &str, field: &str, text: &str) -> Result<(), BackupError> {
    use rust_decimal::Decimal;
    let value = text
        .parse::<Decimal>()
        .map_err(|_| BackupError::Invalid(format!("Invalid decimal in {table}.{field}")))?;
    let positive = matches!(
        field,
        "qty"
            | "quantity"
            | "unit_price"
            | "price"
            | "average_filled_price"
            | "risk_multiplier"
            | "fixed_income_face_value"
    ) || (table == "fee_activities" && field == "amount");
    let nonnegative = matches!(
        field,
        "filled_quantity"
            | "funding"
            | "capital_in_market"
            | "minimum_threshold"
            | "original_amount"
            | "earnings_amount"
            | "tax_amount"
            | "reinvestment_amount"
            | "insurance_amount"
    );
    let loss_threshold = table == "level_adjustment_rules"
        && matches!(
            field,
            "monthly_loss_downgrade_pct" | "single_loss_downgrade_pct"
        );
    let percentage = !loss_threshold
        && (field.ends_with("_pct")
            || field.ends_with("_percent")
            || field.ends_with("_percentage"));
    let ceiling = if table == "distribution_rules" {
        Decimal::ONE
    } else {
        Decimal::from(100)
    };
    if (positive && value <= Decimal::ZERO)
        || (nonnegative && value < Decimal::ZERO)
        || (loss_threshold && value >= Decimal::ZERO)
        || (percentage && (value < Decimal::ZERO || value > ceiling))
    {
        return Err(BackupError::Invalid(format!(
            "Financial value outside domain bounds in {table}.{field}"
        )));
    }
    Ok(())
}

// Keep cross-field policy and ledger constraints together for restore review.
#[allow(clippy::too_many_lines)]
fn validate_policy_rows(tables: &BackupTablesV1) -> Result<(), BackupError> {
    use rust_decimal::Decimal;
    for rule in &tables.distribution_rules {
        let total = [
            &rule.earnings_percent,
            &rule.tax_percent,
            &rule.reinvestment_percent,
            &rule.insurance_percent,
        ]
        .iter()
        .try_fold(Decimal::ZERO, |total, value| {
            value
                .parse::<Decimal>()
                .ok()
                .and_then(|value| total.checked_add(value))
        });
        if total != Some(Decimal::ONE) {
            return Err(BackupError::Invalid(
                "distribution_rules percentages must sum to 1".into(),
            ));
        }
    }
    for rule in &tables.level_adjustment_rules {
        if [
            rule.upgrade_profitable_trades,
            rule.upgrade_consecutive_wins,
            rule.cooldown_profitable_trades,
            rule.cooldown_consecutive_wins,
            rule.recovery_profitable_trades,
            rule.recovery_consecutive_wins,
            rule.min_trades_at_level_for_upgrade,
            rule.max_changes_in_30_days,
        ]
        .iter()
        .any(|value| *value <= 0)
        {
            return Err(BackupError::Invalid(
                "level_adjustment_rules counts must be positive".into(),
            ));
        }
    }
    for row in &tables.orders {
        if row
            .filled_quantity
            .as_deref()
            .and_then(|value| value.parse::<Decimal>().ok())
            > row.quantity.parse::<Decimal>().ok()
        {
            return Err(BackupError::Invalid(
                "orders.filled_quantity exceeds quantity".into(),
            ));
        }
    }
    for row in &tables.transactions {
        if !matches!(
            row.category.as_str(),
            "open_trade"
                | "close_target"
                | "close_safety_stop"
                | "close_safety_stop_slippage"
                | "payment_from_trade"
        ) && row
            .amount
            .parse::<Decimal>()
            .is_ok_and(|value| value < Decimal::ZERO)
        {
            return Err(BackupError::Invalid(
                "transactions.amount is negative for an unsigned cash flow".into(),
            ));
        }
    }
    for row in &tables.fee_activities {
        if row.broker_activity_id.is_empty() {
            return Err(BackupError::Invalid(
                "fee_activities requires an activity identity".into(),
            ));
        }
    }
    Ok(())
}

fn validate_financial_fields(tables: &BackupTablesV1) -> Result<(), BackupError> {
    let value = serde_json::to_value(tables)?;
    let table_map = value
        .as_object()
        .ok_or_else(|| BackupError::Invalid("Invalid table envelope".into()))?;
    for (table, rows) in table_map {
        let rows = rows
            .as_array()
            .ok_or_else(|| BackupError::Invalid(format!("Invalid rows in {table}")))?;
        for row in rows {
            let fields = row
                .as_object()
                .ok_or_else(|| BackupError::Invalid(format!("Invalid row in {table}")))?;
            for (field, value) in fields {
                let Some(text) = value.as_str() else {
                    continue;
                };
                if field == "id"
                    || matches!(
                        field.as_str(),
                        "account_id" | "source_account_id" | "trade_id" | "order_id"
                    )
                {
                    validate_uuid_field(table, field, text)?;
                }
                if DECIMAL_FIELDS.contains(&field.as_str()) {
                    validate_decimal_field(table, field, text)?;
                }
                let valid = match (table.as_str(), field.as_str()) {
                    (_, "currency") => text.parse::<model::Currency>().is_ok(),
                    ("trades", "status") => text.parse::<model::Status>().is_ok(),
                    ("trades", "category") => text.parse::<model::TradeCategory>().is_ok(),
                    ("orders", "status") => text.parse::<model::OrderStatus>().is_ok(),
                    ("orders", "category") => text.parse::<model::OrderCategory>().is_ok(),
                    ("orders", "action") => text.parse::<model::OrderAction>().is_ok(),
                    ("orders", "time_in_force") => text.parse::<model::TimeInForce>().is_ok(),
                    ("accounts", "environment") => text.parse::<model::Environment>().is_ok(),
                    ("accounts", "account_type") => matches!(
                        text,
                        "primary" | "earnings" | "tax_reserve" | "reinvestment" | "insurance"
                    ),
                    (_, "broker_kind") | ("executions" | "fee_activities", "broker") => {
                        text.parse::<model::BrokerKind>().is_ok()
                    }
                    ("trading_vehicles", "category") => {
                        text.parse::<model::TradingVehicleCategory>().is_ok()
                    }
                    ("transactions", "category") => {
                        let trade_id = fields
                            .get("trade_id")
                            .and_then(serde_json::Value::as_str)
                            .and_then(|value| Uuid::parse_str(value).ok());
                        model::TransactionCategory::parse(text, trade_id).is_ok()
                    }
                    ("executions", "side") => text.parse::<model::ExecutionSide>().is_ok(),
                    ("executions", "source") => text.parse::<model::ExecutionSource>().is_ok(),
                    _ => true,
                };
                if !valid {
                    return Err(BackupError::Invalid(format!(
                        "Invalid domain value in {table}.{field}"
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_account_uuid_rows(tables: &BackupTablesV1) -> Result<(), BackupError> {
    for row in &tables.accounts {
        validate_uuid_field("accounts", "id", &row.id)?;
        validate_optional_uuid_field(
            "accounts",
            "parent_account_id",
            row.parent_account_id.as_deref(),
        )?;
    }
    for row in &tables.accounts_balances {
        validate_uuid_field("accounts_balances", "id", &row.id)?;
        validate_uuid_field("accounts_balances", "account_id", &row.account_id)?;
    }
    for row in &tables.rules {
        validate_uuid_field("rules", "id", &row.id)?;
        validate_uuid_field("rules", "account_id", &row.account_id)?;
    }
    for row in &tables.transactions {
        validate_uuid_field("transactions", "id", &row.id)?;
        validate_uuid_field("transactions", "account_id", &row.account_id)?;
        validate_optional_uuid_field("transactions", "trade_id", row.trade_id.as_deref())?;
    }
    for row in &tables.trading_vehicles {
        validate_uuid_field("trading_vehicles", "id", &row.id)?;
    }
    Ok(())
}

fn validate_trade_uuid_rows(tables: &BackupTablesV1) -> Result<(), BackupError> {
    for row in &tables.orders {
        validate_uuid_field("orders", "id", &row.id)?;
        validate_uuid_field("orders", "trading_vehicle_id", &row.trading_vehicle_id)?;
    }
    for row in &tables.trades_balances {
        validate_uuid_field("trades_balances", "id", &row.id)?;
    }
    for row in &tables.trades {
        validate_uuid_field("trades", "id", &row.id)?;
        validate_uuid_field("trades", "trading_vehicle_id", &row.trading_vehicle_id)?;
        validate_uuid_field("trades", "safety_stop_id", &row.safety_stop_id)?;
        validate_uuid_field("trades", "entry_id", &row.entry_id)?;
        validate_uuid_field("trades", "target_id", &row.target_id)?;
        validate_uuid_field("trades", "account_id", &row.account_id)?;
        validate_uuid_field("trades", "balance_id", &row.balance_id)?;
    }
    for row in &tables.logs {
        validate_uuid_field("logs", "id", &row.id)?;
        validate_uuid_field("logs", "trade_id", &row.trade_id)?;
    }
    Ok(())
}

fn validate_review_uuid_rows(tables: &BackupTablesV1) -> Result<(), BackupError> {
    for row in &tables.levels {
        validate_uuid_field("levels", "id", &row.id)?;
        validate_uuid_field("levels", "account_id", &row.account_id)?;
    }
    for row in &tables.level_changes {
        validate_uuid_field("level_changes", "id", &row.id)?;
        validate_uuid_field("level_changes", "account_id", &row.account_id)?;
    }
    for row in &tables.session_plans {
        validate_uuid_field("session_plans", "id", &row.id)?;
        validate_uuid_field("session_plans", "account_id", &row.account_id)?;
    }
    for row in &tables.mistakes {
        validate_uuid_field("mistakes", "id", &row.id)?;
        validate_uuid_field("mistakes", "trade_id", &row.trade_id)?;
    }
    for row in &tables.trade_events {
        validate_uuid_field("trade_events", "id", &row.id)?;
        validate_uuid_field("trade_events", "trade_id", &row.trade_id)?;
    }
    for row in &tables.trade_grades {
        validate_uuid_field("trade_grades", "id", &row.id)?;
        validate_uuid_field("trade_grades", "trade_id", &row.trade_id)?;
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

    let result = conn.transaction(|tx| {
        let mut cleared_rows: u64 = 0;
        match options.mode {
            ImportMode::Strict => {
                // Fail early if any known table contains rows.
                for table in ALLOWED_TABLES {
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
        if options.dry_run {
            return Err(BackupError::Diesel(
                diesel::result::Error::RollbackTransaction,
            ));
        }
        Ok(ImportReport {
            inserted_rows,
            cleared_rows,
        })
    });
    match result {
        Err(BackupError::Diesel(diesel::result::Error::RollbackTransaction)) if options.dry_run => {
            Ok(ImportReport {
                inserted_rows: 0,
                cleared_rows: 0,
            })
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
    use std::path::PathBuf;

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn establish() -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        sql_query("PRAGMA foreign_keys=ON;")
            .execute(&mut conn)
            .unwrap();
        conn
    }

    fn fixed_dt() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2020, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
    }

    fn fixed_exported_at() -> DateTime<Utc> {
        DateTime::<Utc>::from_naive_utc_and_offset(fixed_dt(), Utc)
    }

    fn uuid_text(n: u64) -> String {
        format!("00000000-0000-0000-0000-{n:012}")
    }

    fn account_row(id: String, name: &str) -> AccountRow {
        let dt = fixed_dt();
        AccountRow {
            id,
            created_at: dt,
            updated_at: dt,
            deleted_at: None,
            name: name.to_string(),
            description: name.to_string(),
            environment: "paper".to_string(),
            taxes_percentage: "0".to_string(),
            earnings_percentage: "0".to_string(),
            account_type: "primary".to_string(),
            parent_account_id: None,
            broker_kind: "alpaca".to_string(),
            broker_account_id: None,
        }
    }

    fn insert_account(conn: &mut SqliteConnection, id: String, name: &str) {
        diesel::insert_into(schema::accounts::table)
            .values(account_row(id, name))
            .execute(conn)
            .expect("account row should insert");
    }

    fn empty_backup(conn: &mut SqliteConnection) -> BackupEnvelopeV1 {
        read_backup_at(conn, fixed_exported_at()).expect("empty backup should read")
    }

    fn temp_backup_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "trust-backup-test-{}-{}.json",
            std::process::id(),
            Uuid::new_v4()
        ))
    }

    fn invalid_uuid() -> String {
        "not-a-uuid".to_string()
    }

    fn first_row<T>(rows: &mut [T]) -> &mut T {
        rows.first_mut()
            .expect("validation fixture should include one row")
    }

    #[allow(clippy::too_many_lines)]
    fn backup_with_all_uuid_fields() -> BackupEnvelopeV1 {
        let date = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let dt = date.and_hms_opt(0, 0, 0).unwrap();
        let account_id = uuid_text(701);
        let parent_account_id = uuid_text(702);
        let account_balance_id = uuid_text(703);
        let rule_id = uuid_text(704);
        let transaction_id = uuid_text(705);
        let transaction_trade_id = uuid_text(706);
        let trading_vehicle_id = uuid_text(707);
        let order_id = uuid_text(708);
        let trade_balance_id = uuid_text(709);
        let trade_id = uuid_text(710);
        let safety_stop_id = uuid_text(711);
        let entry_id = uuid_text(712);
        let target_id = uuid_text(713);
        let log_id = uuid_text(714);
        let level_id = uuid_text(715);
        let level_change_id = uuid_text(716);
        let trade_grade_id = uuid_text(717);
        let trade_event_id = uuid_text(718);
        let mistake_id = uuid_text(719);
        let session_plan_id = uuid_text(720);

        let mut account = account_row(account_id.clone(), "uuid-validation");
        account.parent_account_id = Some(parent_account_id);

        BackupEnvelopeV1 {
            format: BACKUP_FORMAT.to_string(),
            version: BACKUP_VERSION,
            exported_at: fixed_exported_at(),
            schema: BackupSchema {
                diesel_migrations: Vec::new(),
            },
            tables: BackupTablesV1 {
                fee_activities: Vec::new(),
                advisory_thresholds: Vec::new(),
                level_adjustment_rules: Vec::new(),
                distribution_history: Vec::new(),
                distribution_rules: Vec::new(),
                executions: Vec::new(),
                accounts: vec![account],
                accounts_balances: vec![AccountBalanceRow {
                    id: account_balance_id,
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    account_id: account_id.clone(),
                    total_balance: "100".to_string(),
                    total_in_trade: "0".to_string(),
                    total_available: "100".to_string(),
                    taxed: "0".to_string(),
                    currency: "USD".to_string(),
                    total_earnings: "0".to_string(),
                }],
                rules: vec![RuleRow {
                    id: rule_id,
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    name: "risk_per_trade".to_string(),
                    risk: 1,
                    description: "risk rule".to_string(),
                    priority: 1,
                    level: "normal".to_string(),
                    account_id: account_id.clone(),
                    active: true,
                }],
                transactions: vec![TransactionRow {
                    id: transaction_id,
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    currency: "USD".to_string(),
                    category: "deposit".to_string(),
                    amount: "100".to_string(),
                    account_id: account_id.clone(),
                    trade_id: Some(transaction_trade_id),
                }],
                trading_vehicles: vec![TradingVehicleRow {
                    id: trading_vehicle_id.clone(),
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    symbol: "AAPL".to_string(),
                    isin: None,
                    category: "stock".to_string(),
                    broker: "alpaca".to_string(),
                    broker_asset_id: None,
                    exchange: None,
                    broker_asset_class: None,
                    broker_asset_status: None,
                    tradable: None,
                    marginable: None,
                    shortable: None,
                    easy_to_borrow: None,
                    fractionable: None,
                    fixed_income_face_value: None,
                    fixed_income_coupon_rate_pct: None,
                    fixed_income_maturity_date: None,
                    fixed_income_coupon_frequency_per_year: None,
                }],
                orders: vec![OrderRow {
                    id: order_id,
                    broker_order_id: None,
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    unit_price: "10".to_string(),
                    currency: "USD".to_string(),
                    quantity: "1".to_string(),
                    category: "limit".to_string(),
                    trading_vehicle_id: trading_vehicle_id.clone(),
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
                }],
                trades_balances: vec![TradeBalanceRow {
                    id: trade_balance_id.clone(),
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    currency: "USD".to_string(),
                    funding: "100".to_string(),
                    capital_in_market: "0".to_string(),
                    capital_out_market: "0".to_string(),
                    taxed: "0".to_string(),
                    total_performance: "0".to_string(),
                }],
                trades: vec![TradeRow {
                    id: trade_id.clone(),
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    category: "long".to_string(),
                    status: "new".to_string(),
                    currency: "USD".to_string(),
                    trading_vehicle_id,
                    safety_stop_id,
                    entry_id,
                    target_id,
                    account_id: account_id.clone(),
                    balance_id: trade_balance_id,
                    thesis: None,
                    sector: None,
                    asset_class: None,
                    context: None,
                    settlement_date: Some(date),
                }],
                logs: vec![LogRow {
                    id: log_id,
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    log: "created".to_string(),
                    trade_id: trade_id.clone(),
                }],
                levels: vec![LevelRow {
                    id: level_id,
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    account_id: account_id.clone(),
                    current_level: 1,
                    risk_multiplier: "1".to_string(),
                    status: "active".to_string(),
                    trades_at_level: 0,
                    level_start_date: date,
                }],
                level_changes: vec![LevelChangeRow {
                    id: level_change_id,
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    account_id: account_id.clone(),
                    old_level: 1,
                    new_level: 2,
                    change_reason: "manual".to_string(),
                    trigger_type: "manual_override".to_string(),
                    changed_at: dt,
                }],
                session_plans: vec![SessionPlanRow {
                    id: session_plan_id,
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    account_id: account_id.clone(),
                    opened_at: dt,
                    closed_at: Some(dt),
                    regime: "normal".to_string(),
                    permitted_setups: "opening range,pullback".to_string(),
                    max_positions: 2,
                    hypothesis: "follow only planned setups".to_string(),
                    success_criteria: "take valid setups only".to_string(),
                    failure_criteria: "force trades outside plan".to_string(),
                    session_grade: Some("A".to_string()),
                    adherence_notes: Some("followed plan".to_string()),
                }],
                mistakes: vec![MistakeRow {
                    id: mistake_id,
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    trade_id: trade_id.clone(),
                    bias_tags: "5,14".to_string(),
                    lollapalooza: true,
                    error_type: "commission".to_string(),
                    rule_violated: Some("risk_rule".to_string()),
                    counterfactual_r: "1.25".to_string(),
                    lesson: "Follow the rule.".to_string(),
                }],
                trade_events: vec![TradeEventRow {
                    id: trade_event_id,
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    trade_id: trade_id.clone(),
                    symbol: "AAPL".to_string(),
                    event_type: "earnings".to_string(),
                    event_date: date,
                    severity: "high".to_string(),
                    notes: Some("watch earnings".to_string()),
                    source: "manual".to_string(),
                }],
                trade_grades: vec![TradeGradeRow {
                    id: trade_grade_id,
                    created_at: dt,
                    updated_at: dt,
                    deleted_at: None,
                    trade_id,
                    overall_score: 75,
                    overall_grade: "B".to_string(),
                    process_score: 75,
                    risk_score: 75,
                    execution_score: 75,
                    documentation_score: 75,
                    recommendations: None,
                    graded_at: dt,
                    process_weight_permille: 400,
                    risk_weight_permille: 300,
                    execution_weight_permille: 200,
                    documentation_weight_permille: 100,
                }],
            },
        }
    }

    #[test]
    fn backup_table_manifest_covers_the_entire_application_schema() {
        #[derive(QueryableByName)]
        struct TableName {
            #[diesel(sql_type = Text)]
            name: String,
        }
        let mut conn = establish();
        let mut actual: Vec<String> = sql_query("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name != '__diesel_schema_migrations'")
            .load::<TableName>(&mut conn).unwrap().into_iter().map(|row| row.name).collect();
        actual.sort();
        let mut expected: Vec<String> = ALLOWED_TABLES
            .iter()
            .map(|name| (*name).to_string())
            .collect();
        expected.sort();
        assert_eq!(actual, expected);
        let backup = read_backup(&mut conn).unwrap();
        let json = serde_json::to_value(&backup.tables).unwrap();
        let mut exported: Vec<String> = json.as_object().unwrap().keys().cloned().collect();
        exported.sort();
        assert_eq!(actual, exported);
    }

    #[test]
    fn dry_run_checks_foreign_keys_and_strict_emptiness_without_changes() {
        let mut conn = establish();
        let mut backup = read_backup(&mut conn).unwrap();
        backup.tables.fee_activities.push(FeeActivityRow {
            id: uuid_text(990),
            broker: "alpaca".into(),
            account_id: uuid_text(991),
            broker_activity_id: "fee-1".into(),
            broker_order_id: None,
            amount: "1".into(),
            occurred_at: fixed_dt(),
            trade_id: None,
        });
        assert!(import_backup(
            &mut conn,
            &backup,
            ImportOptions {
                mode: ImportMode::Strict,
                dry_run: true
            }
        )
        .is_err());
        assert_eq!(read_table_count(&mut conn, "fee_activities").unwrap(), 0);
        backup.tables.fee_activities.clear();
        insert_account(&mut conn, uuid_text(992), "existing");
        assert!(import_backup(
            &mut conn,
            &backup,
            ImportOptions {
                mode: ImportMode::Strict,
                dry_run: true
            }
        )
        .is_err());
        assert_eq!(read_table_count(&mut conn, "accounts").unwrap(), 1);
    }

    #[test]
    fn invalid_financial_values_are_rejected_before_restore() {
        let mut conn = establish();
        let mut backup = read_backup(&mut conn).unwrap();
        let mut account = account_row(uuid_text(993), "invalid decimal");
        account.taxes_percentage = "NaN".into();
        backup.tables.accounts.push(account);
        assert!(import_backup(
            &mut conn,
            &backup,
            ImportOptions {
                mode: ImportMode::Replace,
                dry_run: true
            }
        )
        .is_err());
        assert_eq!(read_table_count(&mut conn, "accounts").unwrap(), 0);
    }

    #[test]
    fn import_options_and_legacy_account_defaults_are_stable() {
        let options = ImportOptions::default();
        assert_eq!(options.mode, ImportMode::Strict);
        assert!(!options.dry_run);

        let account: AccountRow = serde_json::from_value(serde_json::json!({
            "id": uuid_text(801),
            "created_at": "2026-01-01T00:00:00",
            "updated_at": "2026-01-01T00:00:00",
            "deleted_at": null,
            "name": "legacy",
            "description": "legacy account",
            "environment": "paper",
            "taxes_percentage": "0",
            "earnings_percentage": "0"
        }))
        .expect("legacy account row should deserialize with schema defaults");

        assert_eq!(account.account_type, default_account_type());
        assert_eq!(account.parent_account_id, None);
        assert_eq!(account.broker_kind, default_broker_kind());
        assert_eq!(account.broker_account_id, None);
    }

    #[derive(Clone, Copy)]
    struct UuidValidationCase {
        table: &'static str,
        field: &'static str,
        mutate: fn(&mut BackupEnvelopeV1),
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn backup_row_validation_reports_invalid_uuid_fields() {
        let cases = [
            UuidValidationCase {
                table: "accounts",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.accounts).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "accounts",
                field: "parent_account_id",
                mutate: |backup| {
                    first_row(&mut backup.tables.accounts).parent_account_id = Some(invalid_uuid());
                },
            },
            UuidValidationCase {
                table: "accounts_balances",
                field: "id",
                mutate: |backup| {
                    first_row(&mut backup.tables.accounts_balances).id = invalid_uuid();
                },
            },
            UuidValidationCase {
                table: "accounts_balances",
                field: "account_id",
                mutate: |backup| {
                    first_row(&mut backup.tables.accounts_balances).account_id = invalid_uuid();
                },
            },
            UuidValidationCase {
                table: "rules",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.rules).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "rules",
                field: "account_id",
                mutate: |backup| first_row(&mut backup.tables.rules).account_id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "transactions",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.transactions).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "transactions",
                field: "account_id",
                mutate: |backup| {
                    first_row(&mut backup.tables.transactions).account_id = invalid_uuid();
                },
            },
            UuidValidationCase {
                table: "transactions",
                field: "trade_id",
                mutate: |backup| {
                    first_row(&mut backup.tables.transactions).trade_id = Some(invalid_uuid());
                },
            },
            UuidValidationCase {
                table: "trading_vehicles",
                field: "id",
                mutate: |backup| {
                    first_row(&mut backup.tables.trading_vehicles).id = invalid_uuid();
                },
            },
            UuidValidationCase {
                table: "orders",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.orders).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "orders",
                field: "trading_vehicle_id",
                mutate: |backup| {
                    first_row(&mut backup.tables.orders).trading_vehicle_id = invalid_uuid();
                },
            },
            UuidValidationCase {
                table: "trades_balances",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.trades_balances).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "trades",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.trades).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "trades",
                field: "trading_vehicle_id",
                mutate: |backup| {
                    first_row(&mut backup.tables.trades).trading_vehicle_id = invalid_uuid();
                },
            },
            UuidValidationCase {
                table: "trades",
                field: "safety_stop_id",
                mutate: |backup| {
                    first_row(&mut backup.tables.trades).safety_stop_id = invalid_uuid();
                },
            },
            UuidValidationCase {
                table: "trades",
                field: "entry_id",
                mutate: |backup| first_row(&mut backup.tables.trades).entry_id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "trades",
                field: "target_id",
                mutate: |backup| first_row(&mut backup.tables.trades).target_id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "trades",
                field: "account_id",
                mutate: |backup| first_row(&mut backup.tables.trades).account_id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "trades",
                field: "balance_id",
                mutate: |backup| first_row(&mut backup.tables.trades).balance_id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "logs",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.logs).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "logs",
                field: "trade_id",
                mutate: |backup| first_row(&mut backup.tables.logs).trade_id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "levels",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.levels).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "levels",
                field: "account_id",
                mutate: |backup| first_row(&mut backup.tables.levels).account_id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "level_changes",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.level_changes).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "level_changes",
                field: "account_id",
                mutate: |backup| {
                    first_row(&mut backup.tables.level_changes).account_id = invalid_uuid();
                },
            },
            UuidValidationCase {
                table: "session_plans",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.session_plans).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "session_plans",
                field: "account_id",
                mutate: |backup| {
                    first_row(&mut backup.tables.session_plans).account_id = invalid_uuid();
                },
            },
            UuidValidationCase {
                table: "mistakes",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.mistakes).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "mistakes",
                field: "trade_id",
                mutate: |backup| first_row(&mut backup.tables.mistakes).trade_id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "trade_events",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.trade_events).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "trade_events",
                field: "trade_id",
                mutate: |backup| {
                    first_row(&mut backup.tables.trade_events).trade_id = invalid_uuid();
                },
            },
            UuidValidationCase {
                table: "trade_grades",
                field: "id",
                mutate: |backup| first_row(&mut backup.tables.trade_grades).id = invalid_uuid(),
            },
            UuidValidationCase {
                table: "trade_grades",
                field: "trade_id",
                mutate: |backup| {
                    first_row(&mut backup.tables.trade_grades).trade_id = invalid_uuid();
                },
            },
        ];

        for case in cases {
            let mut backup = backup_with_all_uuid_fields();
            (case.mutate)(&mut backup);

            let error =
                validate_backup_rows(&backup).expect_err("invalid backup UUID should be rejected");
            let expected = format!("{}.{}", case.table, case.field);
            assert!(
                error.to_string().contains(&expected),
                "expected error to mention {expected}, got {error}"
            );
        }
    }

    #[allow(clippy::too_many_lines)]
    fn insert_extended_backup_rows(
        conn: &mut SqliteConnection,
        account: &str,
        trade: &str,
        order: &str,
    ) {
        let dt = fixed_dt();
        diesel::insert_into(schema::executions::table)
            .values(ExecutionRow {
                id: uuid_text(701),
                created_at: dt,
                updated_at: dt,
                deleted_at: Some(dt),
                broker: "alpaca".into(),
                source: "account_activities".into(),
                account_id: account.into(),
                trade_id: Some(trade.into()),
                order_id: Some(order.into()),
                broker_execution_id: "backup-execution".into(),
                broker_order_id: Some("broker-order".into()),
                symbol: "AAPL".into(),
                side: "buy".into(),
                qty: "2.5".into(),
                price: "100.125".into(),
                executed_at: dt,
                raw_json: Some("{}".into()),
            })
            .execute(conn)
            .unwrap();
        diesel::insert_into(schema::fee_activities::table)
            .values(FeeActivityRow {
                id: uuid_text(702),
                broker: "alpaca".into(),
                account_id: account.into(),
                broker_activity_id: "backup-fee".into(),
                broker_order_id: Some("broker-order".into()),
                amount: "0.0123".into(),
                occurred_at: dt,
                trade_id: Some(trade.into()),
            })
            .execute(conn)
            .unwrap();
        diesel::insert_into(schema::distribution_rules::table)
            .values(DistributionRuleRow {
                id: uuid_text(703),
                created_at: dt,
                updated_at: dt,
                account_id: account.into(),
                earnings_percent: "0.2".into(),
                tax_percent: "0.25".into(),
                reinvestment_percent: "0.5".into(),
                minimum_threshold: "10".into(),
                configuration_password_hash: "fixture-hash".into(),
                insurance_percent: "0.05".into(),
            })
            .execute(conn)
            .unwrap();
        diesel::insert_into(schema::distribution_history::table)
            .values(DistributionHistoryRow {
                id: uuid_text(704),
                created_at: dt,
                updated_at: dt,
                source_account_id: account.into(),
                trade_id: Some(trade.into()),
                original_amount: "100".into(),
                distribution_date: dt,
                earnings_amount: Some("20".into()),
                tax_amount: Some("25".into()),
                reinvestment_amount: Some("50".into()),
                insurance_amount: Some("5".into()),
            })
            .execute(conn)
            .unwrap();
        diesel::insert_into(schema::advisory_thresholds::table)
            .values(AdvisoryThresholdRow {
                id: uuid_text(705),
                created_at: dt,
                updated_at: dt,
                account_id: account.into(),
                sector_limit_pct: "30".into(),
                asset_class_limit_pct: "60".into(),
                single_position_limit_pct: "10".into(),
            })
            .execute(conn)
            .unwrap();
        diesel::insert_into(schema::level_adjustment_rules::table)
            .values(LevelAdjustmentRuleRow {
                id: uuid_text(706),
                created_at: dt,
                updated_at: dt,
                deleted_at: None,
                account_id: account.into(),
                monthly_loss_downgrade_pct: "-5".into(),
                single_loss_downgrade_pct: "-2".into(),
                upgrade_profitable_trades: 10,
                upgrade_win_rate_pct: "60".into(),
                upgrade_consecutive_wins: 3,
                cooldown_profitable_trades: 4,
                cooldown_win_rate_pct: "50".into(),
                cooldown_consecutive_wins: 2,
                recovery_profitable_trades: 5,
                recovery_win_rate_pct: "55".into(),
                recovery_consecutive_wins: 3,
                min_trades_at_level_for_upgrade: 10,
                max_changes_in_30_days: 2,
            })
            .execute(conn)
            .unwrap();
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
        let trade_event_id = "00000000-0000-0000-0000-000000000015".to_string();
        let mistake_id = "00000000-0000-0000-0000-000000000016".to_string();
        let session_plan_id = "00000000-0000-0000-0000-000000000017".to_string();

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
                broker_kind: "alpaca".to_string(),
                broker_account_id: None,
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
                fixed_income_face_value: None,
                fixed_income_coupon_rate_pct: None,
                fixed_income_maturity_date: None,
                fixed_income_coupon_frequency_per_year: None,
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
            quantity: "1".to_string(),
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
                settlement_date: Some(t0),
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

        diesel::insert_into(schema::session_plans::table)
            .values(SessionPlanRow {
                id: session_plan_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: None,
                account_id: account_id.clone(),
                opened_at: dt0,
                closed_at: Some(dt0),
                regime: "normal".to_string(),
                permitted_setups: "opening range,pullback".to_string(),
                max_positions: 2,
                hypothesis: "follow only planned setups".to_string(),
                success_criteria: "take valid setups only".to_string(),
                failure_criteria: "force trades outside plan".to_string(),
                session_grade: Some("A".to_string()),
                adherence_notes: Some("followed plan".to_string()),
            })
            .execute(&mut conn1)
            .unwrap();

        diesel::insert_into(schema::mistakes::table)
            .values(MistakeRow {
                id: mistake_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: None,
                trade_id: trade_id.clone(),
                bias_tags: "5,14".to_string(),
                lollapalooza: true,
                error_type: "commission".to_string(),
                rule_violated: Some("risk_rule".to_string()),
                counterfactual_r: "1.25".to_string(),
                lesson: "Follow the rule.".to_string(),
            })
            .execute(&mut conn1)
            .unwrap();

        diesel::insert_into(schema::trade_events::table)
            .values(TradeEventRow {
                id: trade_event_id.clone(),
                created_at: dt0,
                updated_at: dt0,
                deleted_at: None,
                trade_id: trade_id.clone(),
                symbol: "AAPL".to_string(),
                event_type: "earnings".to_string(),
                event_date: t0,
                severity: "high".to_string(),
                notes: Some("watch earnings".to_string()),
                source: "manual".to_string(),
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

        insert_extended_backup_rows(&mut conn1, &account_id, &trade_id, &order_entry_id);

        let fixed_exported_at = DateTime::<Utc>::from_naive_utc_and_offset(dt0, Utc);
        let backup1 = read_backup_at(&mut conn1, fixed_exported_at).unwrap();

        let mut json = Vec::new();
        serde_json::to_writer_pretty(&mut json, &backup1).unwrap();

        let backup_from_json = read_backup_from_reader(&json[..]).unwrap();
        assert_eq!(backup_from_json.format, BACKUP_FORMAT);
        assert_eq!(backup_from_json.version, BACKUP_VERSION);

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
        assert_eq!(report.inserted_rows, 23);

        let backup2 = read_backup_at(&mut conn2, fixed_exported_at).unwrap();
        assert_eq!(backup1.schema, backup2.schema);
        assert_eq!(backup1.tables, backup2.tables);
    }

    #[test]
    fn test_validate_table_name_rejects_unknown() {
        assert!(validate_table_name("accounts").is_ok());
        assert!(validate_table_name("trades").is_ok());

        let err = validate_table_name("users; DROP TABLE accounts").unwrap_err();
        assert!(err.to_string().contains("unknown or disallowed table name"));

        assert!(validate_table_name("not_a_real_table").is_err());
    }

    #[test]
    fn test_read_table_count_rejects_malicious_name() {
        let mut conn = establish();
        let err = read_table_count(&mut conn, "accounts; DROP TABLE accounts").unwrap_err();
        assert!(err.to_string().contains("unknown or disallowed table name"));
    }

    // ---------------------------------------------------------------
    // Security tests
    // ---------------------------------------------------------------

    /// Oversized backup files should be rejected with a size-limit error,
    /// not read entirely into memory.
    #[test]
    fn backup_reader_should_enforce_size_limit() {
        let big_payload = format!(
            r#"{{"format":"trust-backup","version":1,"data":"{}"}}"#,
            "x".repeat(MAX_BACKUP_BYTES + 1)
        );
        let reader = std::io::Cursor::new(big_payload.as_bytes());

        let result = read_backup_from_reader(reader);
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("size") || err_msg.contains("limit") || err_msg.contains("too large"),
            "Backup reader should reject oversized files with a size-limit error, got: {err_msg}"
        );
    }

    /// Malformed UUIDs in backup JSON should be rejected during
    /// deserialization.
    #[test]
    fn backup_should_reject_malformed_uuids() {
        let malformed_backup = serde_json::json!({
            "format": "trust-backup",
            "version": 2,
            "exported_at": "2026-01-01T00:00:00Z",
            "schema": { "diesel_migrations": [] },
            "tables": {
                "fee_activities": [], "executions": [], "distribution_rules": [], "distribution_history": [], "level_adjustment_rules": [], "advisory_thresholds": [],
                "accounts": [{
                    "id": "NOT-A-VALID-UUID",
                    "created_at": "2026-01-01T00:00:00",
                    "updated_at": "2026-01-01T00:00:00",
                    "deleted_at": null,
                    "name": "evil",
                    "description": "test",
                    "environment": "paper",
                    "taxes_percentage": "0",
                    "earnings_percentage": "0",
                    "account_type": "primary",
                    "parent_account_id": null,
                    "broker_kind": "alpaca",
                    "broker_account_id": null
                }],
                "accounts_balances": [],
                "logs": [],
                "level_changes": [],
                "levels": [],
                "orders": [],
                "rules": [],
                "trade_grades": [],
                "trades": [],
                "trades_balances": [],
                "trading_vehicles": [],
                "transactions": [],
                "session_plans": [], "mistakes": [], "trade_events": []
            }
        });

        let json_bytes = serde_json::to_vec(&malformed_backup).unwrap();
        let reader = std::io::Cursor::new(json_bytes);
        let error = read_backup_from_reader(reader).expect_err("should reject malformed UUID");
        assert!(error.to_string().contains("accounts.id"));
    }

    #[test]
    fn restore_rejects_invalid_domains_and_financial_policy_values() {
        let mut conn = establish();
        let mut backup = read_backup_at(&mut conn, fixed_exported_at()).unwrap();
        backup
            .tables
            .accounts
            .push(account_row(uuid_text(901), "validation"));
        assert!(validate_backup_rows(&backup).is_ok());
        backup.tables.accounts.first_mut().unwrap().broker_kind = "unknown".into();
        assert!(validate_backup_rows(&backup)
            .unwrap_err()
            .to_string()
            .contains("broker_kind"));
        backup.tables.accounts.first_mut().unwrap().broker_kind = "alpaca".into();
        backup.tables.accounts.first_mut().unwrap().account_type = "unknown".into();
        assert!(validate_backup_rows(&backup)
            .unwrap_err()
            .to_string()
            .contains("account_type"));
        backup.tables.accounts.first_mut().unwrap().account_type = "primary".into();
        backup.tables.transactions.push(TransactionRow {
            id: uuid_text(902),
            created_at: fixed_dt(),
            updated_at: fixed_dt(),
            deleted_at: None,
            currency: "USD".into(),
            category: "unknown".into(),
            amount: "1".into(),
            account_id: uuid_text(901),
            trade_id: None,
        });
        assert!(validate_backup_rows(&backup)
            .unwrap_err()
            .to_string()
            .contains("category"));
        for (table, field, value) in [
            ("fee_activities", "amount", "-1"),
            ("fee_activities", "amount", "0"),
            ("distribution_rules", "earnings_percent", "1.1"),
            ("advisory_thresholds", "sector_limit_pct", "101"),
            ("level_adjustment_rules", "monthly_loss_downgrade_pct", "5"),
            ("level_adjustment_rules", "upgrade_win_rate_pct", "-1"),
        ] {
            assert!(
                validate_decimal_field(table, field, value).is_err(),
                "{table}.{field}"
            );
        }
        backup.tables.distribution_rules.push(DistributionRuleRow {
            id: uuid_text(903),
            created_at: fixed_dt(),
            updated_at: fixed_dt(),
            account_id: uuid_text(901),
            earnings_percent: "0.2".into(),
            tax_percent: "0.2".into(),
            reinvestment_percent: "0.2".into(),
            insurance_percent: "0.2".into(),
            minimum_threshold: "10".into(),
            configuration_password_hash: String::new(),
        });
        assert!(validate_policy_rows(&backup.tables)
            .unwrap_err()
            .to_string()
            .contains("sum to 1"));
    }

    #[test]
    fn version_two_requires_every_table_even_when_empty() {
        let mut conn = establish();
        let backup = read_backup_at(&mut conn, fixed_exported_at()).unwrap();
        let complete = serde_json::to_value(backup).unwrap();
        for table in ALLOWED_TABLES {
            let mut incomplete = complete.clone();
            incomplete
                .get_mut("tables")
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(*table);
            assert!(
                serde_json::from_value::<BackupEnvelopeV1>(incomplete).is_err(),
                "missing table {table} must not silently become empty"
            );
        }
    }

    #[test]
    fn incomplete_legacy_backup_is_rejected() {
        let account_id = uuid_text(101);
        let trading_vehicle_id = uuid_text(102);
        let legacy_backup = serde_json::json!({
            "format": "trust-backup",
            "version": 1,
            "exported_at": "2026-01-01T00:00:00Z",
            "schema": { "diesel_migrations": [] },
            "tables": {
                "accounts": [{
                    "id": account_id,
                    "created_at": "2026-01-01T00:00:00",
                    "updated_at": "2026-01-01T00:00:00",
                    "deleted_at": null,
                    "name": "legacy",
                    "description": "legacy account",
                    "environment": "paper",
                    "taxes_percentage": "0",
                    "earnings_percentage": "0"
                }],
                "accounts_balances": [],
                "logs": [],
                "level_changes": [],
                "levels": [],
                "orders": [],
                "rules": [],
                "trade_grades": [],
                "trades": [],
                "trades_balances": [],
                "trading_vehicles": [{
                    "id": trading_vehicle_id,
                    "created_at": "2026-01-01T00:00:00",
                    "updated_at": "2026-01-01T00:00:00",
                    "deleted_at": null,
                    "symbol": "AAPL",
                    "isin": null,
                    "category": "stock",
                    "broker": "alpaca",
                    "broker_asset_id": null,
                    "exchange": null,
                    "broker_asset_class": null,
                    "broker_asset_status": null,
                    "tradable": null,
                    "marginable": null,
                    "shortable": null,
                    "easy_to_borrow": null,
                    "fractionable": null
                }],
                "transactions": []
            }
        });

        let result = read_backup_from_reader(std::io::Cursor::new(
            serde_json::to_vec(&legacy_backup).unwrap(),
        ));
        assert!(
            result.is_err(),
            "Incomplete v1 exports must not replace a complete database"
        );
    }

    #[test]
    fn import_dry_run_validates_and_does_not_write() {
        let mut source = establish();
        insert_account(&mut source, uuid_text(201), "dry-run-source");
        let backup = read_backup_at(&mut source, fixed_exported_at()).unwrap();

        let mut target = establish();
        let report = import_backup(
            &mut target,
            &backup,
            ImportOptions {
                mode: ImportMode::Strict,
                dry_run: true,
            },
        )
        .expect("dry-run import should validate");

        assert_eq!(
            report,
            ImportReport {
                inserted_rows: 0,
                cleared_rows: 0
            }
        );
        assert_eq!(read_table_count(&mut target, "accounts").unwrap(), 0);
    }

    #[test]
    fn strict_import_rejects_non_empty_targets() {
        let mut source = establish();
        let backup = empty_backup(&mut source);

        let mut target = establish();
        insert_account(&mut target, uuid_text(301), "strict-existing");

        let error = import_backup(&mut target, &backup, ImportOptions::default())
            .expect_err("strict import should reject non-empty target");

        assert!(error
            .to_string()
            .contains("strict import requires empty DB"));
        assert!(error.to_string().contains("accounts"));
    }

    #[test]
    fn replace_import_clears_existing_rows() {
        let mut source = establish();
        let backup = empty_backup(&mut source);

        let mut target = establish();
        insert_account(&mut target, uuid_text(401), "replace-existing");

        let report = import_backup(
            &mut target,
            &backup,
            ImportOptions {
                mode: ImportMode::Replace,
                dry_run: false,
            },
        )
        .expect("replace import should clear existing rows");

        assert_eq!(report.cleared_rows, 1);
        assert_eq!(report.inserted_rows, 0);
        assert_eq!(read_table_count(&mut target, "accounts").unwrap(), 0);
    }

    #[test]
    fn import_rejects_format_version_and_migration_mismatch() {
        let mut source = establish();
        let backup = empty_backup(&mut source);
        let mut target = establish();

        let mut invalid = backup.clone();
        invalid.format = "other-format".to_string();
        let error = import_backup(&mut target, &invalid, ImportOptions::default())
            .expect_err("format mismatch should fail");
        assert!(error.to_string().contains("unsupported backup format"));

        let mut invalid = backup.clone();
        invalid.version = 999;
        let error = import_backup(&mut target, &invalid, ImportOptions::default())
            .expect_err("version mismatch should fail");
        assert!(error.to_string().contains("unsupported backup version"));

        let mut invalid = backup;
        invalid
            .schema
            .diesel_migrations
            .push("99999999999999_fake_migration".to_string());
        let error = import_backup(&mut target, &invalid, ImportOptions::default())
            .expect_err("migration mismatch should fail");
        assert!(error.to_string().contains("migration mismatch"));
    }

    #[test]
    fn import_rolls_back_when_foreign_key_check_fails() {
        let mut source = establish();
        let mut backup = empty_backup(&mut source);
        let dt = fixed_dt();
        backup.tables.transactions.push(TransactionRow {
            id: uuid_text(501),
            created_at: dt,
            updated_at: dt,
            deleted_at: None,
            currency: "USD".to_string(),
            category: "deposit".to_string(),
            amount: "100".to_string(),
            account_id: uuid_text(502),
            trade_id: None,
        });

        let mut target = establish();
        sql_query("PRAGMA foreign_keys=OFF;")
            .execute(&mut target)
            .unwrap();

        let error = import_backup(&mut target, &backup, ImportOptions::default())
            .expect_err("foreign key check should reject orphan rows");

        assert!(error
            .to_string()
            .contains("foreign key violations detected"));
        assert_eq!(read_table_count(&mut target, "transactions").unwrap(), 0);
    }

    #[test]
    fn failed_export_preserves_the_previous_backup() {
        let mut conn = establish();
        let path = temp_backup_path();
        std::fs::write(&path, b"previous-good-backup").unwrap();
        sql_query("DROP TABLE fee_activities")
            .execute(&mut conn)
            .unwrap();
        assert!(export_to_path(&mut conn, &path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous-good-backup");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn backup_path_roundtrip_uses_file_io() {
        let mut conn = establish();
        insert_account(&mut conn, uuid_text(601), "path-roundtrip");
        let path = temp_backup_path();

        export_to_path(&mut conn, &path).expect("backup export to path should succeed");
        let backup = read_backup_from_path(&path).expect("backup should read from path");

        assert_eq!(backup.tables.accounts.len(), 1);
        assert_eq!(
            backup.tables.accounts.first().expect("account row").name,
            "path-roundtrip"
        );

        std::fs::remove_file(path).expect("temporary backup file should be removed");
    }
}
