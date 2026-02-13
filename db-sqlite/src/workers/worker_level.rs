use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::{level_changes, levels};
use chrono::{Duration, NaiveDate, NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{Account, Level, LevelChange, LevelStatus, LevelTrigger};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use uuid::Uuid;

/// Worker for handling level database operations.
#[derive(Debug)]
pub struct WorkerLevel;

impl WorkerLevel {
    pub fn create_default(
        connection: &mut SqliteConnection,
        account: &Account,
    ) -> Result<Level, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        let record = NewLevel {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: account.id.to_string(),
            current_level: i32::from(3_u8),
            risk_multiplier: Level::multiplier_for_level(3)?.to_string(),
            status: LevelStatus::Normal.to_string(),
            trades_at_level: i32::from(0_u8),
            level_start_date: now.date(),
        };

        diesel::insert_into(levels::table)
            .values(&record)
            .get_result::<LevelSQLite>(connection)
            .map_err(|db_error| {
                error!(
                    "Error creating default level for account {}: {db_error:?}",
                    account.id
                );
                db_error
            })?
            .into_domain_model()
    }

    pub fn read_for_account(
        connection: &mut SqliteConnection,
        account_id: Uuid,
    ) -> Result<Level, Box<dyn Error>> {
        levels::table
            .filter(levels::account_id.eq(account_id.to_string()))
            .filter(levels::deleted_at.is_null())
            .first::<LevelSQLite>(connection)
            .map_err(|db_error| {
                error!(
                    "Error reading level for account {}: {db_error:?}",
                    account_id
                );
                db_error
            })?
            .into_domain_model()
    }

    pub fn read_changes_for_account(
        connection: &mut SqliteConnection,
        account_id: Uuid,
    ) -> Result<Vec<LevelChange>, Box<dyn Error>> {
        level_changes::table
            .filter(level_changes::account_id.eq(account_id.to_string()))
            .filter(level_changes::deleted_at.is_null())
            .order(level_changes::changed_at.desc())
            .load::<LevelChangeSQLite>(connection)
            .map_err(|db_error| {
                error!(
                    "Error reading level changes for account {}: {db_error:?}",
                    account_id
                );
                db_error
            })?
            .into_domain_models()
    }

    pub fn read_recent_changes_for_account(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        days: u32,
    ) -> Result<Vec<LevelChange>, Box<dyn Error>> {
        let cutoff = Utc::now()
            .naive_utc()
            .checked_sub_signed(Duration::days(i64::from(days)))
            .ok_or_else(|| ConversionError::new("days", "Invalid days window"))?;

        level_changes::table
            .filter(level_changes::account_id.eq(account_id.to_string()))
            .filter(level_changes::deleted_at.is_null())
            .filter(level_changes::changed_at.ge(cutoff))
            .order(level_changes::changed_at.desc())
            .load::<LevelChangeSQLite>(connection)
            .map_err(|db_error| {
                error!(
                    "Error reading recent level changes for account {}: {db_error:?}",
                    account_id
                );
                db_error
            })?
            .into_domain_models()
    }

    pub fn update(
        connection: &mut SqliteConnection,
        level: &Level,
    ) -> Result<Level, Box<dyn Error>> {
        let now = Utc::now().naive_utc();

        diesel::update(levels::table.filter(levels::id.eq(level.id.to_string())))
            .set((
                levels::updated_at.eq(now),
                levels::current_level.eq(i32::from(level.current_level)),
                levels::risk_multiplier.eq(level.risk_multiplier.to_string()),
                levels::status.eq(level.status.to_string()),
                levels::trades_at_level.eq(i32::try_from(level.trades_at_level).map_err(|_| {
                    ConversionError::new("trades_at_level", "trades_at_level overflows i32")
                })?),
                levels::level_start_date.eq(level.level_start_date),
            ))
            .get_result::<LevelSQLite>(connection)
            .map_err(|db_error| {
                error!("Error updating level {}: {db_error:?}", level.id);
                db_error
            })?
            .into_domain_model()
    }

    pub fn create_change(
        connection: &mut SqliteConnection,
        level_change: &LevelChange,
    ) -> Result<LevelChange, Box<dyn Error>> {
        let record = NewLevelChange {
            id: level_change.id.to_string(),
            created_at: level_change.created_at,
            updated_at: level_change.updated_at,
            deleted_at: level_change.deleted_at,
            account_id: level_change.account_id.to_string(),
            old_level: i32::from(level_change.old_level),
            new_level: i32::from(level_change.new_level),
            change_reason: level_change.change_reason.clone(),
            trigger_type: level_change.trigger_type.to_string(),
            changed_at: level_change.changed_at,
        };

        diesel::insert_into(level_changes::table)
            .values(&record)
            .get_result::<LevelChangeSQLite>(connection)
            .map_err(|db_error| {
                error!(
                    "Error creating level change event {}: {db_error:?}",
                    level_change.id
                );
                db_error
            })?
            .into_domain_model()
    }
}

#[derive(Debug, Queryable, Identifiable, Selectable)]
#[diesel(table_name = levels)]
struct LevelSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    account_id: String,
    current_level: i32,
    risk_multiplier: String,
    status: String,
    trades_at_level: i32,
    level_start_date: NaiveDate,
}

#[derive(Debug, Queryable, Identifiable, Selectable)]
#[diesel(table_name = level_changes)]
struct LevelChangeSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    account_id: String,
    old_level: i32,
    new_level: i32,
    change_reason: String,
    trigger_type: String,
    changed_at: NaiveDateTime,
}

impl TryFrom<LevelSQLite> for Level {
    type Error = ConversionError;

    fn try_from(value: LevelSQLite) -> Result<Self, Self::Error> {
        Ok(Level {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse level id"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            account_id: Uuid::parse_str(&value.account_id)
                .map_err(|_| ConversionError::new("account_id", "Failed to parse account id"))?,
            current_level: u8::try_from(value.current_level)
                .map_err(|_| ConversionError::new("current_level", "Invalid level value"))?,
            risk_multiplier: Decimal::from_str(&value.risk_multiplier).map_err(|_| {
                ConversionError::new("risk_multiplier", "Failed to parse risk multiplier")
            })?,
            status: LevelStatus::from_str(&value.status)
                .map_err(|_| ConversionError::new("status", "Failed to parse level status"))?,
            trades_at_level: u32::try_from(value.trades_at_level)
                .map_err(|_| ConversionError::new("trades_at_level", "Invalid trades count"))?,
            level_start_date: value.level_start_date,
        })
    }
}

impl TryFrom<LevelChangeSQLite> for LevelChange {
    type Error = ConversionError;

    fn try_from(value: LevelChangeSQLite) -> Result<Self, Self::Error> {
        Ok(LevelChange {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse level change id"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            account_id: Uuid::parse_str(&value.account_id)
                .map_err(|_| ConversionError::new("account_id", "Failed to parse account id"))?,
            old_level: u8::try_from(value.old_level)
                .map_err(|_| ConversionError::new("old_level", "Invalid old_level value"))?,
            new_level: u8::try_from(value.new_level)
                .map_err(|_| ConversionError::new("new_level", "Invalid new_level value"))?,
            change_reason: value.change_reason,
            trigger_type: LevelTrigger::from_str(&value.trigger_type).map_err(|_| {
                ConversionError::new("trigger_type", "Failed to parse level trigger")
            })?,
            changed_at: value.changed_at,
        })
    }
}

impl IntoDomainModel<Level> for LevelSQLite {
    fn into_domain_model(self) -> Result<Level, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

impl IntoDomainModel<LevelChange> for LevelChangeSQLite {
    fn into_domain_model(self) -> Result<LevelChange, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Insertable)]
#[diesel(table_name = levels)]
#[diesel(treat_none_as_null = true)]
struct NewLevel {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    account_id: String,
    current_level: i32,
    risk_multiplier: String,
    status: String,
    trades_at_level: i32,
    level_start_date: NaiveDate,
}

#[derive(Insertable)]
#[diesel(table_name = level_changes)]
#[diesel(treat_none_as_null = true)]
struct NewLevelChange {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    account_id: String,
    old_level: i32,
    new_level: i32,
    change_reason: String,
    trigger_type: String,
    changed_at: NaiveDateTime,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]

    use super::*;
    use diesel::sql_query;
    use diesel::Connection;
    use diesel_migrations::*;
    use model::Environment;
    use rust_decimal_macros::dec;

    fn setup_account(conn: &mut SqliteConnection, id: Uuid) {
        let sql = format!(
            "INSERT INTO accounts (id, created_at, updated_at, deleted_at, name, description, environment, taxes_percentage, earnings_percentage) VALUES ('{}', '2020-01-01 00:00:00', '2020-01-01 00:00:00', NULL, 'acct', 'acct', 'paper', '0', '0')",
            id
        );
        sql_query(sql).execute(conn).expect("insert account");
    }

    #[test]
    fn test_create_default_and_manual_transition_roundtrip() {
        pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
        let mut conn = SqliteConnection::establish(":memory:").expect("sqlite in-memory");
        conn.run_pending_migrations(MIGRATIONS)
            .expect("run migrations");
        conn.begin_test_transaction().expect("begin transaction");

        let account_id = Uuid::new_v4();
        setup_account(&mut conn, account_id);

        let account = model::Account {
            id: account_id,
            environment: Environment::Paper,
            ..Default::default()
        };

        let level = WorkerLevel::create_default(&mut conn, &account).expect("create level");
        assert_eq!(level.current_level, 3);
        assert_eq!(level.risk_multiplier, dec!(1.00));

        let now = Utc::now().naive_utc();
        let (updated, event) = level
            .transition_to(
                2,
                "Exceeded monthly risk budget",
                LevelTrigger::Custom("manual_adjustment".to_string()),
                now,
            )
            .expect("transition");

        let stored = WorkerLevel::update(&mut conn, &updated).expect("update level");
        let change = WorkerLevel::create_change(&mut conn, &event).expect("create change");

        assert_eq!(stored.current_level, 2);
        assert_eq!(stored.status, LevelStatus::Probation);
        assert_eq!(change.old_level, 3);
        assert_eq!(change.new_level, 2);

        let latest = WorkerLevel::read_for_account(&mut conn, account_id).expect("read level");
        assert_eq!(latest.current_level, 2);

        let history =
            WorkerLevel::read_changes_for_account(&mut conn, account_id).expect("history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, change.id);

        let recent = WorkerLevel::read_recent_changes_for_account(&mut conn, account_id, 30)
            .expect("recent");
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].new_level, 2);
    }

    #[test]
    fn test_create_default_twice_for_same_account_fails_unique_constraint() {
        pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
        let mut conn = SqliteConnection::establish(":memory:").expect("sqlite in-memory");
        conn.run_pending_migrations(MIGRATIONS)
            .expect("run migrations");
        conn.begin_test_transaction().expect("begin transaction");

        let account_id = Uuid::new_v4();
        setup_account(&mut conn, account_id);
        let account = model::Account {
            id: account_id,
            environment: Environment::Paper,
            ..Default::default()
        };

        let first = WorkerLevel::create_default(&mut conn, &account);
        assert!(first.is_ok());

        let second = WorkerLevel::create_default(&mut conn, &account);
        assert!(second.is_err());
        let message = second.expect_err("error").to_string().to_lowercase();
        assert!(message.contains("unique"));
    }
}
