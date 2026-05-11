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
                orders::broker_order_id.eq(order.broker_order_id.clone()),
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

        let mut updated = order.clone();
        updated.updated_at = now;
        Ok(updated)
    }

    pub fn update_price(
        connection: &mut SqliteConnection,
        order: &Order,
        new_price: Decimal,
        new_broker_id: String,
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

        let mut updated = order.clone();
        updated.unit_price = new_price;
        updated.broker_order_id = Some(new_broker_id);
        updated.updated_at = now;
        Ok(updated)
    }

    pub fn update_submitted_at(
        connection: &mut SqliteConnection,
        order: &Order,
        broker_order_id: String,
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

        let mut updated = order.clone();
        updated.submitted_at = Some(now);
        updated.broker_order_id = Some(broker_order_id);
        updated.updated_at = now;
        Ok(updated)
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

        let mut updated = order.clone();
        updated.filled_at = Some(now);
        updated.updated_at = now;
        Ok(updated)
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

        let mut updated = order.clone();
        updated.closed_at = Some(now);
        updated.updated_at = now;
        Ok(updated)
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
            broker_order_id: value.broker_order_id,
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

    fn trading_vehicle(conn: &mut SqliteConnection) -> TradingVehicle {
        let symbol = format!("T{}", Uuid::new_v4().simple());
        WorkerTradingVehicle::create(
            conn,
            &symbol,
            None,
            &TradingVehicleCategory::Crypto,
            "NASDAQ",
        )
        .unwrap()
    }

    fn create_order(conn: &mut SqliteConnection) -> Order {
        let trading_vehicle = trading_vehicle(conn);
        WorkerOrder::create(
            conn,
            dec!(150.00),
            &Currency::USD,
            100,
            &OrderAction::Buy,
            &OrderCategory::Limit,
            &trading_vehicle,
        )
        .expect("Error creating order")
    }

    fn base_sqlite_order() -> OrderSQLite {
        let now = Utc::now().naive_utc();
        OrderSQLite {
            id: Uuid::new_v4().to_string(),
            broker_order_id: Some("broker-1".to_string()),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            unit_price: "150.25".to_string(),
            currency: Currency::USD.to_string(),
            quantity: 100,
            category: OrderCategory::Limit.to_string(),
            trading_vehicle_id: Uuid::new_v4().to_string(),
            action: OrderAction::Buy.to_string(),
            status: OrderStatus::New.to_string(),
            time_in_force: TimeInForce::UntilCanceled.to_string(),
            trailing_percentage: Some("1.5".to_string()),
            trailing_price: Some("2.5".to_string()),
            filled_quantity: Some(10),
            average_filled_price: Some("151.25".to_string()),
            extended_hours: true,
            submitted_at: Some(now),
            filled_at: None,
            expired_at: None,
            cancelled_at: None,
            closed_at: None,
        }
    }

    fn assert_conversion_error(row: OrderSQLite, field: &str) {
        let error = Order::try_from(row).expect_err("corrupt order row must fail conversion");
        assert!(error.to_string().contains(field));
    }

    #[test]
    fn test_create_order() {
        let mut conn = establish_connection();

        let trading_vehicle = trading_vehicle(&mut conn);

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

    #[test]
    fn create_order_roundtrips_stop_limit_category() {
        let mut conn = establish_connection();
        let trading_vehicle = trading_vehicle(&mut conn);

        let order = WorkerOrder::create(
            &mut conn,
            dec!(95.00),
            &Currency::USD,
            100,
            &OrderAction::Sell,
            &OrderCategory::StopLimit,
            &trading_vehicle,
        )
        .expect("stop-limit order should create");
        let read = WorkerOrder::read(&mut conn, order.id).expect("stop-limit order should read");

        assert_eq!(read.category, OrderCategory::StopLimit);
    }

    #[test]
    fn create_and_read_report_missing_orders_table_errors() {
        let mut conn = establish_connection();
        let trading_vehicle = trading_vehicle(&mut conn);
        diesel::sql_query("DROP TABLE orders")
            .execute(&mut conn)
            .expect("orders table should drop");

        let create_error = WorkerOrder::create(
            &mut conn,
            dec!(150.00),
            &Currency::USD,
            100,
            &OrderAction::Buy,
            &OrderCategory::Limit,
            &trading_vehicle,
        )
        .expect_err("missing table should fail order create");
        assert!(create_error.to_string().contains("orders"));

        let read_error = WorkerOrder::read(&mut conn, Uuid::new_v4())
            .expect_err("missing table should fail order read");
        assert!(read_error.to_string().contains("orders"));
    }

    fn assert_order_can_be_read(conn: &mut SqliteConnection, order: &Order) -> Order {
        let read = WorkerOrder::read(conn, order.id).expect("created order should read");
        assert_eq!(read.id, order.id);
        assert_eq!(read.unit_price, dec!(150.00));
        read
    }

    fn submit_order(conn: &mut SqliteConnection, order: &Order) -> Order {
        let submitted = WorkerOrder::update_submitted_at(conn, order, "broker-entry-1".to_string())
            .expect("submit timestamp should update");
        assert_eq!(
            submitted.broker_order_id,
            Some("broker-entry-1".to_string())
        );
        assert!(submitted.submitted_at.is_some());
        submitted
    }

    fn fill_order(conn: &mut SqliteConnection, order: &Order) -> Order {
        let filled = WorkerOrder::update_filled_at(conn, order).expect("filled timestamp update");
        assert!(filled.filled_at.is_some());
        filled
    }

    fn persist_fill_details(conn: &mut SqliteConnection, filled: &Order) -> Order {
        let mut filled_snapshot = filled.clone();
        filled_snapshot.status = OrderStatus::Filled;
        filled_snapshot.filled_quantity = 100;
        filled_snapshot.average_filled_price = Some(dec!(151.25));
        filled_snapshot.category = OrderCategory::Market;
        let updated = WorkerOrder::update(conn, &filled_snapshot).expect("order update");
        assert_eq!(updated.status, OrderStatus::Filled);
        assert_eq!(updated.average_filled_price, Some(dec!(151.25)));
        updated
    }

    fn reprice_order(conn: &mut SqliteConnection, order: &Order) -> Order {
        let repriced =
            WorkerOrder::update_price(conn, order, dec!(149.75), "broker-entry-2".to_string())
                .expect("price update");
        assert_eq!(repriced.unit_price, dec!(149.75));
        assert_eq!(repriced.broker_order_id, Some("broker-entry-2".to_string()));
        repriced
    }

    fn close_order(conn: &mut SqliteConnection, order: &Order) -> Order {
        let closed = WorkerOrder::update_closed_at(conn, order).expect("closed timestamp update");
        assert!(closed.closed_at.is_some());
        closed
    }

    fn assert_persisted_fill_details(order: &Order) {
        assert_eq!(order.status, OrderStatus::Filled);
        assert_eq!(order.category, OrderCategory::Market);
        assert_eq!(order.filled_quantity, 100);
        assert_eq!(order.average_filled_price, Some(dec!(151.25)));
    }

    fn assert_persisted_broker_and_timestamps(order: &Order) {
        assert_eq!(order.unit_price, dec!(149.75));
        assert_eq!(order.broker_order_id, Some("broker-entry-2".to_string()));
        assert!(order.submitted_at.is_some());
        assert!(order.filled_at.is_some());
        assert!(order.closed_at.is_some());
    }

    #[test]
    fn read_update_price_and_lifecycle_timestamps_are_persisted() {
        let mut conn = establish_connection();
        let order = create_order(&mut conn);

        let read = assert_order_can_be_read(&mut conn, &order);
        let submitted = submit_order(&mut conn, &read);
        let filled = fill_order(&mut conn, &submitted);
        let updated = persist_fill_details(&mut conn, &filled);
        let repriced = reprice_order(&mut conn, &updated);
        close_order(&mut conn, &repriced);

        let persisted = WorkerOrder::read(&mut conn, order.id).expect("updated order should read");
        assert_persisted_fill_details(&persisted);
        assert_persisted_broker_and_timestamps(&persisted);
    }

    #[test]
    fn order_sqlite_conversion_clamps_negative_quantities_and_ignores_invalid_optional_prices() {
        let row = OrderSQLite {
            quantity: -10,
            trailing_percentage: Some("not-a-decimal".to_string()),
            trailing_price: Some("still-not-decimal".to_string()),
            filled_quantity: Some(-5),
            average_filled_price: Some("bad-price".to_string()),
            ..base_sqlite_order()
        };

        let order = Order::try_from(row).expect("lossy optional values should not fail");

        assert_eq!(order.quantity, 0);
        assert_eq!(order.filled_quantity, 0);
        assert_eq!(order.trailing_percent, None);
        assert_eq!(order.trailing_price, None);
        assert_eq!(order.average_filled_price, None);
    }

    #[test]
    fn order_sqlite_conversion_reports_corrupt_required_fields() {
        assert_conversion_error(
            OrderSQLite {
                id: "not-a-uuid".to_string(),
                ..base_sqlite_order()
            },
            "id",
        );
        assert_conversion_error(
            OrderSQLite {
                unit_price: "not-a-price".to_string(),
                ..base_sqlite_order()
            },
            "unit_price",
        );
        assert_conversion_error(
            OrderSQLite {
                currency: "XYZ".to_string(),
                ..base_sqlite_order()
            },
            "currency",
        );
        assert_conversion_error(
            OrderSQLite {
                action: "hold".to_string(),
                ..base_sqlite_order()
            },
            "action",
        );
        assert_conversion_error(
            OrderSQLite {
                category: "trailing".to_string(),
                ..base_sqlite_order()
            },
            "category",
        );
        assert_conversion_error(
            OrderSQLite {
                status: "definitely_not_open".to_string(),
                ..base_sqlite_order()
            },
            "status",
        );
        assert_conversion_error(
            OrderSQLite {
                trading_vehicle_id: "not-a-uuid".to_string(),
                ..base_sqlite_order()
            },
            "trading_vehicle_id",
        );
        assert_conversion_error(
            OrderSQLite {
                time_in_force: "forever".to_string(),
                ..base_sqlite_order()
            },
            "time_in_force",
        );
    }
}
