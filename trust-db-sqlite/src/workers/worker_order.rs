use crate::schema::orders;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use trust_model::{Currency, Order, OrderAction, OrderCategory, TimeInForce, TradingVehicle};
use uuid::Uuid;

use super::WorkerPrice;

pub struct WorkerOrder;
impl WorkerOrder {
    pub fn create(
        connection: &mut SqliteConnection,
        price: Decimal,
        currency: &Currency,
        quantity: i64,
        action: &OrderAction,
        category: &OrderCategory,
        trading_vehicle: &TradingVehicle,
    ) -> Result<Order, Box<dyn Error>> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let price = WorkerPrice::create(connection, currency, price)?;

        let new_order = NewOrder {
            id,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            price_id: price.id.to_string(),
            quantity,
            category: category.to_string(),
            trading_vehicle_id: trading_vehicle.id.to_string(),
            action: action.to_string(),
            time_in_force: TimeInForce::default().to_string(),
            trailing_percentage: None,
            trailing_price: None,
            filled_quantity: 0,
            average_filled_price: None,
            extended_hours: false,
            submitted_at: None,
            filled_at: None,
            expired_at: None,
            cancelled_at: None,
            closed_at: None,
        };

        let order = diesel::insert_into(orders::table)
            .values(&new_order)
            .get_result::<OrderSQLite>(connection)
            .map(|order| order.domain_model(connection))
            .map_err(|error| {
                error!("Error creating order: {:?}", error);
                error
            })?;
        Ok(order)
    }

    pub fn read(connection: &mut SqliteConnection, id: Uuid) -> Result<Order, Box<dyn Error>> {
        let order = orders::table
            .filter(orders::id.eq(id.to_string()))
            .first::<OrderSQLite>(connection)
            .map(|order| order.domain_model(connection))
            .map_err(|error| {
                error!("Error reading account: {:?}", error);
                error
            })?;
        Ok(order)
    }

    pub fn update_submitted_at(
        connection: &mut SqliteConnection,
        order: &Order,
    ) -> Result<Order, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        diesel::update(orders::table)
            .filter(orders::id.eq(&order.id.to_string()))
            .set(orders::submitted_at.eq(now))
            .execute(connection)?;

        return WorkerOrder::read(connection, order.id);
    }

    pub fn update_filled_at(
        connection: &mut SqliteConnection,
        order: &Order,
    ) -> Result<Order, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        diesel::update(orders::table)
            .filter(orders::id.eq(&order.id.to_string()))
            .set(orders::filled_at.eq(now))
            .execute(connection)?;

        return WorkerOrder::read(connection, order.id);
    }

    pub fn update_closed_at(
        connection: &mut SqliteConnection,
        order: &Order,
    ) -> Result<Order, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        diesel::update(orders::table)
            .filter(orders::id.eq(&order.id.to_string()))
            .set(orders::closed_at.eq(now))
            .execute(connection)?;

        return WorkerOrder::read(connection, order.id);
    }
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = orders)]
struct OrderSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    price_id: String,
    quantity: i64,
    category: String,
    trading_vehicle_id: String,
    action: String,
    time_in_force: String,
    trailing_percentage: Option<String>,
    trailing_price: Option<String>,
    filled_quantity: i64,
    average_filled_price: Option<String>,
    extended_hours: bool,
    submitted_at: Option<NaiveDateTime>,
    filled_at: Option<NaiveDateTime>,
    expired_at: Option<NaiveDateTime>,
    cancelled_at: Option<NaiveDateTime>,
    closed_at: Option<NaiveDateTime>,
}

impl OrderSQLite {
    fn domain_model(self, connection: &mut SqliteConnection) -> Order {
        Order {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            unit_price: WorkerPrice::read(connection, Uuid::parse_str(&self.price_id).unwrap())
                .unwrap(),
            quantity: self.quantity as u64,
            action: OrderAction::from_str(&self.action).unwrap(),
            category: OrderCategory::from_str(&self.category).unwrap(),
            trading_vehicle_id: Uuid::parse_str(&self.trading_vehicle_id).unwrap(),
            time_in_force: TimeInForce::from_str(&self.time_in_force).unwrap(),
            trailing_percent: self
                .trailing_percentage
                .map(|p| Decimal::from_str(&p).unwrap()),
            trailing_price: self.trailing_price.map(|p| Decimal::from_str(&p).unwrap()),
            filled_quantity: self.filled_quantity as u64,
            average_filled_price: self
                .average_filled_price
                .map(|p| Decimal::from_str(&p).unwrap()),
            extended_hours: self.extended_hours,
            submitted_at: self.submitted_at,
            filled_at: self.filled_at,
            expired_at: self.expired_at,
            cancelled_at: self.cancelled_at,
            closed_at: self.closed_at,
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = orders)]
#[diesel(treat_none_as_null = true)]
struct NewOrder {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    price_id: String,
    quantity: i64,
    category: String,
    trading_vehicle_id: String,
    action: String,
    time_in_force: String,
    trailing_percentage: Option<String>,
    trailing_price: Option<String>,
    filled_quantity: i64,
    average_filled_price: Option<String>,
    extended_hours: bool,
    submitted_at: Option<NaiveDateTime>,
    filled_at: Option<NaiveDateTime>,
    expired_at: Option<NaiveDateTime>,
    cancelled_at: Option<NaiveDateTime>,
    closed_at: Option<NaiveDateTime>,
}

#[cfg(test)]
mod tests {
    use crate::workers::WorkerTradingVehicle;

    use super::*;
    use diesel_migrations::*;
    use rust_decimal_macros::dec;
    use trust_model::{Currency, TradingVehicleCategory};

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    // Declare a test database connection in memory.
    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        // This will run the necessary migrations.
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    #[test]
    fn test_create_order() {
        let mut conn = establish_connection();

        let trading_vehicle = WorkerTradingVehicle::create(
            &mut conn,
            "AAPL",
            "isin",
            &TradingVehicleCategory::Crypto,
            "NASDAQ",
        )
        .unwrap();

        // Create a new order record
        let order = WorkerOrder::create(
            &mut conn,
            dec!(100.00),
            &Currency::USD,
            100,
            &OrderAction::Buy,
            &OrderCategory::Limit,
            &trading_vehicle,
        )
        .expect("Error creating order");

        assert_eq!(order.unit_price.amount, dec!(100.00));
        assert_eq!(order.unit_price.currency, Currency::USD);
        assert_eq!(order.quantity, 100);
        assert_eq!(order.action, OrderAction::Buy);
        assert_eq!(order.category, OrderCategory::Limit);
        assert_eq!(order.trading_vehicle_id, trading_vehicle.id);
        assert_eq!(order.filled_at, None);
        assert_eq!(order.closed_at, None);
        assert_eq!(order.created_at, order.updated_at);
        assert_eq!(order.deleted_at, None);
    }
}
