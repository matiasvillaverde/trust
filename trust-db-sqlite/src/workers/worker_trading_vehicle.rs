use std::error::Error;
use std::str::FromStr;

use crate::schema::trading_vehicles;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use tracing::error;
use trust_model::{TradingVehicle, TradingVehicleCategory};
use uuid::Uuid;

pub struct WorkerTradingVehicle;
impl WorkerTradingVehicle {
    pub fn create(
        connection: &mut SqliteConnection,
        symbol: &str,
        isin: &str,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn Error>> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let new_trading_vehicle = NewTradingVehicle {
            id,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            symbol: symbol.to_uppercase(),
            isin: isin.to_uppercase(),
            category: category.to_string(),
            broker: broker.to_lowercase(),
        };

        let tv = diesel::insert_into(trading_vehicles::table)
            .values(&new_trading_vehicle)
            .get_result::<TradingVehicleSQLite>(connection)
            .map(|tv| tv.domain_model())
            .map_err(|error| {
                error!("Error creating price: {:?}", error);
                error
            })?;
        Ok(tv)
    }

    pub fn read_all(
        connection: &mut SqliteConnection,
    ) -> Result<Vec<TradingVehicle>, Box<dyn Error>> {
        let tvs = trading_vehicles::table
            .filter(trading_vehicles::deleted_at.is_null())
            .load::<TradingVehicleSQLite>(connection)
            .map(|tv: Vec<TradingVehicleSQLite>| {
                tv.into_iter().map(|tv| tv.domain_model()).collect()
            })
            .map_err(|error| {
                error!("Error creating price: {:?}", error);
                error
            })?;
        Ok(tvs)
    }
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = trading_vehicles)]
struct TradingVehicleSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    symbol: String,
    isin: String,
    category: String,
    broker: String,
}

impl TradingVehicleSQLite {
    fn domain_model(self) -> TradingVehicle {
        TradingVehicle {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            symbol: self.symbol,
            isin: self.isin,
            category: TradingVehicleCategory::from_str(&self.category).unwrap(),
            broker: self.broker,
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = trading_vehicles)]
pub struct NewTradingVehicle {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    symbol: String,
    isin: String,
    category: String,
    broker: String,
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
            "uS0378331005",
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
        assert_eq!(trading_vehicle.isin, "US0378331005"); // isin should be uppercase
        assert_eq!(trading_vehicle.category, TradingVehicleCategory::Fiat);
        assert_eq!(trading_vehicle.broker, "nasdaq"); // broker should be lowercase
        assert_eq!(trading_vehicle.updated_at, trading_vehicle.created_at); // created_at and updated_at should be the same
        assert_eq!(trading_vehicle.created_at, trading_vehicle.updated_at); // created_at and updated_at should be the same
        assert_eq!(trading_vehicle.deleted_at, None);
    }

    #[test]
    fn test_create_trading_vehicle_same_isin() {
        let mut conn = establish_connection();
        create_apple_trading_vehicle(&mut conn);
        WorkerTradingVehicle::create(
            &mut conn,
            "AAPl",
            "uS0378331005",
            &TradingVehicleCategory::Fiat,
            "NASDAQ",
        )
        .expect_err("Error creating trading_vehicle with same isin");
    }

    #[test]
    fn test_read_trading_vehicle() {
        let mut conn = establish_connection();

        WorkerTradingVehicle::create(
            &mut conn,
            "TSLA",
            "US88160R1014",
            &TradingVehicleCategory::Fiat,
            "NASDAQ",
        )
        .unwrap();

        create_apple_trading_vehicle(&mut conn);

        let read_trading_vehicles =
            WorkerTradingVehicle::read_all(&mut conn).expect("Error reading trading_vehicle");

        assert_eq!(read_trading_vehicles.len(), 2);
    }
}
