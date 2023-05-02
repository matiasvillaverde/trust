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
        trade_id: Option<Uuid>,
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
            trade_id: trade_id.map(|uuid| uuid.to_string()),
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
