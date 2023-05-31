use super::{WorkerOrder, WorkerPrice};
use crate::schema::targets;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use rust_decimal::Decimal;
use std::error::Error;
use tracing::error;
use trust_model::{Currency, Order, Target, Trade};
use uuid::Uuid;

pub struct WorkerTarget;
impl WorkerTarget {
    pub fn create(
        connection: &mut SqliteConnection,
        amount: Decimal,
        currency: &Currency,
        order: &Order,
        trade: &Trade,
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
            trade_id: trade.id.to_string(),
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

    pub fn read_all(
        trade_id: Uuid,
        connection: &mut SqliteConnection,
    ) -> Result<Vec<Target>, Box<dyn Error>> {
        let targets = targets::table
            .filter(targets::deleted_at.is_null())
            .filter(targets::trade_id.eq(trade_id.to_string()))
            .load::<TargetSQLite>(connection)
            .map(|targets: Vec<TargetSQLite>| {
                targets
                    .into_iter()
                    .map(|target| target.domain_model(connection))
                    .collect()
            })
            .map_err(|error| {
                error!("Error creating price: {:?}", error);
                error
            })?;
        Ok(targets)
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
    trade_id: String,
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
            order,
            trade_id: Uuid::parse_str(&self.trade_id).unwrap(),
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
    trade_id: String,
}
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::workers::{WorkerOrder, WorkerTrade, WorkerTradingVehicle};
//     use diesel_migrations::*;
//     use rust_decimal_macros::dec;
//     use trust_model::{
//         Account, OrderAction, OrderCategory, TradingVehicle, TradingVehicleCategory, DatabaseFactory
//     };
//     use std::sync::{Arc, Mutex};
//     use crate::SqliteDatabase;

//     pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

//     // Declare a test database connection in memory.
//     fn establish_connection() -> SqliteConnection {
//         let mut connection = SqliteConnection::establish(":memory:").unwrap();
//         // This will run the necessary migrations.
//         connection.run_pending_migrations(MIGRATIONS).unwrap();
//         connection.begin_test_transaction().unwrap();
//         connection
//     }

//     fn create_factory(connection: SqliteConnection) -> Box<dyn DatabaseFactory> {
//         Box::new(SqliteDatabase::new_from(Arc::new(Mutex::new(connection))))
//     }

//     fn create_account(db: Box<dyn DatabaseFactory>) -> Account {
//         db.write_account_db().create_account("Test Account 3", "This is a test account")
//                 .expect("Error creating account")
//     }

//     fn create_trading_vehicle(conn: &mut SqliteConnection) -> TradingVehicle {
//         WorkerTradingVehicle::create(
//             conn,
//             "AAPL",
//             "US0378331005",
//             &TradingVehicleCategory::Stock,
//             "Alpaca",
//         )
//         .unwrap()
//     }

//     fn create_order(conn: &mut SqliteConnection, tv: &TradingVehicle) -> Order {
//         WorkerOrder::create(
//             conn,
//             dec!(9),
//             &Currency::USD,
//             99,
//             &OrderAction::Sell,
//             &OrderCategory::Limit,
//             tv,
//         )
//         .unwrap()
//     }

//     fn create_trade(
//         conn: &mut SqliteConnection,
//         order: &Order,
//         tv: &TradingVehicle,
//         account: &Account,
//     ) -> Trade {
//         WorkerTrade::create(
//             conn,
//             &trust_model::TradeCategory::Long,
//             &Currency::USD,
//             tv,
//             order,
//             order,
//             account,
//         )
//         .unwrap()
//     }

//     fn create_target(conn: &mut SqliteConnection, order: &Order, trade: &Trade) -> Target {
//         WorkerTarget::create(conn, dec!(10), &Currency::USD, order, trade).unwrap()
//     }

//     #[test]
//     fn test_create_target() {
//         let mut conn = establish_connection();

//         let tv = create_trading_vehicle(&mut conn);
//         let order = create_order(&mut conn, &tv);
//         let account = create_account(create_factory(conn));
//         let trade = create_trade(&mut conn, &order, &tv, &account);
//         let target = create_target(&mut conn, &order, &trade);

//         assert_eq!(target.order, order);
//         assert_eq!(target.target_price.amount, dec!(10));
//         assert_eq!(target.target_price.currency, Currency::USD);
//         assert_eq!(target.created_at, target.updated_at);
//         assert_eq!(target.deleted_at, None);
//     }

//     #[test]
//     fn test_read_all_targets() {
//         let mut conn = establish_connection();

//         let tv = create_trading_vehicle(&mut conn);
//         let order = create_order(&mut conn, &tv);
//         let account = create_account(create_factory(conn));
//         let trade = create_trade(&mut conn, &order, &tv, &account);
//         let target = create_target(&mut conn, &order, &trade);
//         let order2 = create_order(&mut conn, &tv);
//         let target2 = create_target(&mut conn, &order2, &trade);

//         let targets = WorkerTarget::read_all(trade.id, &mut conn).unwrap();

//         assert_eq!(targets.len(), 2);
//         assert_eq!(targets[0], target);
//         assert_eq!(targets[1], target2);
//     }
// }
