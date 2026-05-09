use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::logs;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{BrokerLog, ReadBrokerLogsDB, Trade, WriteBrokerLogsDB};
use std::error::Error;
use std::sync::{Arc, Mutex, MutexGuard};
use tracing::error;
use uuid::Uuid;

/// Database worker for broker log operations
pub struct BrokerLogDB {
    pub connection: Arc<Mutex<SqliteConnection>>,
}

impl std::fmt::Debug for BrokerLogDB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerLogDB")
            .field("connection", &"Arc<Mutex<SqliteConnection>>")
            .finish()
    }
}

impl BrokerLogDB {
    fn connection_guard(&self) -> Result<MutexGuard<'_, SqliteConnection>, Box<dyn Error>> {
        self.connection.lock().map_err(|error| {
            format!("failed to acquire broker log database connection lock: {error}").into()
        })
    }
}

impl WriteBrokerLogsDB for BrokerLogDB {
    fn create_log(&mut self, log: &str, trade: &Trade) -> Result<BrokerLog, Box<dyn Error>> {
        let uuid = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();
        let normalized_log = log.to_lowercase();

        let new_account = NewBrokerLogs {
            id: uuid.clone(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            log: normalized_log.clone(),
            trade_id: trade.id.to_string(),
        };

        let mut guard = self.connection_guard()?;
        let connection: &mut SqliteConnection = &mut guard;

        diesel::insert_into(logs::table)
            .values(&new_account)
            .execute(connection)
            .map_err(|error| {
                error!("Error creating broker log: {:?}", error);
                error
            })?;

        Ok(BrokerLog {
            id: Uuid::parse_str(&uuid)?,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            log: normalized_log,
            trade_id: trade.id,
        })
    }
}

impl ReadBrokerLogsDB for BrokerLogDB {
    fn read_all_logs_for_trade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<BrokerLog>, Box<dyn Error>> {
        let mut guard = self.connection_guard()?;
        let connection: &mut SqliteConnection = &mut guard;

        logs::table
            .filter(logs::trade_id.eq(trade_id.to_string()))
            .filter(logs::deleted_at.is_null())
            .load::<BrokerLogSQLite>(connection)
            .map_err(|error| {
                error!("Error reading broker logs for trade: {:?}", error);
                error
            })?
            .into_domain_models()
    }
}

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = logs)]
pub struct BrokerLogSQLite {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub log: String,
    pub trade_id: String,
}

impl TryFrom<BrokerLogSQLite> for BrokerLog {
    type Error = ConversionError;

    fn try_from(value: BrokerLogSQLite) -> Result<Self, Self::Error> {
        Ok(BrokerLog {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse log ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            log: value.log,
            trade_id: Uuid::parse_str(&value.trade_id)
                .map_err(|_| ConversionError::new("trade_id", "Failed to parse trade ID"))?,
        })
    }
}

impl IntoDomainModel<BrokerLog> for BrokerLogSQLite {
    fn into_domain_model(self) -> Result<BrokerLog, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Insertable)]
#[diesel(table_name = logs)]
#[diesel(treat_none_as_null = true)]
struct NewBrokerLogs {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    log: String,
    trade_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel_migrations::*;
    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    // Declare a test database connection
    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        // This will run the necessary migrations.
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    fn base_sqlite_log() -> BrokerLogSQLite {
        let now = Utc::now().naive_utc();
        BrokerLogSQLite {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            log: "submitted".to_string(),
            trade_id: Uuid::new_v4().to_string(),
        }
    }

    fn assert_conversion_error(row: BrokerLogSQLite, field: &str) {
        let error = BrokerLog::try_from(row).expect_err("corrupt broker log must fail conversion");
        assert!(error.to_string().contains(field));
    }

    fn assert_error_mentions(error: Box<dyn Error>, expected: &str) {
        let message = error.to_string();
        assert!(
            message.contains(expected),
            "expected error to mention {expected:?}, got {message:?}"
        );
    }

    fn broker_log_db_with_missing_logs_table() -> BrokerLogDB {
        let mut conn = establish_connection();
        diesel::sql_query("DROP TABLE logs")
            .execute(&mut conn)
            .expect("logs table should be dropped");

        BrokerLogDB {
            connection: Arc::new(Mutex::new(conn)),
        }
    }

    fn poisoned_broker_log_db() -> BrokerLogDB {
        let connection = Arc::new(Mutex::new(establish_connection()));
        let poisoned_connection = Arc::clone(&connection);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = poisoned_connection
                .lock()
                .expect("connection lock should be acquired before poisoning");
            std::panic::resume_unwind(Box::new("poison broker log connection lock"));
        }));
        BrokerLogDB { connection }
    }

    fn assert_connection_lock_error<T>(result: Result<T, Box<dyn Error>>) {
        assert!(result.is_err());
        let error = result.err().expect("operation should fail");
        assert!(error
            .to_string()
            .contains("failed to acquire broker log database connection lock"));
    }

    #[test]
    fn test_create_log() {
        let conn: SqliteConnection = establish_connection();
        let mut db = BrokerLogDB {
            connection: Arc::new(Mutex::new(conn)),
        };

        let trade = Trade::default();

        let log = db
            .create_log("Test Account", &trade)
            .expect("Error creating log");

        assert_eq!(log.log, "test account");
        assert_eq!(log.trade_id, trade.id);
        assert_eq!(log.deleted_at, None);
    }

    #[test]
    fn test_read_log() {
        let conn: SqliteConnection = establish_connection();
        let mut db = BrokerLogDB {
            connection: Arc::new(Mutex::new(conn)),
        };

        let trade = Trade::default();

        let log = db
            .create_log("Test Account", &trade)
            .expect("Error creating log");

        let read_log = db
            .read_all_logs_for_trade(trade.id)
            .expect("Error reading log");

        assert_eq!(read_log.len(), 1);
        assert_eq!(
            log.log,
            read_log.first().expect("Expected at least one log").log
        );
        assert_eq!(
            read_log
                .first()
                .expect("Expected at least one log")
                .trade_id,
            trade.id
        );
        assert_eq!(log.deleted_at, None);
    }

    #[test]
    fn debug_representation_hides_connection_internals() {
        let db = BrokerLogDB {
            connection: Arc::new(Mutex::new(establish_connection())),
        };

        assert_eq!(
            format!("{db:?}"),
            "BrokerLogDB { connection: \"Arc<Mutex<SqliteConnection>>\" }"
        );
    }

    #[test]
    fn broker_log_methods_return_errors_when_connection_lock_is_poisoned() {
        let mut db = poisoned_broker_log_db();
        let trade = Trade::default();

        assert_connection_lock_error(db.create_log("submitted", &trade));
        assert_connection_lock_error(db.read_all_logs_for_trade(trade.id));
    }

    #[test]
    fn read_logs_filters_trade_id_and_soft_deleted_rows() {
        let conn: SqliteConnection = establish_connection();
        let mut db = BrokerLogDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        let trade = Trade::default();
        let other_trade = Trade::default();

        let active = db
            .create_log("Active", &trade)
            .expect("active log should write");
        let deleted = db
            .create_log("Deleted", &trade)
            .expect("deleted log should write");
        db.create_log("Other Trade", &other_trade)
            .expect("other trade log should write");

        {
            let mut conn = db
                .connection
                .lock()
                .expect("connection lock should be available");
            diesel::update(logs::table.filter(logs::id.eq(deleted.id.to_string())))
                .set(logs::deleted_at.eq(Some(Utc::now().naive_utc())))
                .execute(&mut *conn)
                .expect("log should be soft deleted");
        }

        let logs = db
            .read_all_logs_for_trade(trade.id)
            .expect("logs should read");
        let log = logs.first().expect("one active log should remain");

        assert_eq!(logs.len(), 1);
        assert_eq!(log.id, active.id);
        assert_eq!(log.log, "active");
    }

    #[test]
    fn broker_log_sqlite_conversion_reports_corrupt_fields() {
        assert_conversion_error(
            BrokerLogSQLite {
                id: "not-a-uuid".to_string(),
                ..base_sqlite_log()
            },
            "id",
        );
        assert_conversion_error(
            BrokerLogSQLite {
                trade_id: "not-a-uuid".to_string(),
                ..base_sqlite_log()
            },
            "trade_id",
        );
    }

    #[test]
    fn broker_log_sqlite_into_domain_model_reports_corrupt_id() {
        let error = BrokerLogSQLite {
            id: "not-a-uuid".to_string(),
            ..base_sqlite_log()
        }
        .into_domain_model()
        .expect_err("trait conversion must surface corrupt IDs");

        assert_error_mentions(error, "id");
    }

    #[test]
    fn create_log_reports_missing_logs_table_error() {
        let mut db = broker_log_db_with_missing_logs_table();

        let error = db
            .create_log("submitted", &Trade::default())
            .expect_err("missing logs table should fail create");

        assert_error_mentions(error, "logs");
    }

    #[test]
    fn read_logs_reports_missing_logs_table_error() {
        let mut db = broker_log_db_with_missing_logs_table();

        let error = db
            .read_all_logs_for_trade(Uuid::new_v4())
            .expect_err("missing logs table should fail read");

        assert_error_mentions(error, "logs");
    }

    #[test]
    fn read_logs_surfaces_corrupt_row_id() {
        let conn: SqliteConnection = establish_connection();
        let mut db = BrokerLogDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        let trade = Trade::default();

        {
            let mut conn = db
                .connection
                .lock()
                .expect("connection lock should be available");
            diesel::insert_into(logs::table)
                .values(BrokerLogSQLite {
                    id: "not-a-uuid".to_string(),
                    trade_id: trade.id.to_string(),
                    ..base_sqlite_log()
                })
                .execute(&mut *conn)
                .expect("corrupt broker log row should insert for conversion test");
        }

        let error = db
            .read_all_logs_for_trade(trade.id)
            .expect_err("corrupt broker log row should fail conversion");

        assert_error_mentions(error, "id");
    }
}
