use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::trading_vehicles;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::OptionalExtension;
use model::database::TradingVehicleUpsert;
use model::{TradingVehicle, TradingVehicleCategory};
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use uuid::Uuid;

/// Worker for handling trading vehicle database operations
#[derive(Debug)]
pub struct WorkerTradingVehicle;
impl WorkerTradingVehicle {
    pub fn create(
        connection: &mut SqliteConnection,
        symbol: &str,
        isin: Option<&str>,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let symbol_norm = symbol.trim().to_uppercase();
        let broker_norm = broker.trim().to_lowercase();
        let isin_norm = isin
            .map(|value| value.trim().to_uppercase())
            // Some brokers do not provide ISIN. Keep the DB constraint happy with a stable,
            // broker-scoped synthetic identifier.
            .or_else(|| Some(format!("{}:{}", broker_norm.to_uppercase(), symbol_norm)));

        let new_trading_vehicle = NewTradingVehicle {
            id,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            symbol: symbol_norm,
            isin: isin_norm,
            category: category.to_string(),
            broker: broker_norm,
            broker_asset_id: None,
            exchange: None,
            broker_asset_class: None,
            broker_asset_status: None,
            tradable: None,
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
        };

        let tv = diesel::insert_into(trading_vehicles::table)
            .values(&new_trading_vehicle)
            .get_result::<TradingVehicleSQLite>(connection)
            .map_err(|error| {
                error!("Error creating price: {:?}", error);
                error
            })?
            .into_domain_model()?;
        Ok(tv)
    }

    pub fn upsert(
        connection: &mut SqliteConnection,
        input: TradingVehicleUpsert,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        let now = Utc::now().naive_utc();

        let symbol_norm = input.symbol.trim().to_uppercase();
        let broker_norm = input.broker.trim().to_lowercase();
        let broker_norm_upper = input.broker.trim().to_uppercase();
        let provided_isin = input
            .isin
            .as_deref()
            .map(|value| value.trim().to_uppercase());
        let existing_isin = trading_vehicles::table
            .filter(trading_vehicles::broker.eq(&broker_norm))
            .filter(trading_vehicles::symbol.eq(&symbol_norm))
            .select(trading_vehicles::isin)
            .first::<Option<String>>(connection)
            .optional()?
            .flatten();
        let isin_norm = provided_isin
            .or(existing_isin)
            .or_else(|| Some(format!("{}:{}", broker_norm_upper, symbol_norm)));

        let new_trading_vehicle = NewTradingVehicle {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            symbol: symbol_norm,
            isin: isin_norm,
            category: input.category.to_string(),
            broker: broker_norm,
            broker_asset_id: input
                .broker_asset_id
                .as_deref()
                .map(|v| v.trim().to_string()),
            exchange: input.exchange.as_deref().map(|v| v.trim().to_string()),
            broker_asset_class: input
                .broker_asset_class
                .as_deref()
                .map(|v| v.trim().to_string()),
            broker_asset_status: input
                .broker_asset_status
                .as_deref()
                .map(|v| v.trim().to_string()),
            tradable: input.tradable,
            marginable: input.marginable,
            shortable: input.shortable,
            easy_to_borrow: input.easy_to_borrow,
            fractionable: input.fractionable,
        };

        let tv = diesel::insert_into(trading_vehicles::table)
            .values(&new_trading_vehicle)
            .on_conflict((trading_vehicles::broker, trading_vehicles::symbol))
            .do_update()
            .set((
                trading_vehicles::updated_at.eq(now),
                trading_vehicles::deleted_at.eq::<Option<NaiveDateTime>>(None),
                trading_vehicles::isin.eq(new_trading_vehicle.isin.clone()),
                trading_vehicles::category.eq(new_trading_vehicle.category.clone()),
                trading_vehicles::broker_asset_id.eq(new_trading_vehicle.broker_asset_id.clone()),
                trading_vehicles::exchange.eq(new_trading_vehicle.exchange.clone()),
                trading_vehicles::broker_asset_class
                    .eq(new_trading_vehicle.broker_asset_class.clone()),
                trading_vehicles::broker_asset_status
                    .eq(new_trading_vehicle.broker_asset_status.clone()),
                trading_vehicles::tradable.eq(new_trading_vehicle.tradable),
                trading_vehicles::marginable.eq(new_trading_vehicle.marginable),
                trading_vehicles::shortable.eq(new_trading_vehicle.shortable),
                trading_vehicles::easy_to_borrow.eq(new_trading_vehicle.easy_to_borrow),
                trading_vehicles::fractionable.eq(new_trading_vehicle.fractionable),
            ))
            .get_result::<TradingVehicleSQLite>(connection)
            .map_err(|error| {
                error!("Error upserting trading vehicle: {:?}", error);
                error
            })?
            .into_domain_model()?;

        Ok(tv)
    }

    pub fn read_all(
        connection: &mut SqliteConnection,
    ) -> Result<Vec<TradingVehicle>, Box<dyn Error>> {
        let tvs = trading_vehicles::table
            .filter(trading_vehicles::deleted_at.is_null())
            .load::<TradingVehicleSQLite>(connection)
            .map_err(|error| {
                error!("Error creating price: {:?}", error);
                error
            })?
            .into_domain_models()?;
        Ok(tvs)
    }

    pub fn read(
        connection: &mut SqliteConnection,
        id: Uuid,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        let tv = trading_vehicles::table
            .filter(trading_vehicles::id.eq(id.to_string()))
            .filter(trading_vehicles::deleted_at.is_null())
            .first::<TradingVehicleSQLite>(connection)
            .map_err(|error| {
                error!("Error reading trading vehicle: {:?}", error);
                error
            })?
            .into_domain_model()?;
        Ok(tv)
    }
}

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = trading_vehicles)]
struct TradingVehicleSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    symbol: String,
    isin: Option<String>,
    category: String,
    broker: String,
    broker_asset_id: Option<String>,
    exchange: Option<String>,
    broker_asset_class: Option<String>,
    broker_asset_status: Option<String>,
    tradable: Option<bool>,
    marginable: Option<bool>,
    shortable: Option<bool>,
    easy_to_borrow: Option<bool>,
    fractionable: Option<bool>,
}

impl TryFrom<TradingVehicleSQLite> for TradingVehicle {
    type Error = ConversionError;

    fn try_from(value: TradingVehicleSQLite) -> Result<Self, Self::Error> {
        Ok(TradingVehicle {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse trading vehicle ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            symbol: value.symbol,
            isin: value.isin,
            category: TradingVehicleCategory::from_str(&value.category).map_err(|_| {
                ConversionError::new("category", "Failed to parse trading vehicle category")
            })?,
            broker: value.broker,
            broker_asset_id: value.broker_asset_id,
            exchange: value.exchange,
            broker_asset_class: value.broker_asset_class,
            broker_asset_status: value.broker_asset_status,
            tradable: value.tradable,
            marginable: value.marginable,
            shortable: value.shortable,
            easy_to_borrow: value.easy_to_borrow,
            fractionable: value.fractionable,
        })
    }
}

impl IntoDomainModel<TradingVehicle> for TradingVehicleSQLite {
    fn into_domain_model(self) -> Result<TradingVehicle, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = trading_vehicles)]
pub struct NewTradingVehicle {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    symbol: String,
    isin: Option<String>,
    category: String,
    broker: String,
    broker_asset_id: Option<String>,
    exchange: Option<String>,
    broker_asset_class: Option<String>,
    broker_asset_status: Option<String>,
    tradable: Option<bool>,
    marginable: Option<bool>,
    shortable: Option<bool>,
    easy_to_borrow: Option<bool>,
    fractionable: Option<bool>,
}
#[cfg(test)]
mod tests {
    use super::*;
    use diesel_migrations::*;

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        // This will run the necessary migrations.
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    fn create_apple_trading_vehicle(conn: &mut SqliteConnection) -> TradingVehicle {
        WorkerTradingVehicle::create(
            conn,
            "AAPl",
            Some("uS0378331005"),
            &TradingVehicleCategory::Fiat,
            "NASDAQ",
        )
        .expect("Error creating trading_vehicle")
    }

    #[test]
    fn test_create_trading_vehicle() {
        let mut conn = establish_connection();

        let trading_vehicle = create_apple_trading_vehicle(&mut conn);

        assert_eq!(trading_vehicle.symbol, "AAPL"); // symbol should be uppercase
        assert_eq!(trading_vehicle.isin, Some("US0378331005".to_string())); // isin should be uppercase
        assert_eq!(trading_vehicle.category, TradingVehicleCategory::Fiat);
        assert_eq!(trading_vehicle.broker, "nasdaq"); // broker should be lowercase
        assert_eq!(trading_vehicle.updated_at, trading_vehicle.created_at); // created_at and updated_at should be the same
        assert_eq!(trading_vehicle.created_at, trading_vehicle.updated_at); // created_at and updated_at should be the same
        assert_eq!(trading_vehicle.deleted_at, None);
    }

    #[test]
    fn test_create_trading_vehicle_same_broker_symbol_conflicts() {
        let mut conn = establish_connection();
        create_apple_trading_vehicle(&mut conn);
        WorkerTradingVehicle::create(
            &mut conn,
            "AAPl",
            Some("uS0378331005"),
            &TradingVehicleCategory::Fiat,
            "NASDAQ",
        )
        .expect_err("Error creating trading_vehicle with same broker+symbol");
    }

    #[test]
    fn test_read_trading_vehicle() {
        let mut conn = establish_connection();

        WorkerTradingVehicle::create(
            &mut conn,
            "TSLA",
            Some("US88160R1014"),
            &TradingVehicleCategory::Fiat,
            "NASDAQ",
        )
        .unwrap();

        create_apple_trading_vehicle(&mut conn);

        let read_trading_vehicles =
            WorkerTradingVehicle::read_all(&mut conn).expect("Error reading trading_vehicle");

        assert_eq!(read_trading_vehicles.len(), 2);
    }

    #[test]
    fn test_upsert_updates_metadata_fields() {
        let mut conn = establish_connection();
        let input = TradingVehicleUpsert {
            symbol: "aapl".to_string(),
            isin: None,
            category: TradingVehicleCategory::Stock,
            broker: "alpaca".to_string(),
            broker_asset_id: Some("904837e3-3b76-47ec-b432-046db621571b".to_string()),
            exchange: Some("NASDAQ".to_string()),
            broker_asset_class: Some("us_equity".to_string()),
            broker_asset_status: Some("active".to_string()),
            tradable: Some(true),
            marginable: Some(true),
            shortable: Some(false),
            easy_to_borrow: Some(false),
            fractionable: Some(true),
        };

        let created = WorkerTradingVehicle::upsert(&mut conn, input.clone()).unwrap();
        assert_eq!(created.symbol, "AAPL");
        assert_eq!(created.broker, "alpaca");
        assert_eq!(created.broker_asset_id, input.broker_asset_id);
        assert_eq!(created.exchange, input.exchange);

        let mut updated_input = input;
        updated_input.exchange = Some("NYSE".to_string());
        updated_input.shortable = Some(true);
        let updated = WorkerTradingVehicle::upsert(&mut conn, updated_input.clone()).unwrap();
        assert_eq!(updated.exchange, updated_input.exchange);
        assert_eq!(updated.shortable, updated_input.shortable);
    }

    #[test]
    fn test_upsert_preserves_existing_real_isin_when_input_is_none() {
        let mut conn = establish_connection();

        let created = WorkerTradingVehicle::create(
            &mut conn,
            "AAPL",
            Some("US0378331005"),
            &TradingVehicleCategory::Stock,
            "alpaca",
        )
        .unwrap();
        assert_eq!(created.isin, Some("US0378331005".to_string()));

        let updated = WorkerTradingVehicle::upsert(
            &mut conn,
            TradingVehicleUpsert {
                symbol: "aapl".to_string(),
                isin: None,
                category: TradingVehicleCategory::Stock,
                broker: "alpaca".to_string(),
                broker_asset_id: Some("904837e3-3b76-47ec-b432-046db621571b".to_string()),
                exchange: Some("NASDAQ".to_string()),
                broker_asset_class: Some("us_equity".to_string()),
                broker_asset_status: Some("active".to_string()),
                tradable: Some(true),
                marginable: Some(true),
                shortable: Some(false),
                easy_to_borrow: Some(false),
                fractionable: Some(true),
            },
        )
        .unwrap();

        assert_eq!(updated.isin, Some("US0378331005".to_string()));
        assert_eq!(
            updated.broker_asset_id,
            Some("904837e3-3b76-47ec-b432-046db621571b".to_string())
        );
    }
}
