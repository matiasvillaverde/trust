use crate::error::{ConversionError, IntoDomainModel};
use crate::schema::orders::{self};
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::{
    Currency, Order, OrderAction, OrderCategory, OrderStatus, TimeInForce, TradingVehicle,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use uuid::Uuid;

/// Worker for handling order database operations
#[derive(Debug)]
pub struct WorkerOrder;
impl WorkerOrder {
    pub fn create(
        connection: &mut SqliteConnection,
        unit_price: Decimal,
        currency: &Currency,
        quantity: i64,
        action: &OrderAction,
        category: &OrderCategory,
        trading_vehicle: &TradingVehicle,
    ) -> Result<Order, Box<dyn Error>> {
        let new_order = NewOrder {
            #[allow(clippy::cast_possible_truncation)]
            quantity: quantity as i32,
            unit_price: unit_price.to_string(),
            category: category.to_string(),
            currency: currency.to_string(),
            trading_vehicle_id: trading_vehicle.id.to_string(),
            action: action.to_string(),
            ..Default::default()
        };

        let order = diesel::insert_into(orders::table)
            .values(&new_order)
            .get_result::<OrderSQLite>(connection)
            .map_err(|error| {
                error!("Error creating order: {:?}", error);
                error
            })?
            .into_domain_model()?;
        Ok(order)
    }

    pub fn read(connection: &mut SqliteConnection, id: Uuid) -> Result<Order, Box<dyn Error>> {
        let order = orders::table
            .filter(orders::id.eq(id.to_string()))
            .first::<OrderSQLite>(connection)
            .map_err(|error| {
                error!("Error reading account: {:?}", error);
                error
            })?
            .into_domain_model()?;
        Ok(order)
    }

    pub fn update(
        connection: &mut SqliteConnection,
        order: &Order,
    ) -> Result<Order, Box<dyn Error>> {
        let now: NaiveDateTime = Utc::now().naive_utc();
        diesel::update(orders::table)
            .filter(orders::id.eq(&order.id.to_string()))
            .set((
                orders::updated_at.eq(now),
                orders::broker_order_id.eq(order.broker_order_id.map(|id| id.to_string())),
                orders::status.eq(order.status.to_string()),
                #[allow(clippy::cast_possible_truncation)]
                orders::filled_quantity.eq(Some(order.filled_quantity as i32)),
                orders::average_filled_price
                    .eq(order.average_filled_price.map(|price| price.to_string())),
                orders::submitted_at.eq(order.submitted_at),
                orders::filled_at.eq(order.filled_at),
                orders::expired_at.eq(order.expired_at),
                orders::category.eq(order.category.to_string()),
                orders::cancelled_at.eq(order.cancelled_at),
                orders::closed_at.eq(order.closed_at),
            ))
            .execute(connection)?;

        WorkerOrder::read(connection, order.id)
    }

    pub fn update_price(
        connection: &mut SqliteConnection,
        order: &Order,
        new_price: Decimal,
        new_broker_id: Uuid,
    ) -> Result<Order, Box<dyn Error>> {
        let now: NaiveDateTime = Utc::now().naive_utc();
        diesel::update(orders::table)
            .filter(orders::id.eq(&order.id.to_string()))
            .set((
                orders::updated_at.eq(now),
                orders::unit_price.eq(new_price.to_string()),
                orders::broker_order_id.eq(new_broker_id.to_string()),
            ))
            .execute(connection)?;

        WorkerOrder::read(connection, order.id)
    }

    pub fn update_submitted_at(
        connection: &mut SqliteConnection,
        order: &Order,
        broker_order_id: Uuid,
    ) -> Result<Order, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        diesel::update(orders::table)
            .filter(orders::id.eq(&order.id.to_string()))
            .set((
                orders::submitted_at.eq(now),
                orders::broker_order_id.eq(broker_order_id.to_string()),
                orders::updated_at.eq(now),
            ))
            .execute(connection)?;

        WorkerOrder::read(connection, order.id)
    }

    pub fn update_filled_at(
        connection: &mut SqliteConnection,
        order: &Order,
    ) -> Result<Order, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        diesel::update(orders::table)
            .filter(orders::id.eq(&order.id.to_string()))
            .set((orders::filled_at.eq(now), orders::updated_at.eq(now)))
            .execute(connection)?;

        WorkerOrder::read(connection, order.id)
    }

    pub fn update_closed_at(
        connection: &mut SqliteConnection,
        order: &Order,
    ) -> Result<Order, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        diesel::update(orders::table)
            .filter(orders::id.eq(&order.id.to_string()))
            .set((orders::closed_at.eq(now), orders::updated_at.eq(now)))
            .execute(connection)?;

        WorkerOrder::read(connection, order.id)
    }
}

#[derive(Debug, Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = orders)]
struct OrderSQLite {
    id: String,
    broker_order_id: Option<String>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    unit_price: String,
    currency: String,
    quantity: i32,
    category: String,
    trading_vehicle_id: String,
    action: String,
    status: String,
    time_in_force: String,
    trailing_percentage: Option<String>,
    trailing_price: Option<String>,
    filled_quantity: Option<i32>,
    average_filled_price: Option<String>,
    extended_hours: bool,
    submitted_at: Option<NaiveDateTime>,
    filled_at: Option<NaiveDateTime>,
    expired_at: Option<NaiveDateTime>,
    cancelled_at: Option<NaiveDateTime>,
    closed_at: Option<NaiveDateTime>,
}

impl TryFrom<OrderSQLite> for Order {
    type Error = ConversionError;

    fn try_from(value: OrderSQLite) -> Result<Self, Self::Error> {
        Ok(Order {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse order ID"))?,
            broker_order_id: value
                .broker_order_id
                .and_then(|id| Uuid::parse_str(&id).ok()),
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            unit_price: Decimal::from_str(&value.unit_price)
                .map_err(|_| ConversionError::new("unit_price", "Failed to parse unit price"))?,
            currency: Currency::from_str(&value.currency)
                .map_err(|_| ConversionError::new("currency", "Failed to parse currency"))?,
            #[allow(clippy::cast_sign_loss)]
            quantity: (value.quantity as i64).max(0) as u64,
            action: OrderAction::from_str(&value.action)
                .map_err(|_| ConversionError::new("action", "Failed to parse order action"))?,
            category: OrderCategory::from_str(&value.category)
                .map_err(|_| ConversionError::new("category", "Failed to parse order category"))?,
            status: OrderStatus::from_str(&value.status)
                .map_err(|_| ConversionError::new("status", "Failed to parse order status"))?,
            trading_vehicle_id: Uuid::parse_str(&value.trading_vehicle_id).map_err(|_| {
                ConversionError::new("trading_vehicle_id", "Failed to parse trading vehicle ID")
            })?,
            time_in_force: TimeInForce::from_str(&value.time_in_force).map_err(|_| {
                ConversionError::new("time_in_force", "Failed to parse time in force")
            })?,
            trailing_percent: value
                .trailing_percentage
                .and_then(|p| Decimal::from_str(&p).ok()),
            trailing_price: value
                .trailing_price
                .and_then(|p| Decimal::from_str(&p).ok()),
            #[allow(clippy::cast_sign_loss)]
            filled_quantity: (value.filled_quantity.unwrap_or(0) as i64).max(0) as u64,
            average_filled_price: value
                .average_filled_price
                .and_then(|p| Decimal::from_str(&p).ok()),
            extended_hours: value.extended_hours,
            submitted_at: value.submitted_at,
            filled_at: value.filled_at,
            expired_at: value.expired_at,
            cancelled_at: value.cancelled_at,
            closed_at: value.closed_at,
        })
    }
}

impl IntoDomainModel<Order> for OrderSQLite {
    fn into_domain_model(self) -> Result<Order, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Insertable)]
#[diesel(table_name = orders)]
#[diesel(treat_none_as_null = true)]
struct NewOrder {
    id: String,
    broker_order_id: Option<String>,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    unit_price: String,
    currency: String,
    quantity: i32,
    category: String,
    trading_vehicle_id: String,
    action: String,
    status: String,
    time_in_force: String,
    trailing_percentage: Option<String>,
    trailing_price: Option<String>,
    filled_quantity: Option<i32>,
    average_filled_price: Option<String>,
    extended_hours: bool,
    submitted_at: Option<NaiveDateTime>,
    filled_at: Option<NaiveDateTime>,
    expired_at: Option<NaiveDateTime>,
    cancelled_at: Option<NaiveDateTime>,
    closed_at: Option<NaiveDateTime>,
}

impl Default for NewOrder {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        NewOrder {
            id: Uuid::new_v4().to_string(),
            broker_order_id: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            unit_price: dec!(0).to_string(),
            currency: Currency::default().to_string(),
            quantity: 0,
            category: OrderCategory::Limit.to_string(),
            trading_vehicle_id: Uuid::new_v4().to_string(),
            action: OrderAction::Buy.to_string(),
            status: OrderStatus::New.to_string(),
            time_in_force: TimeInForce::UntilCanceled.to_string(),
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
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::workers::WorkerTradingVehicle;

    use super::*;
    use diesel_migrations::*;
    use model::{Currency, TradingVehicleCategory};
    use rust_decimal_macros::dec;

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
            dec!(150.00),
            &Currency::USD,
            100,
            &OrderAction::Buy,
            &OrderCategory::Limit,
            &trading_vehicle,
        )
        .expect("Error creating order");

        assert_eq!(order.unit_price, dec!(150.00));
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
