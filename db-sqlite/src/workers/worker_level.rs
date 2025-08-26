use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::{level_changes, levels};
use chrono::{NaiveDate, NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{
    Account, Level, LevelChange, LevelStatus, ReadLevelDB, WriteLevelDB, LEVEL_MULTIPLIERS,
};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::error;
use uuid::Uuid;

/// Database worker for level operations
pub struct LevelDB {
    pub connection: Arc<Mutex<SqliteConnection>>,
}

impl std::fmt::Debug for LevelDB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LevelDB")
            .field("connection", &"Arc<Mutex<SqliteConnection>>")
            .finish()
    }
}

impl ReadLevelDB for LevelDB {
    fn level_for_account(&mut self, account_id: Uuid) -> Result<Level, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

        levels::table
            .filter(levels::account_id.eq(account_id.to_string()))
            .filter(levels::deleted_at.is_null())
            .first::<LevelSQLite>(connection)
            .map_err(|error| {
                error!(
                    "Error reading level for account {}: {:?}",
                    account_id, error
                );
                error
            })?
            .into_domain_model()
    }

    fn level_changes_for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<LevelChange>, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

        level_changes::table
            .filter(level_changes::account_id.eq(account_id.to_string()))
            .filter(level_changes::deleted_at.is_null())
            .order(level_changes::changed_at.desc())
            .load::<LevelChangeSQLite>(connection)
            .map_err(|error| {
                error!(
                    "Error reading level changes for account {}: {:?}",
                    account_id, error
                );
                error
            })?
            .into_domain_models()
    }

    fn recent_level_changes(
        &mut self,
        account_id: Uuid,
        days: u32,
    ) -> Result<Vec<LevelChange>, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

        let cutoff_date = Utc::now().naive_utc() - chrono::Duration::days(days as i64);

        level_changes::table
            .filter(level_changes::account_id.eq(account_id.to_string()))
            .filter(level_changes::deleted_at.is_null())
            .filter(level_changes::changed_at.ge(cutoff_date))
            .order(level_changes::changed_at.desc())
            .load::<LevelChangeSQLite>(connection)
            .map_err(|error| {
                error!(
                    "Error reading recent level changes for account {}: {:?}",
                    account_id, error
                );
                error
            })?
            .into_domain_models()
    }
}

impl WriteLevelDB for LevelDB {
    fn create_default_level(&mut self, account: &Account) -> Result<Level, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

        let now = Utc::now().naive_utc();
        let today = now.date();

        let new_level = NewLevel {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            account_id: account.id.to_string(),
            current_level: 3, // Default to Level 3 (Full Size Trading)
            risk_multiplier: LEVEL_MULTIPLIERS[3].to_string(), // 1.0x
            status: LevelStatus::Normal.to_string(),
            trades_at_level: 0,
            level_start_date: today,
        };

        diesel::insert_into(levels::table)
            .values(&new_level)
            .get_result::<LevelSQLite>(connection)
            .map_err(|error| {
                error!(
                    "Error creating default level for account {}: {:?}",
                    account.id, error
                );
                error
            })?
            .into_domain_model()
    }

    fn update_level(&mut self, level: &Level) -> Result<Level, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

        let now = Utc::now().naive_utc();

        diesel::update(levels::table.filter(levels::id.eq(level.id.to_string())))
            .set((
                levels::updated_at.eq(now),
                levels::current_level.eq(level.current_level as i32),
                levels::risk_multiplier.eq(level.risk_multiplier.to_string()),
                levels::status.eq(level.status.to_string()),
                levels::trades_at_level.eq(level.trades_at_level as i32),
                levels::level_start_date.eq(level.level_start_date),
            ))
            .get_result::<LevelSQLite>(connection)
            .map_err(|error| {
                error!("Error updating level {}: {:?}", level.id, error);
                error
            })?
            .into_domain_model()
    }

    fn create_level_change(
        &mut self,
        level_change: &LevelChange,
    ) -> Result<LevelChange, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

        let new_level_change = NewLevelChange {
            id: level_change.id.to_string(),
            created_at: level_change.created_at,
            updated_at: level_change.updated_at,
            account_id: level_change.account_id.to_string(),
            old_level: level_change.old_level as i32,
            new_level: level_change.new_level as i32,
            change_reason: level_change.change_reason.clone(),
            trigger_type: level_change.trigger_type.clone(),
            changed_at: level_change.changed_at,
        };

        diesel::insert_into(level_changes::table)
            .values(&new_level_change)
            .get_result::<LevelChangeSQLite>(connection)
            .map_err(|error| {
                error!("Error creating level change: {:?}", error);
                error
            })?
            .into_domain_model()
    }
}

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = levels)]
#[diesel(primary_key(id))]
#[diesel(treat_none_as_null = true)]
pub struct LevelSQLite {
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

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = level_changes)]
#[diesel(primary_key(id))]
#[diesel(treat_none_as_null = true)]
pub struct LevelChangeSQLite {
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

impl TryFrom<LevelSQLite> for Level {
    type Error = ConversionError;

    fn try_from(value: LevelSQLite) -> Result<Self, Self::Error> {
        Ok(Level {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse level ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            account_id: Uuid::parse_str(&value.account_id)
                .map_err(|_| ConversionError::new("account_id", "Failed to parse account ID"))?,
            current_level: value.current_level as u8,
            risk_multiplier: Decimal::from_str(&value.risk_multiplier).map_err(|_| {
                ConversionError::new("risk_multiplier", "Failed to parse risk multiplier")
            })?,
            status: LevelStatus::from_str(&value.status)
                .map_err(|_| ConversionError::new("status", "Failed to parse level status"))?,
            trades_at_level: value.trades_at_level as u32,
            level_start_date: value.level_start_date,
        })
    }
}

impl TryFrom<LevelChangeSQLite> for LevelChange {
    type Error = ConversionError;

    fn try_from(value: LevelChangeSQLite) -> Result<Self, Self::Error> {
        Ok(LevelChange {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse level change ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            account_id: Uuid::parse_str(&value.account_id)
                .map_err(|_| ConversionError::new("account_id", "Failed to parse account ID"))?,
            old_level: value.old_level as u8,
            new_level: value.new_level as u8,
            change_reason: value.change_reason,
            trigger_type: value.trigger_type,
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
    account_id: String,
    old_level: i32,
    new_level: i32,
    change_reason: String,
    trigger_type: String,
    changed_at: NaiveDateTime,
}
