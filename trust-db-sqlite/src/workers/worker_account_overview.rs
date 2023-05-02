use crate::schema::account_overviews;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::SqliteConnection;
use rust_decimal_macros::dec;
use std::error::Error;
use tracing::error;
use trust_model::{Account, AccountOverview, Currency, Price};
use uuid::Uuid;

use super::worker_price::WorkerPrice;

pub struct WorkerAccountOverview;

impl WorkerAccountOverview {
    fn new(
        connection: &mut SqliteConnection,
        account: &Account,
        currency: Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let total_balance = WorkerPrice::new(connection, &currency, dec!(0))?;
        let total_in_trade = WorkerPrice::new(connection, &currency, dec!(0))?;
        let total_available = WorkerPrice::new(connection, &currency, dec!(0))?;
        let total_taxable = WorkerPrice::new(connection, &currency, dec!(0))?;

        let new_account_overview = NewAccountOverview {
            id,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: account.id.to_string(),
            total_balance_id: total_balance.id.to_string(),
            total_in_trade_id: total_in_trade.id.to_string(),
            total_available_id: total_available.id.to_string(),
            total_taxable_id: total_taxable.id.to_string(),
            currency: currency.to_string(),
        };

        let overview = diesel::insert_into(account_overviews::table)
            .values(&new_account_overview)
            .get_result::<AccountOverviewSQLite>(connection)
            .map(|overview| overview.domain_model(connection))
            .map_err(|error| {
                error!("Error creating overview: {:?}", error);
                error
            })?;
        Ok(overview)
    }

    fn read(
        connection: &mut SqliteConnection,
        account: &Account,
    ) -> Result<Vec<AccountOverview>, Box<dyn Error>> {
        let overviews = account_overviews::table
            .filter(account_overviews::account_id.eq(account.id.to_string()))
            .filter(account_overviews::deleted_at.is_null())
            .load::<AccountOverviewSQLite>(connection)
            .map(|overviews| {
                overviews
                    .into_iter()
                    .map(|overview| overview.domain_model(connection))
                    .collect()
            })
            .map_err(|error| {
                error!("Error reading overviews: {:?}", error);
                error
            })?;
        Ok(overviews)
    }
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = account_overviews)]
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
    total_taxable_id: String,
    currency: String,
}

impl AccountOverviewSQLite {
    fn domain_model(self, connection: &mut SqliteConnection) -> AccountOverview {
        use std::str::FromStr;
        AccountOverview {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            account_id: Uuid::parse_str(&self.account_id).unwrap(),
            total_balance: WorkerPrice::read(
                connection,
                Uuid::parse_str(&self.total_balance_id).unwrap(),
            )
            .unwrap(),
            total_in_trade: WorkerPrice::read(
                connection,
                Uuid::parse_str(&self.total_in_trade_id).unwrap(),
            )
            .unwrap(),
            total_available: WorkerPrice::read(
                connection,
                Uuid::parse_str(&self.total_available_id).unwrap(),
            )
            .unwrap(),
            total_taxable: WorkerPrice::read(
                connection,
                Uuid::parse_str(&self.total_taxable_id).unwrap(),
            )
            .unwrap(),
            currency: Currency::from_str(&self.currency).unwrap(),
        }
    }
}

#[derive(Insertable)]
#[diesel(table_name = account_overviews)]
pub struct NewAccountOverview {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    account_id: String,
    total_balance_id: String,
    total_in_trade_id: String,
    total_available_id: String,
    total_taxable_id: String,
    currency: String,
}

#[cfg(test)]
mod tests {
    use crate::workers::worker_account::WorkerAccount;

    use super::*;
    use diesel_migrations::*;

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        // This will run the necessary migrations.
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    #[test]
    fn test_create_overview() {
        let mut conn = establish_connection();

        let account = WorkerAccount::create_account(&mut conn, "Test Account", "Some description")
            .expect("Failed to create account");
        let overview = WorkerAccountOverview::new(&mut conn, &account, Currency::BTC)
            .expect("Failed to create overview");

        assert_eq!(overview.account_id, account.id);
        assert_eq!(overview.currency, Currency::BTC);
        assert_eq!(overview.total_balance.amount, dec!(0));
        assert_eq!(overview.total_in_trade.amount, dec!(0));
        assert_eq!(overview.total_available.amount, dec!(0));
        assert_eq!(overview.total_taxable.amount, dec!(0));
        assert_eq!(overview.total_balance.currency, Currency::BTC);
        assert_eq!(overview.total_in_trade.currency, Currency::BTC);
        assert_eq!(overview.total_available.currency, Currency::BTC);
        assert_eq!(overview.total_taxable.currency, Currency::BTC);
    }

    #[test]
    fn test_read_overviews() {
        let mut conn = establish_connection();

        let account = WorkerAccount::create_account(&mut conn, "Test Account", "Some description")
            .expect("Failed to create account");
        let overview_btc: AccountOverview =
            WorkerAccountOverview::new(&mut conn, &account, Currency::BTC)
                .expect("Failed to create overview");
        let overview_usd: AccountOverview =
            WorkerAccountOverview::new(&mut conn, &account, Currency::USD)
                .expect("Failed to create overview");

        let overviews =
            WorkerAccountOverview::read(&mut conn, &account).expect("Failed to read overviews");

        assert_eq!(overviews.len(), 2);
        assert_eq!(overviews[0], overview_btc);
        assert_eq!(overviews[1], overview_usd);
    }
}