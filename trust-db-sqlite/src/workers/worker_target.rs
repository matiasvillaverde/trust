use super::{WorkerOrder, WorkerPrice};
use crate::schema::targets;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use rust_decimal::Decimal;
use std::error::Error;
use tracing::error;
use trust_model::{Currency, Order, Target};
use uuid::Uuid;

pub struct WorkerTarget;
impl WorkerTarget {
    pub fn create(
        connection: &mut SqliteConnection,
        amount: Decimal,
        currency: &Currency,
        order: &Order,
    ) -> Result<Target, Box<dyn Error>> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let price = WorkerPrice::create(connection, currency, amount)?;

        let new_target = NewTarget {
            id,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            target_price_id: price.id.to_string(),
            order_id: order.id.to_string(),
        };

        let target = diesel::insert_into(targets::table)
            .values(&new_target)
            .get_result::<TargetSQLite>(connection)
            .map(|target| target.domain_model(connection))
            .map_err(|error| {
                error!("Error creating target: {:?}", error);
                error
            })?;
        Ok(target)
    }

    pub fn read(
        connection: &mut SqliteConnection,
        id: Uuid,
    ) -> Result<Target, diesel::result::Error> {
        targets::table
            .filter(targets::id.eq(&id.to_string()))
            .first(connection)
            .map(|target: TargetSQLite| target.domain_model(connection))
    }
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = targets)]
#[diesel(treat_none_as_null = true)]
struct TargetSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    target_price_id: String,
    order_id: String,
}

impl TargetSQLite {
    fn domain_model(self, connection: &mut SqliteConnection) -> Target {
        let price = WorkerPrice::read(connection, Uuid::parse_str(&self.target_price_id).unwrap())
            .expect("Error reading price for target");
        let order = WorkerOrder::read(connection, Uuid::parse_str(&self.order_id).unwrap())
            .expect("Error reading order for target");
        Target {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            target_price: price,
            order: order,
            trade_id: Uuid::new_v4(), // TODO: read trade_id later
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = targets)]
#[diesel(treat_none_as_null = true)]
struct NewTarget {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    target_price_id: String,
    order_id: String,
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::workers::WorkerTradingVehicle;
    use diesel_migrations::*;
    use rust_decimal_macros::dec;
    use trust_model::{OrderAction, OrderCategory, TradingVehicleCategory};

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    // Declare a test database connection in memory.
    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        // This will run the necessary migrations.
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    fn create_target(conn: &mut SqliteConnection) -> (Target, Order) {
        let tv = WorkerTradingVehicle::create(
            conn,
            "AAPL",
            "US0378331005",
            &TradingVehicleCategory::Stock,
            "Alpaca",
        )
        .unwrap();

        let order = WorkerOrder::create(
            conn,
            dec!(9),
            &Currency::USD,
            99,
            &OrderAction::Sell,
            &OrderCategory::Limit,
            &tv,
        )
        .unwrap();

        let target = WorkerTarget::create(conn, dec!(10), &Currency::USD, &order).unwrap();

        return (target, order);
    }

    #[test]
    fn test_create_target() {
        let mut conn = establish_connection();

        let (target, order) = create_target(&mut conn);

        assert_eq!(target.order, order);
        assert_eq!(target.target_price.amount, dec!(10));
        assert_eq!(target.target_price.currency, Currency::USD);
        assert_eq!(target.created_at, target.updated_at);
        assert_eq!(target.deleted_at, None);
    }

    #[test]
    fn test_read_target() {
        let mut conn = establish_connection();

        let (target, order) = create_target(&mut conn);

        let read_target = WorkerTarget::read(&mut conn, target.id).unwrap();

        assert_eq!(read_target.order, order);
        assert_eq!(read_target.target_price.amount, dec!(10));
        assert_eq!(read_target.target_price.currency, Currency::USD);
        assert_eq!(read_target.created_at, read_target.updated_at);
        assert_eq!(read_target.deleted_at, None);
    }
}
