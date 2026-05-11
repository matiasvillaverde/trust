use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::trade_events;
use chrono::{NaiveDate, NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{TradeEvent, TradeEventSeverity, TradeEventSource, TradeEventType};
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use uuid::Uuid;

/// Worker for handling trade event database operations.
#[derive(Debug)]
pub struct WorkerTradeEvent;

impl WorkerTradeEvent {
    /// Create a new trade event.
    pub fn create(
        connection: &mut SqliteConnection,
        event: &TradeEvent,
    ) -> Result<TradeEvent, Box<dyn Error>> {
        let record = NewTradeEvent {
            id: event.id.to_string(),
            created_at: event.created_at,
            updated_at: event.updated_at,
            deleted_at: event.deleted_at,
            trade_id: event.trade_id.to_string(),
            symbol: event.symbol.clone(),
            event_type: event.event_type.to_string(),
            event_date: event.event_date,
            severity: event.severity.to_string(),
            notes: event.notes.clone(),
            source: event.source.to_string(),
        };

        diesel::insert_into(trade_events::table)
            .values(&record)
            .get_result::<TradeEventSQLite>(connection)
            .map_err(|error| {
                error!("Error creating trade event: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    /// Read active trade events for a trade.
    pub fn read_for_trade(
        connection: &mut SqliteConnection,
        trade_id: Uuid,
    ) -> Result<Vec<TradeEvent>, Box<dyn Error>> {
        trade_events::table
            .filter(trade_events::deleted_at.is_null())
            .filter(trade_events::trade_id.eq(trade_id.to_string()))
            .order_by((
                trade_events::event_date.asc(),
                trade_events::created_at.asc(),
                trade_events::id.asc(),
            ))
            .load::<TradeEventSQLite>(connection)
            .map_err(|error| {
                error!("Error reading trade events for trade: {:?}", error);
                error
            })?
            .into_domain_models()
    }

    /// Soft-delete a trade event by identifier.
    pub fn delete(connection: &mut SqliteConnection, event_id: Uuid) -> Result<(), Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        let affected = diesel::update(
            trade_events::table
                .filter(trade_events::id.eq(event_id.to_string()))
                .filter(trade_events::deleted_at.is_null()),
        )
        .set((
            trade_events::deleted_at.eq(Some(now)),
            trade_events::updated_at.eq(now),
        ))
        .execute(connection)
        .map_err(|error| {
            error!("Error deleting trade event: {:?}", error);
            error
        })?;

        if affected == 0 {
            return Err(format!("trade event not found: {event_id}").into());
        }

        Ok(())
    }
}

#[derive(Debug, Queryable, Identifiable)]
#[diesel(table_name = trade_events)]
struct TradeEventSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    trade_id: String,
    symbol: String,
    event_type: String,
    event_date: NaiveDate,
    severity: String,
    notes: Option<String>,
    source: String,
}

impl TryFrom<TradeEventSQLite> for TradeEvent {
    type Error = ConversionError;

    fn try_from(value: TradeEventSQLite) -> Result<Self, Self::Error> {
        let event_type = TradeEventType::from_str(&value.event_type)
            .map_err(|_| ConversionError::new("event_type", "Failed to parse event type"))?;
        let severity = TradeEventSeverity::from_str(&value.severity)
            .map_err(|_| ConversionError::new("severity", "Failed to parse event severity"))?;
        let source = TradeEventSource::from_str(&value.source)
            .map_err(|_| ConversionError::new("source", "Failed to parse event source"))?;

        Ok(TradeEvent {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse trade event ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            trade_id: Uuid::parse_str(&value.trade_id)
                .map_err(|_| ConversionError::new("trade_id", "Failed to parse trade ID"))?,
            symbol: value.symbol,
            event_type,
            event_date: value.event_date,
            severity,
            notes: value.notes,
            source,
        })
    }
}

impl IntoDomainModel<TradeEvent> for TradeEventSQLite {
    fn into_domain_model(self) -> Result<TradeEvent, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Insertable)]
#[diesel(table_name = trade_events)]
#[diesel(treat_none_as_null = true)]
struct NewTradeEvent {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    trade_id: String,
    symbol: String,
    event_type: String,
    event_date: NaiveDate,
    severity: String,
    notes: Option<String>,
    source: String,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::too_many_lines)]

    use super::*;
    use crate::schema::accounts;
    use crate::workers::{WorkerOrder, WorkerTrade, WorkerTradingVehicle};
    use diesel::Connection;
    use diesel_migrations::*;
    use model::{
        Currency, DraftTrade, OrderAction, OrderCategory, Status, TradeCategory,
        TradingVehicleCategory,
    };
    use rust_decimal_macros::dec;

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn setup_connection() -> SqliteConnection {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();
        conn.run_pending_migrations(MIGRATIONS).unwrap();
        diesel::sql_query("PRAGMA foreign_keys=ON;")
            .execute(&mut conn)
            .unwrap();
        conn.begin_test_transaction().unwrap();
        conn
    }

    fn ensure_account(conn: &mut SqliteConnection, account_id: Uuid) {
        let now = Utc::now().naive_utc();
        diesel::insert_or_ignore_into(accounts::table)
            .values((
                accounts::id.eq(account_id.to_string()),
                accounts::created_at.eq(now),
                accounts::updated_at.eq(now),
                accounts::deleted_at.eq(Option::<NaiveDateTime>::None),
                accounts::name.eq(format!("account-{account_id}")),
                accounts::description.eq("fixture account"),
                accounts::environment.eq("paper"),
                accounts::taxes_percentage.eq("0"),
                accounts::earnings_percentage.eq("0"),
                accounts::account_type.eq("primary"),
                accounts::parent_account_id.eq(Option::<String>::None),
                accounts::broker_kind.eq("alpaca"),
                accounts::broker_account_id.eq(Option::<String>::None),
            ))
            .execute(conn)
            .unwrap();
    }

    fn create_trade(conn: &mut SqliteConnection, account_id: Uuid) -> model::Trade {
        ensure_account(conn, account_id);
        let symbol = format!("T{}", Uuid::new_v4().simple());
        let tv = WorkerTradingVehicle::create(
            conn,
            &symbol,
            None,
            &TradingVehicleCategory::Stock,
            "NASDAQ",
        )
        .unwrap();

        let stop = WorkerOrder::create(
            conn,
            dec!(190),
            &Currency::USD,
            10,
            &OrderAction::Sell,
            &OrderCategory::Stop,
            &tv,
        )
        .unwrap();
        let entry = WorkerOrder::create(
            conn,
            dec!(200),
            &Currency::USD,
            10,
            &OrderAction::Buy,
            &OrderCategory::Limit,
            &tv,
        )
        .unwrap();
        let target = WorkerOrder::create(
            conn,
            dec!(220),
            &Currency::USD,
            10,
            &OrderAction::Sell,
            &OrderCategory::Limit,
            &tv,
        )
        .unwrap();

        WorkerTrade::create(
            conn,
            DraftTrade {
                account: model::Account {
                    id: account_id,
                    ..Default::default()
                },
                trading_vehicle: tv,
                quantity: 10,
                currency: Currency::USD,
                category: TradeCategory::Long,
                thesis: None,
                sector: None,
                asset_class: None,
                context: None,
            },
            &stop,
            &entry,
            &target,
        )
        .unwrap()
    }

    fn event_for(
        trade_id: Uuid,
        symbol: &str,
        event_date: NaiveDate,
        created_at: NaiveDateTime,
    ) -> TradeEvent {
        TradeEvent {
            id: Uuid::new_v4(),
            created_at,
            updated_at: created_at,
            deleted_at: None,
            trade_id,
            symbol: symbol.to_string(),
            event_type: TradeEventType::Earnings,
            event_date,
            severity: TradeEventSeverity::High,
            notes: Some("watch gap risk".to_string()),
            source: TradeEventSource::Manual,
        }
    }

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, day).unwrap()
    }

    fn base_sqlite_row() -> TradeEventSQLite {
        let now = Utc::now().naive_utc();
        TradeEventSQLite {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: Uuid::new_v4().to_string(),
            symbol: "AAPL".to_string(),
            event_type: "earnings".to_string(),
            event_date: date(20),
            severity: "high".to_string(),
            notes: Some("notes".to_string()),
            source: "manual".to_string(),
        }
    }

    fn assert_conversion_error(row: TradeEventSQLite, field: &str) {
        let error = TradeEvent::try_from(row).expect_err("corrupt row must fail conversion");
        assert!(error.to_string().contains(field));
    }

    #[test]
    fn create_read_delete_trade_event_roundtrip() {
        let mut conn = setup_connection();
        let account_id = Uuid::new_v4();
        let trade = create_trade(&mut conn, account_id);
        let now = Utc::now().naive_utc();
        let later = event_for(trade.id, "AAPL", date(22), now);
        let earlier = TradeEvent {
            event_type: TradeEventType::Cpi,
            severity: TradeEventSeverity::Medium,
            notes: None,
            source: TradeEventSource::CalendarApi,
            ..event_for(trade.id, "AAPL", date(20), now)
        };

        assert_eq!(trade.account_id, account_id);
        assert_eq!(trade.status, Status::New);

        let created_later = WorkerTradeEvent::create(&mut conn, &later).unwrap();
        let created_earlier = WorkerTradeEvent::create(&mut conn, &earlier).unwrap();

        assert_eq!(created_later.trade_id, trade.id);
        assert_eq!(created_later.event_type, TradeEventType::Earnings);
        assert_eq!(created_later.severity, TradeEventSeverity::High);
        assert_eq!(created_later.notes.as_deref(), Some("watch gap risk"));

        let events = WorkerTradeEvent::read_for_trade(&mut conn, trade.id).unwrap();
        assert_eq!(
            events.iter().map(|event| event.id).collect::<Vec<_>>(),
            vec![created_earlier.id, created_later.id]
        );
        assert_eq!(
            events.first().and_then(|event| event.notes.as_deref()),
            None
        );
        assert_eq!(
            events.first().map(|event| event.source),
            Some(TradeEventSource::CalendarApi)
        );

        WorkerTradeEvent::delete(&mut conn, created_earlier.id).unwrap();

        let events_after_delete = WorkerTradeEvent::read_for_trade(&mut conn, trade.id).unwrap();
        assert_eq!(
            events_after_delete
                .iter()
                .map(|event| event.id)
                .collect::<Vec<_>>(),
            vec![created_later.id]
        );
    }

    #[test]
    fn read_for_trade_filters_other_trades_and_soft_deleted_rows() {
        let mut conn = setup_connection();
        let account_id = Uuid::new_v4();
        let trade = create_trade(&mut conn, account_id);
        let other_trade = create_trade(&mut conn, account_id);
        let now = Utc::now().naive_utc();

        let kept = WorkerTradeEvent::create(&mut conn, &event_for(trade.id, "MSFT", date(10), now))
            .unwrap();
        let deleted =
            WorkerTradeEvent::create(&mut conn, &event_for(trade.id, "MSFT", date(11), now))
                .unwrap();
        let other =
            WorkerTradeEvent::create(&mut conn, &event_for(other_trade.id, "MSFT", date(9), now))
                .unwrap();

        WorkerTradeEvent::delete(&mut conn, deleted.id).unwrap();

        let events = WorkerTradeEvent::read_for_trade(&mut conn, trade.id).unwrap();

        assert_eq!(
            events.iter().map(|event| event.id).collect::<Vec<_>>(),
            vec![kept.id]
        );
        assert!(!events.iter().any(|event| event.id == deleted.id));
        assert!(!events.iter().any(|event| event.id == other.id));
    }

    #[test]
    fn create_rejects_missing_trade_foreign_key() {
        let mut conn = setup_connection();
        let event = event_for(Uuid::new_v4(), "AAPL", date(20), Utc::now().naive_utc());

        let error =
            WorkerTradeEvent::create(&mut conn, &event).expect_err("missing trade FK should fail");

        assert!(error.to_string().contains("FOREIGN KEY"));
    }

    #[test]
    fn delete_rejects_missing_or_already_deleted_event() {
        let mut conn = setup_connection();
        let trade = create_trade(&mut conn, Uuid::new_v4());
        let event = WorkerTradeEvent::create(
            &mut conn,
            &event_for(trade.id, "AAPL", date(20), Utc::now().naive_utc()),
        )
        .unwrap();

        WorkerTradeEvent::delete(&mut conn, event.id).unwrap();

        let missing_error = WorkerTradeEvent::delete(&mut conn, Uuid::new_v4())
            .expect_err("missing event should fail");
        let already_deleted_error = WorkerTradeEvent::delete(&mut conn, event.id)
            .expect_err("already deleted event should fail");

        assert!(missing_error.to_string().contains("trade event not found"));
        assert!(already_deleted_error
            .to_string()
            .contains("trade event not found"));
    }

    #[test]
    fn trade_event_worker_reports_database_errors() {
        let mut conn = setup_connection();
        diesel::sql_query("DROP TABLE trade_events")
            .execute(&mut conn)
            .expect("trade_events table should drop");
        let event = event_for(Uuid::new_v4(), "AAPL", date(20), Utc::now().naive_utc());

        let create_error = WorkerTradeEvent::create(&mut conn, &event)
            .expect_err("missing table should fail trade event create");
        assert!(create_error.to_string().contains("trade_events"));

        let read_error = WorkerTradeEvent::read_for_trade(&mut conn, Uuid::new_v4())
            .expect_err("missing table should fail trade event read");
        assert!(read_error.to_string().contains("trade_events"));

        let delete_error = WorkerTradeEvent::delete(&mut conn, Uuid::new_v4())
            .expect_err("missing table should fail trade event delete");
        assert!(delete_error.to_string().contains("trade_events"));
    }

    #[test]
    fn trade_event_sqlite_conversion_reports_corrupt_fields() {
        assert_conversion_error(
            TradeEventSQLite {
                id: "not-a-uuid".to_string(),
                ..base_sqlite_row()
            },
            "id",
        );
        assert_conversion_error(
            TradeEventSQLite {
                trade_id: "not-a-uuid".to_string(),
                ..base_sqlite_row()
            },
            "trade_id",
        );
        assert_conversion_error(
            TradeEventSQLite {
                event_type: "split".to_string(),
                ..base_sqlite_row()
            },
            "event_type",
        );
        assert_conversion_error(
            TradeEventSQLite {
                severity: "critical".to_string(),
                ..base_sqlite_row()
            },
            "severity",
        );
        assert_conversion_error(
            TradeEventSQLite {
                source: "email".to_string(),
                ..base_sqlite_row()
            },
            "source",
        );
    }

    #[test]
    fn debug_representation_is_stable() {
        assert_eq!(format!("{WorkerTradeEvent:?}"), "WorkerTradeEvent");
    }
}
