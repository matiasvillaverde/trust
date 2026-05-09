use crate::schema::executions;
use diesel::prelude::*;
use model::{Execution, ExecutionSide, ExecutionSource};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use uuid::Uuid;

pub struct WorkerExecution;

#[derive(Queryable, Insertable, AsChangeset, Debug, Clone)]
#[diesel(table_name = executions)]
struct ExecutionSQLite {
    id: String,
    created_at: chrono::NaiveDateTime,
    updated_at: chrono::NaiveDateTime,
    deleted_at: Option<chrono::NaiveDateTime>,

    broker: String,
    source: String,
    account_id: String,
    trade_id: Option<String>,
    order_id: Option<String>,
    broker_execution_id: String,
    broker_order_id: Option<String>,
    symbol: String,
    side: String,
    qty: String,
    price: String,
    executed_at: chrono::NaiveDateTime,
    raw_json: Option<String>,
}

impl TryFrom<ExecutionSQLite> for Execution {
    type Error = Box<dyn Error>;

    fn try_from(value: ExecutionSQLite) -> Result<Self, Self::Error> {
        Ok(Execution {
            id: Uuid::parse_str(&value.id)?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            broker: value.broker,
            source: ExecutionSource::from_str(&value.source)
                .map_err(|_| "invalid execution.source in database")?,
            account_id: Uuid::parse_str(&value.account_id)?,
            trade_id: value.trade_id.map(|x| Uuid::parse_str(&x)).transpose()?,
            order_id: value.order_id.map(|x| Uuid::parse_str(&x)).transpose()?,
            broker_execution_id: value.broker_execution_id,
            broker_order_id: value.broker_order_id,
            symbol: value.symbol,
            side: ExecutionSide::from_str(&value.side)
                .map_err(|_| "invalid execution.side in database")?,
            qty: Decimal::from_str(&value.qty)
                .map_err(|e| format!("invalid execution.qty in database: {e}"))?,
            price: Decimal::from_str(&value.price)
                .map_err(|e| format!("invalid execution.price in database: {e}"))?,
            executed_at: value.executed_at,
            raw_json: value.raw_json,
        })
    }
}

impl From<&Execution> for ExecutionSQLite {
    fn from(value: &Execution) -> Self {
        ExecutionSQLite {
            id: value.id.to_string(),
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            broker: value.broker.clone(),
            source: value.source.to_string(),
            account_id: value.account_id.to_string(),
            trade_id: value.trade_id.map(|x| x.to_string()),
            order_id: value.order_id.map(|x| x.to_string()),
            broker_execution_id: value.broker_execution_id.clone(),
            broker_order_id: value.broker_order_id.clone(),
            symbol: value.symbol.clone(),
            side: value.side.to_string(),
            qty: value.qty.to_string(),
            price: value.price.to_string(),
            executed_at: value.executed_at,
            raw_json: value.raw_json.clone(),
        }
    }
}

impl WorkerExecution {
    pub fn upsert(
        connection: &mut SqliteConnection,
        execution: &Execution,
    ) -> Result<Execution, Box<dyn Error>> {
        // First try an insert; if it conflicts (duplicate), read and return the existing row.
        let row = ExecutionSQLite::from(execution);

        let insert_result = diesel::insert_into(executions::table)
            .values(&row)
            .on_conflict((
                executions::broker,
                executions::account_id,
                executions::broker_execution_id,
            ))
            .do_nothing()
            .execute(connection)?;

        if insert_result == 1 {
            // Inserted: return by id.
            return executions::table
                .filter(executions::id.eq(&row.id))
                .first::<ExecutionSQLite>(connection)
                .map_err(|e| Box::new(e) as Box<dyn Error>)?
                .try_into();
        }

        // Conflict: find by unique key.
        executions::table
            .filter(executions::deleted_at.is_null())
            .filter(executions::broker.eq(&row.broker))
            .filter(executions::account_id.eq(&row.account_id))
            .filter(executions::broker_execution_id.eq(&row.broker_execution_id))
            .first::<ExecutionSQLite>(connection)
            .map_err(|e| Box::new(e) as Box<dyn Error>)?
            .try_into()
    }

    pub fn read_for_trade(
        connection: &mut SqliteConnection,
        trade_id: Uuid,
    ) -> Result<Vec<Execution>, Box<dyn Error>> {
        let rows = executions::table
            .filter(executions::deleted_at.is_null())
            .filter(executions::trade_id.eq(trade_id.to_string()))
            .order(executions::executed_at.asc())
            .load::<ExecutionSQLite>(connection)?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub fn read_for_order(
        connection: &mut SqliteConnection,
        order_id: Uuid,
    ) -> Result<Vec<Execution>, Box<dyn Error>> {
        let rows = executions::table
            .filter(executions::deleted_at.is_null())
            .filter(executions::order_id.eq(order_id.to_string()))
            .order(executions::executed_at.asc())
            .load::<ExecutionSQLite>(connection)?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub fn latest_for_trade(
        connection: &mut SqliteConnection,
        trade_id: Uuid,
    ) -> Result<Option<chrono::NaiveDateTime>, Box<dyn Error>> {
        use diesel::dsl::max;
        let latest: Option<chrono::NaiveDateTime> = executions::table
            .filter(executions::deleted_at.is_null())
            .filter(executions::trade_id.eq(trade_id.to_string()))
            .select(max(executions::executed_at))
            .first(connection)?;
        Ok(latest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteDatabase;
    use chrono::{Duration, Utc};
    use diesel_migrations::*;
    use model::{
        Account, AccountType, Currency, DatabaseFactory, DraftTrade, Environment, Execution,
        ExecutionSide, ExecutionSource, Order, OrderAction, OrderCategory, Trade, TradeCategory,
        TradingVehicle, TradingVehicleCategory,
    };
    use rust_decimal_macros::dec;
    use std::sync::{Arc, Mutex};

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    fn create_database() -> (SqliteDatabase, Arc<Mutex<SqliteConnection>>) {
        let connection = Arc::new(Mutex::new(establish_connection()));
        (SqliteDatabase::new_from(connection.clone()), connection)
    }

    fn sample_account() -> Account {
        let now = chrono::Utc::now().naive_utc();
        Account {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            name: "test".to_string(),
            description: "test".to_string(),
            environment: Environment::Paper,
            taxes_percentage: dec!(0),
            earnings_percentage: dec!(0),
            account_type: AccountType::Primary,
            parent_account_id: None,
            broker_kind: model::BrokerKind::Alpaca,
            broker_account_id: None,
        }
    }

    fn create_account(database: &SqliteDatabase, name: &str) -> Account {
        database
            .account_write()
            .create(name, name, Environment::Paper, dec!(0), dec!(0))
            .expect("account should be created")
    }

    fn create_vehicle(database: &SqliteDatabase, symbol: &str) -> TradingVehicle {
        database
            .trading_vehicle_write()
            .create_trading_vehicle(
                symbol,
                Some(symbol),
                &TradingVehicleCategory::Stock,
                "alpaca",
            )
            .expect("trading vehicle should be created")
    }

    fn create_order(
        database: &SqliteDatabase,
        vehicle: &TradingVehicle,
        action: OrderAction,
        category: OrderCategory,
        price: Decimal,
    ) -> Order {
        database
            .order_write()
            .create(vehicle, 10, price, &Currency::USD, &action, &category)
            .expect("order should be created")
    }

    fn create_trade_graph(database: &SqliteDatabase, account: &Account) -> Trade {
        let vehicle = create_vehicle(database, "AAPL");
        let stop = create_order(
            database,
            &vehicle,
            OrderAction::Sell,
            OrderCategory::Stop,
            dec!(90),
        );
        let entry = create_order(
            database,
            &vehicle,
            OrderAction::Buy,
            OrderCategory::Limit,
            dec!(100),
        );
        let target = create_order(
            database,
            &vehicle,
            OrderAction::Sell,
            OrderCategory::Limit,
            dec!(120),
        );
        let draft = DraftTrade {
            account: account.clone(),
            trading_vehicle: vehicle,
            quantity: 10,
            currency: Currency::USD,
            category: TradeCategory::Long,
            thesis: Some("execution ledger test".to_string()),
            sector: Some("technology".to_string()),
            asset_class: Some("equity".to_string()),
            context: Some("unit test".to_string()),
        };

        database
            .trade_write()
            .create_trade(draft, &stop, &entry, &target)
            .expect("trade should be created")
    }

    fn execution_for(
        account_id: Uuid,
        trade_id: Uuid,
        order_id: Uuid,
        broker_execution_id: &str,
        executed_at: chrono::NaiveDateTime,
    ) -> Execution {
        let mut execution = Execution::new(
            "alpaca".to_string(),
            ExecutionSource::TradeUpdates,
            account_id,
            broker_execution_id.to_string(),
            Some(format!("order-{broker_execution_id}")),
            "AAPL".to_string(),
            ExecutionSide::Buy,
            dec!(1),
            dec!(100),
            executed_at,
        );
        execution.trade_id = Some(trade_id);
        execution.order_id = Some(order_id);
        execution
    }

    fn standalone_execution(account_id: Uuid, broker_execution_id: &str) -> Execution {
        Execution::new(
            "alpaca".to_string(),
            ExecutionSource::TradeUpdates,
            account_id,
            broker_execution_id.to_string(),
            Some(format!("order-{broker_execution_id}")),
            "AAPL".to_string(),
            ExecutionSide::Buy,
            dec!(1),
            dec!(100),
            Utc::now().naive_utc(),
        )
    }

    fn sample_execution_sqlite() -> ExecutionSQLite {
        let execution = Execution::new(
            "alpaca".to_string(),
            ExecutionSource::TradeUpdates,
            Uuid::new_v4(),
            "exec-1".to_string(),
            Some("order-1".to_string()),
            "AAPL".to_string(),
            ExecutionSide::Buy,
            dec!(1),
            dec!(100),
            Utc::now().naive_utc(),
        );
        ExecutionSQLite::from(&execution)
    }

    fn assert_execution_conversion_error(row: ExecutionSQLite, expected: &str) {
        let error = Execution::try_from(row).expect_err("corrupt execution row should fail");
        assert!(
            error.to_string().contains(expected),
            "expected {expected} in error: {error}"
        );
    }

    fn assert_execution_ids(executions: &[Execution], expected: &[Uuid]) {
        assert_eq!(
            executions
                .iter()
                .map(|execution| execution.id)
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn execution_sqlite_conversion_reports_corrupt_fields() {
        let mut invalid_id = sample_execution_sqlite();
        invalid_id.id = "not-a-uuid".to_string();
        assert_execution_conversion_error(invalid_id, "invalid");

        let mut invalid_source = sample_execution_sqlite();
        invalid_source.source = "stream".to_string();
        assert_execution_conversion_error(invalid_source, "invalid execution.source");

        let mut invalid_account = sample_execution_sqlite();
        invalid_account.account_id = "not-a-uuid".to_string();
        assert_execution_conversion_error(invalid_account, "invalid");

        let mut invalid_trade = sample_execution_sqlite();
        invalid_trade.trade_id = Some("not-a-uuid".to_string());
        assert_execution_conversion_error(invalid_trade, "invalid");

        let mut invalid_order = sample_execution_sqlite();
        invalid_order.order_id = Some("not-a-uuid".to_string());
        assert_execution_conversion_error(invalid_order, "invalid");

        let mut invalid_side = sample_execution_sqlite();
        invalid_side.side = "cover".to_string();
        assert_execution_conversion_error(invalid_side, "invalid execution.side");

        let mut invalid_qty = sample_execution_sqlite();
        invalid_qty.qty = "not-a-decimal".to_string();
        assert_execution_conversion_error(invalid_qty, "invalid execution.qty");

        let mut invalid_price = sample_execution_sqlite();
        invalid_price.price = "not-a-decimal".to_string();
        assert_execution_conversion_error(invalid_price, "invalid execution.price");
    }

    #[test]
    fn read_queries_sort_by_execution_time_and_ignore_soft_deleted_rows() {
        let (db, shared_connection) = create_database();
        let account = create_account(&db, "Execution Read Account");
        let trade = create_trade_graph(&db, &account);
        let earlier = Utc::now().naive_utc();
        let later = earlier + Duration::seconds(30);

        let first = db
            .execution_write()
            .upsert_execution(&execution_for(
                account.id,
                trade.id,
                trade.entry.id,
                "exec-1",
                earlier,
            ))
            .expect("first execution should write");
        let second = db
            .execution_write()
            .upsert_execution(&execution_for(
                account.id,
                trade.id,
                trade.entry.id,
                "exec-2",
                later,
            ))
            .expect("second execution should write");

        let trade_executions = db
            .execution_read()
            .all_trade_executions(trade.id)
            .expect("trade executions should read");
        assert_execution_ids(&trade_executions, &[first.id, second.id]);

        let order_executions = db
            .execution_read()
            .all_order_executions(trade.entry.id)
            .expect("order executions should read");
        assert_execution_ids(&order_executions, &[first.id, second.id]);
        assert_eq!(
            db.execution_read()
                .latest_trade_execution_at(trade.id)
                .expect("latest execution should read"),
            Some(later)
        );

        {
            let mut connection = shared_connection
                .lock()
                .expect("connection lock should be available");
            diesel::update(executions::table.filter(executions::id.eq(second.id.to_string())))
                .set(executions::deleted_at.eq(Some(Utc::now().naive_utc())))
                .execute(&mut *connection)
                .expect("execution should be soft deleted");
        }

        let trade_executions = db
            .execution_read()
            .all_trade_executions(trade.id)
            .expect("trade executions should read after soft delete");
        assert_execution_ids(&trade_executions, &[first.id]);
        assert_eq!(
            db.execution_read()
                .latest_trade_execution_at(trade.id)
                .expect("latest execution should read after soft delete"),
            Some(earlier)
        );
    }

    #[test]
    fn upsert_is_idempotent_on_broker_account_execution_id() {
        let db = SqliteDatabase::new_in_memory();

        // Insert account row (executions references accounts).
        let account = sample_account();
        db.account_write()
            .create(
                &account.name,
                &account.description,
                account.environment,
                account.taxes_percentage,
                account.earnings_percentage,
            )
            .unwrap();
        let stored_account = db.account_read().for_name(&account.name).unwrap();

        let mut exec = Execution::new(
            "alpaca".to_string(),
            ExecutionSource::TradeUpdates,
            stored_account.id,
            "exec-1".to_string(),
            None,
            "AAPL".to_string(),
            ExecutionSide::Buy,
            dec!(1),
            dec!(100),
            chrono::Utc::now().naive_utc(),
        );

        let first = db.execution_write().upsert_execution(&exec).unwrap();
        exec.id = Uuid::new_v4(); // Attempt to insert "same" execution with different local id.
        let second = db.execution_write().upsert_execution(&exec).unwrap();

        assert_eq!(first.broker_execution_id, second.broker_execution_id);
        assert_eq!(
            first.id, second.id,
            "should return existing row on conflict"
        );
    }

    #[test]
    fn upsert_reports_when_inserted_row_cannot_be_reloaded() {
        let (db, shared_connection) = create_database();
        let account = create_account(&db, "Execution Reload Account");
        let mut connection = shared_connection
            .lock()
            .expect("connection lock should be available");
        diesel::sql_query(
            "CREATE TRIGGER delete_execution_after_insert \
             AFTER INSERT ON executions \
             BEGIN \
                 DELETE FROM executions WHERE id = NEW.id; \
             END",
        )
        .execute(&mut *connection)
        .expect("cleanup trigger should be created");

        let error = WorkerExecution::upsert(
            &mut connection,
            &standalone_execution(account.id, "deleted-after-insert"),
        )
        .expect_err("deleted inserted row should fail reload");

        assert!(error.to_string().contains("Record not found"));
    }

    #[test]
    fn upsert_reports_when_conflict_lookup_finds_no_row() {
        let (db, shared_connection) = create_database();
        let account = create_account(&db, "Execution Ignored Account");
        let mut connection = shared_connection
            .lock()
            .expect("connection lock should be available");
        diesel::sql_query(
            "CREATE TRIGGER ignore_execution_insert \
             BEFORE INSERT ON executions \
             BEGIN \
                 SELECT RAISE(IGNORE); \
             END",
        )
        .execute(&mut *connection)
        .expect("ignore trigger should be created");

        let error = WorkerExecution::upsert(
            &mut connection,
            &standalone_execution(account.id, "ignored-insert"),
        )
        .expect_err("ignored insert should fail conflict lookup");

        assert!(error.to_string().contains("Record not found"));
    }

    #[test]
    fn execution_worker_reports_missing_table_errors() {
        let mut connection = establish_connection();
        diesel::sql_query("DROP TABLE executions")
            .execute(&mut connection)
            .expect("executions table should drop");
        let trade_id = Uuid::new_v4();
        let order_id = Uuid::new_v4();
        let execution = execution_for(
            Uuid::new_v4(),
            trade_id,
            order_id,
            "missing-table-exec",
            Utc::now().naive_utc(),
        );

        let upsert_error = WorkerExecution::upsert(&mut connection, &execution)
            .expect_err("missing table should fail execution upsert");
        assert!(upsert_error.to_string().contains("executions"));

        let trade_error = WorkerExecution::read_for_trade(&mut connection, trade_id)
            .expect_err("missing table should fail trade execution read");
        assert!(trade_error.to_string().contains("executions"));

        let order_error = WorkerExecution::read_for_order(&mut connection, order_id)
            .expect_err("missing table should fail order execution read");
        assert!(order_error.to_string().contains("executions"));

        let latest_error = WorkerExecution::latest_for_trade(&mut connection, trade_id)
            .expect_err("missing table should fail latest execution read");
        assert!(latest_error.to_string().contains("executions"));
    }
}
