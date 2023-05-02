use crate::schema::transactions;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use rust_decimal::Decimal;
use std::error::Error;
use tracing::error;
use trust_model::{Currency, Transaction, TransactionCategory};
use uuid::Uuid;

use super::worker_price::WorkerPrice;

pub struct WorkerTransaction;

impl WorkerTransaction {
    pub fn create_transaction(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        amount: Decimal,
        currency: Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>> {
        let now = Utc::now().naive_utc();
        let price = WorkerPrice::new(connection, currency, amount).unwrap();

        let new_transaction = NewTransaction {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: account_id.to_string(),
            price_id: price.id.to_string(),
            category: category.to_string(),
            trade_id: category.trade_id().map(|uuid| uuid.to_string()),
        };

        let transaction = diesel::insert_into(transactions::table)
            .values(&new_transaction)
            .get_result::<TransactionSQLite>(connection)
            .map(|tx| tx.domain_model(connection))
            .map_err(|error| {
                error!("Error creating transaction: {:?}", error);
                error
            })?;
        Ok(transaction)
    }
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = transactions)]
pub struct TransactionSQLite {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub category: String,
    pub price_id: String,
    pub account_id: String,
    pub trade_id: Option<String>,
}

impl TransactionSQLite {
    fn domain_model(&self, connection: &mut SqliteConnection) -> Transaction {
        let price = WorkerPrice::read(connection, Uuid::parse_str(&self.price_id).unwrap())
            .expect("Transaction without price");

        let category = TransactionCategory::parse(
            &self.category,
            self.trade_id
                .clone()
                .map(|uuid| Uuid::parse_str(&uuid).unwrap()),
        )
        .unwrap();

        Transaction {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            category: category,
            price,
            account_id: Uuid::parse_str(&self.account_id).unwrap(),
        }
    }
}
#[derive(Insertable)]
#[diesel(table_name = transactions)]
#[diesel(treat_none_as_null = true)]
pub struct NewTransaction {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub category: String,
    pub price_id: String,
    pub account_id: String,
    pub trade_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::workers::worker_account::WorkerAccount;
    use rust_decimal_macros::dec;

    use super::*;
    use diesel_migrations::*;

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    // Declare a test database connection
    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        // This will run the necessary migrations.
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    #[test]
    fn test_create_transaction() {
        let mut conn: SqliteConnection = establish_connection();

        // Create a new account record
        let account =
            WorkerAccount::create_account(&mut conn, "Test Account", "This is a test account")
                .expect("Error creating account");
        let tx = WorkerTransaction::create_transaction(
            &mut conn,
            account.id,
            dec!(10.99),
            Currency::BTC,
            TransactionCategory::Deposit,
        )
        .expect("Error creating transaction");

        assert_eq!(tx.account_id, account.id);
        assert_eq!(tx.price.amount, dec!(10.99));
        assert_eq!(tx.price.currency, Currency::BTC);
        assert_eq!(tx.category, TransactionCategory::Deposit);
        assert_eq!(tx.deleted_at, None);
    }

    fn test_create_transaction_with_trade_id() {
        let mut conn: SqliteConnection = establish_connection();

        let trade_id = Uuid::new_v4();
        // Create a new account record
        let account =
            WorkerAccount::create_account(&mut conn, "Test Account", "This is a test account")
                .expect("Error creating account");
        let tx = WorkerTransaction::create_transaction(
            &mut conn,
            account.id,
            dec!(10.99),
            Currency::BTC,
            TransactionCategory::Output(trade_id),
        )
        .expect("Error creating transaction");

        assert_eq!(tx.account_id, account.id);
        assert_eq!(tx.price.amount, dec!(10.99));
        assert_eq!(tx.price.currency, Currency::BTC);
        assert_eq!(tx.category, TransactionCategory::Output(trade_id));
        assert_eq!(tx.deleted_at, None);
    }
}
