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
            broker_order_id: value
                .broker_order_id
                .map(|x| Uuid::parse_str(&x))
                .transpose()?,
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
            broker_order_id: value.broker_order_id.map(|x| x.to_string()),
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
            .on_conflict((executions::broker, executions::account_id, executions::broker_execution_id))
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
    use model::{Account, DatabaseFactory, Environment, Execution, ExecutionSide, ExecutionSource};
    use rust_decimal_macros::dec;

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
        }
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
        assert_eq!(first.id, second.id, "should return existing row on conflict");
    }
}
