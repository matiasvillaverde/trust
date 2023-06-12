use crate::schema::accounts_overviews;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::SqliteConnection;
use rust_decimal::Decimal;

use std::error::Error;
use tracing::error;
use trust_model::{Account, AccountOverview, Currency};
use uuid::Uuid;

pub struct WorkerAccountOverview;

impl WorkerAccountOverview {
    pub fn create(
        connection: &mut SqliteConnection,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let new_account_overview = NewAccountOverview {
            account_id: account.id.to_string(),
            currency: currency.to_string(),
            ..Default::default()
        };

        let overview = diesel::insert_into(accounts_overviews::table)
            .values(&new_account_overview)
            .get_result::<AccountOverviewSQLite>(connection)
            .map(|overview| overview.domain_model())
            .map_err(|error| {
                error!("Error creating overview: {:?}", error);
                error
            })?;
        Ok(overview)
    }

    pub fn read(
        connection: &mut SqliteConnection,
        account_id: Uuid,
    ) -> Result<Vec<AccountOverview>, Box<dyn Error>> {
        let overviews = accounts_overviews::table
            .filter(accounts_overviews::account_id.eq(account_id.to_string()))
            .filter(accounts_overviews::deleted_at.is_null())
            .load::<AccountOverviewSQLite>(connection)
            .map(|overviews| {
                overviews
                    .into_iter()
                    .map(|overview| overview.domain_model())
                    .collect()
            })
            .map_err(|error| {
                error!("Error reading overviews: {:?}", error);
                error
            })?;
        Ok(overviews)
    }

    pub fn read_for_currency(
        connection: &mut SqliteConnection,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let overviews = accounts_overviews::table
            .filter(accounts_overviews::account_id.eq(account_id.to_string()))
            .filter(accounts_overviews::currency.eq(currency.to_string()))
            .filter(accounts_overviews::deleted_at.is_null())
            .first::<AccountOverviewSQLite>(connection)
            .map(|overview| overview.domain_model())
            .map_err(|error| {
                error!("Error creating overview: {:?}", error);
                error
            })?;
        Ok(overviews)
    }

    pub fn update(
        connection: &mut SqliteConnection,
        overview: AccountOverview,
        total_balance: Decimal,
        total_available: Decimal,
        total_in_trade: Decimal,
        total_taxed: Decimal,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let overview = diesel::update(accounts_overviews::table)
            .filter(accounts_overviews::id.eq(&overview.id.to_string()))
            .set((
                accounts_overviews::updated_at.eq(Utc::now().naive_utc()),
                accounts_overviews::total_balance_id.eq(total_balance.to_string()),
                accounts_overviews::total_available_id.eq(total_available.to_string()),
                accounts_overviews::total_in_trade_id.eq(total_in_trade.to_string()),
                accounts_overviews::taxed_id.eq(total_taxed.to_string()),
            ))
            .get_result::<AccountOverviewSQLite>(connection)
            .map(|o| o.domain_model())
            .map_err(|error| {
                error!("Error updating overview: {:?}", error);
                error
            })?;
        Ok(overview)
    }
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = accounts_overviews)]
#[diesel(treat_none_as_null = true)]
struct AccountOverviewSQLite {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    account_id: String,
    total_balance_id: String,
    total_in_trade_id: String,
    total_available_id: String,
    taxed_id: String,
    currency: String,
}

impl AccountOverviewSQLite {
    fn domain_model(self) -> AccountOverview {
        use std::str::FromStr;
        AccountOverview {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            account_id: Uuid::parse_str(&self.account_id).unwrap(),
            total_balance: Decimal::from_str(&self.total_balance_id).unwrap(),
            total_in_trade: Decimal::from_str(&self.total_in_trade_id).unwrap(),
            total_available: Decimal::from_str(&self.total_available_id).unwrap(),
            taxed: Decimal::from_str(&self.taxed_id).unwrap(),
            currency: Currency::from_str(&self.currency).unwrap(),
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = accounts_overviews)]
pub struct NewAccountOverview {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    account_id: String,
    total_balance_id: String,
    total_in_trade_id: String,
    total_available_id: String,
    taxed_id: String,
    currency: String,
}

impl Default for NewAccountOverview {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        NewAccountOverview {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: "".to_string(),
            total_balance_id: "".to_string(),
            total_in_trade_id: "".to_string(),
            total_available_id: "".to_string(),
            taxed_id: "".to_string(),
            currency: Currency::USD.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    // use std::sync::{Arc, Mutex};

    // use crate::SqliteDatabase;

    // use super::*;
    // use trust_model::DatabaseFactory;
    // use diesel_migrations::*;

    // pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    // fn establish_connection() -> SqliteConnection {
    //     let mut connection = SqliteConnection::establish(":memory:").unwrap();
    //     // This will run the necessary migrations.
    //     connection.run_pending_migrations(MIGRATIONS).unwrap();
    //     connection.begin_test_transaction().unwrap();
    //     connection
    // }

    // fn create_factory(connection: SqliteConnection) -> Box<dyn DatabaseFactory> {
    //     Box::new(SqliteDatabase::new_from(Arc::new(Mutex::new(connection))))
    // }

    // #[test]
    // fn test_create_overview() {
    //     let mut conn = establish_connection();

    //     let db = create_factory( conn);

    //     let account = db.write_account_db().create_account("Test Account", "Some description")
    //         .expect("Failed to create account");
    //     let overview = WorkerAccountOverview::create(&mut conn, &account, &Currency::BTC)
    //         .expect("Failed to create overview");

    //     assert_eq!(overview.account_id, account.id);
    //     assert_eq!(overview.currency, Currency::BTC);
    //     assert_eq!(overview.total_balance, dec!(0));
    //     assert_eq!(overview.total_in_trade, dec!(0));
    //     assert_eq!(overview.total_available, dec!(0));
    //     assert_eq!(overview.taxed, dec!(0));
    //     assert_eq!(overview.total_balance.currency, Currency::BTC);
    //     assert_eq!(overview.total_in_trade.currency, Currency::BTC);
    //     assert_eq!(overview.total_available.currency, Currency::BTC);
    //     assert_eq!(overview.taxed.currency, Currency::BTC);
    // }

    // #[test]
    // fn test_read_overviews() {
    //     let mut conn = establish_connection();
    //     let db = create_factory( conn);

    //     let account = db.write_account_db().create_account("Test Account", "Some description")
    //         .expect("Failed to create account");
    //     let overview_btc: AccountOverview =
    //         WorkerAccountOverview::create(&mut conn, &account, &Currency::BTC)
    //             .expect("Failed to create overview");
    //     let overview_usd: AccountOverview =
    //         WorkerAccountOverview::create(&mut conn, &account, &Currency::USD)
    //             .expect("Failed to create overview");

    //     let overviews =
    //         WorkerAccountOverview::read(&mut conn, account.id).expect("Failed to read overviews");

    //     assert_eq!(overviews.len(), 2);
    //     assert_eq!(overviews[0], overview_btc);
    //     assert_eq!(overviews[1], overview_usd);
    // }

    // #[test]
    // fn test_update() {
    //     let mut conn = establish_connection();
    //     let db = create_factory(conn);

    //     let account = db.write_account_db().create_account("Test Account", "Some description")
    //         .expect("Failed to create account");
    //     let overview_btc: AccountOverview =
    //         WorkerAccountOverview::create(&mut conn, &account, &Currency::BTC)
    //             .expect("Failed to create overview");

    //     let updated_overview =
    //         WorkerAccountOverview::update_total_balance(&mut conn, overview_btc, dec!(10))
    //             .expect("Should fail to update total balance");

    //     assert_eq!(updated_overview.total_balance, dec!(10));

    //     let updated_overview =
    //         WorkerAccountOverview::update_total_available(&mut conn, updated_overview, dec!(9))
    //             .expect("Should fail to update total available");

    //     assert_eq!(updated_overview.total_available, dec!(9));
    // }
}
