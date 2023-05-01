use crate::schema::prices;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use tracing::error;
use trust_model::{Currency, Price};
use uuid::Uuid;

pub struct WorkerPrice;

impl WorkerPrice {
    pub fn new(
        connection: &mut SqliteConnection,
        currency: Currency,
        amount: Decimal,
    ) -> Result<Price, Box<dyn Error>> {
        let now = Utc::now().naive_utc();

        let new_price = NewPrice {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            currency: currency.to_string(),
            amount: amount.to_string(),
        };

        let price = diesel::insert_into(prices::table)
            .values(&new_price)
            .get_result::<PriceSQLite>(connection)
            .map(|price| price.domain_model())
            .map_err(|error| {
                error!("Error creating price: {:?}", error);
                error
            })?;
        Ok(price)
    }
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = prices)]
#[diesel(treat_none_as_null = true)]
struct PriceSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    currency: String,
    amount: String, // This is a Decimal type
}

impl PriceSQLite {
    fn domain_model(self) -> Price {
        Price {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            currency: Currency::from_str(&self.currency).unwrap(),
            amount: Decimal::from_str(&self.amount).unwrap(),
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = prices)]
#[diesel(treat_none_as_null = true)]
struct NewPrice {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    currency: String,
    amount: String, // Decimal type
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel_migrations::*;
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
    fn test_create_price() {
        let mut conn = establish_connection();

        // Create a new price record
        let price = WorkerPrice::new(&mut conn, Currency::USD, dec!(10.99)).unwrap();

        assert_eq!(price.currency, Currency::USD);
        assert_eq!(price.amount, dec!(10.99));
        assert_eq!(price.updated_at, price.created_at); // created_at and updated_at should be the same
        assert_eq!(price.created_at, price.updated_at); // created_at and updated_at should be the same
        assert_eq!(price.deleted_at, None);
    }

    // #[test]
    // fn test_read_price() {
    //     let mut conn = establish_connection();
    //     let now = Utc::now().naive_utc();
    //     let uuid = Uuid::new_v4();

    //     // Create a new price record
    //     let new_price = NewPrice {
    //         uuid: uuid.to_string(),
    //         currency: Currency::USD,
    //         digit: 10,
    //         decimal: 99,
    //         created_at: now,
    //         updated_at: now,
    //         deleted_at: None,
    //     };
    //     let price = diesel::insert_into(prices::table)
    //         .values(&new_price)
    //         .get_result::<PriceSQLite>(&mut conn)
    //         .unwrap();

    //     // Read the price record by id
    //     let read_price = Price::read(&mut conn, uuid).expect("Error reading price");

    //     assert_eq!(read_price.uuid, uuid);
    //     assert_eq!(read_price.currency, Currency::USD);
    //     assert_eq!(read_price.digit, 10);
    //     assert_eq!(read_price.decimal, 99);
    //     assert_eq!(read_price.created_at, now);
    //     assert_eq!(read_price.updated_at, now);
    //     assert_eq!(price.deleted_at, None);
    // }

    // #[test]
    // fn test_delete_price() {
    //     let mut conn = establish_connection();

    //     // Create a new price record
    //     let price = Price::new(&mut conn, Currency::USD, 10, 99);
    //     let expected_price = price.clone();

    //     // Delete the price record
    //     let deleted_price = price.delete(&mut conn);

    //     assert_eq!(deleted_price.uuid, expected_price.uuid);
    //     assert_eq!(deleted_price.currency, expected_price.currency);
    //     assert_eq!(deleted_price.digit, expected_price.digit);
    //     assert_eq!(deleted_price.decimal, expected_price.decimal);
    //     assert_eq!(deleted_price.created_at, expected_price.created_at);
    //     assert_ne!(deleted_price.updated_at, expected_price.updated_at); // updated_at should be different because it was updated
    //     assert_ne!(deleted_price.deleted_at, None); // deleted_at should be different because it was deleted
    // }
}
