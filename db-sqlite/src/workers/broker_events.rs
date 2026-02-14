use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::broker_events;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{BrokerEvent, ReadBrokerEventsDB, WriteBrokerEventsDB};
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::error;
use uuid::Uuid;

/// Database worker for broker event operations.
///
/// These events are stored verbatim for audit/replay (do not normalize payload).
pub struct BrokerEventDB {
    pub connection: Arc<Mutex<SqliteConnection>>,
}

impl std::fmt::Debug for BrokerEventDB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrokerEventDB")
            .field("connection", &"Arc<Mutex<SqliteConnection>>")
            .finish()
    }
}

impl WriteBrokerEventsDB for BrokerEventDB {
    fn create_event(&mut self, event: &BrokerEvent) -> Result<BrokerEvent, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

        let uuid = event.id.to_string();
        let now = Utc::now().naive_utc();

        let record = BrokerEventSQLite {
            id: uuid.clone(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: event.account_id.to_string(),
            trade_id: event.trade_id.to_string(),
            source: event.source.clone(),
            stream: event.stream.clone(),
            event_type: event.event_type.clone(),
            broker_order_id: event.broker_order_id.map(|x| x.to_string()),
            payload_json: event.payload_json.clone(),
        };

        diesel::insert_into(broker_events::table)
            .values(&record)
            .execute(connection)
            .map_err(|e| {
                error!("Error creating broker event: {:?}", e);
                e
            })?;

        Ok(BrokerEvent {
            created_at: now,
            updated_at: now,
            deleted_at: None,
            ..event.clone()
        })
    }
}

impl ReadBrokerEventsDB for BrokerEventDB {
    fn read_all_for_trade(&mut self, trade_id: Uuid) -> Result<Vec<BrokerEvent>, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

        broker_events::table
            .filter(broker_events::trade_id.eq(trade_id.to_string()))
            .order_by(broker_events::created_at.asc())
            .load::<BrokerEventSQLite>(connection)
            .map_err(|e| {
                error!("Error reading broker events for trade: {:?}", e);
                e
            })?
            .into_domain_models()
    }
}

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = broker_events)]
pub struct BrokerEventSQLite {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,

    pub account_id: String,
    pub trade_id: String,

    pub source: String,
    pub stream: String,
    pub event_type: String,

    pub broker_order_id: Option<String>,

    pub payload_json: String,
}

impl TryFrom<BrokerEventSQLite> for BrokerEvent {
    type Error = ConversionError;

    fn try_from(value: BrokerEventSQLite) -> Result<Self, Self::Error> {
        Ok(BrokerEvent {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse event ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,

            account_id: Uuid::parse_str(&value.account_id)
                .map_err(|_| ConversionError::new("account_id", "Failed to parse account ID"))?,
            trade_id: Uuid::parse_str(&value.trade_id)
                .map_err(|_| ConversionError::new("trade_id", "Failed to parse trade ID"))?,

            source: value.source,
            stream: value.stream,
            event_type: value.event_type,

            broker_order_id: value
                .broker_order_id
                .map(|s| Uuid::parse_str(&s))
                .transpose()
                .map_err(|_| ConversionError::new("broker_order_id", "Failed to parse broker order ID"))?,

            payload_json: value.payload_json,
        })
    }
}

impl IntoDomainModel<BrokerEvent> for BrokerEventSQLite {
    fn into_domain_model(self) -> Result<BrokerEvent, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel_migrations::*;
    use std::sync::Arc;
    use std::sync::Mutex;

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    #[test]
    fn create_and_read_event_roundtrip() {
        let conn: SqliteConnection = establish_connection();
        let mut db = BrokerEventDB {
            connection: Arc::new(Mutex::new(conn)),
        };

        let event = BrokerEvent {
            id: Uuid::new_v4(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            deleted_at: None,
            account_id: Uuid::new_v4(),
            trade_id: Uuid::new_v4(),
            source: "alpaca".to_string(),
            stream: "trade_updates".to_string(),
            event_type: "fill".to_string(),
            broker_order_id: Some(Uuid::new_v4()),
            payload_json: r#"{"stream":"trade_updates","data":{"event":"fill"}}"#.to_string(),
        };

        let stored = db.create_event(&event).unwrap();
        assert_eq!(stored.id, event.id);
        assert_eq!(stored.payload_json, event.payload_json);
        assert_eq!(stored.source, "alpaca");

        let events = db.read_all_for_trade(event.trade_id).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, event.id);
        assert_eq!(events[0].payload_json, event.payload_json);
    }
}

