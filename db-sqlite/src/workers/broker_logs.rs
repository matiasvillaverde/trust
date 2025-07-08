use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::logs;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{BrokerLog, ReadBrokerLogsDB, Trade, WriteBrokerLogsDB};
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
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

impl WriteBrokerLogsDB for BrokerLogDB {
    fn create_log(&mut self, log: &str, trade: &Trade) -> Result<BrokerLog, Box<dyn Error>> {
        let uuid = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let new_account = NewBrokerLogs {
            id: uuid,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            log: log.to_lowercase(),
            trade_id: trade.id.to_string(),
        };

        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

        diesel::insert_into(logs::table)
            .values(&new_account)
            .get_result::<BrokerLogSQLite>(connection)
            .map_err(|error| {
                error!("Error creating broker log: {:?}", error);
                error
            })?
            .into_domain_model()
    }
}

impl ReadBrokerLogsDB for BrokerLogDB {
    fn read_all_logs_for_trade(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<BrokerLog>, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

        logs::table
            .filter(logs::trade_id.eq(trade_id.to_string()))
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
}
