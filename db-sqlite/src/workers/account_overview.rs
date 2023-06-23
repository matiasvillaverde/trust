use crate::schema::accounts_overviews;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::SqliteConnection;
use model::{Account, AccountOverview, Currency, ReadAccountOverviewDB, WriteAccountOverviewDB};
use rust_decimal::Decimal;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::error;
use uuid::Uuid;

pub struct AccountOverviewDB {
    pub connection: Arc<Mutex<SqliteConnection>>,
}

impl WriteAccountOverviewDB for AccountOverviewDB {
    fn create_account_overview(
        &mut self,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let new_account_overview = NewAccountOverview {
            account_id: account.id.to_string(),
            currency: currency.to_string(),
            ..Default::default()
        };

        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap();

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

    fn update_account_overview(
        &mut self,
        overview: &AccountOverview,
        total_balance: Decimal,
        total_in_trade: Decimal,
        total_available: Decimal,
        total_taxed: Decimal,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap();
        let overview = diesel::update(accounts_overviews::table)
            .filter(accounts_overviews::id.eq(&overview.id.to_string()))
            .set((
                accounts_overviews::updated_at.eq(Utc::now().naive_utc()),
                accounts_overviews::total_balance.eq(total_balance.to_string()),
                accounts_overviews::total_available.eq(total_available.to_string()),
                accounts_overviews::total_in_trade.eq(total_in_trade.to_string()),
                accounts_overviews::taxed.eq(total_taxed.to_string()),
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

impl ReadAccountOverviewDB for AccountOverviewDB {
    fn read_account_overview(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountOverview>, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap();
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

    fn read_account_overview_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap();
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
    total_balance: String,
    total_in_trade: String,
    total_available: String,
    taxed: String,
    currency: String,
    total_earnings: String,
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
            total_balance: Decimal::from_str(&self.total_balance).unwrap(),
            total_in_trade: Decimal::from_str(&self.total_in_trade).unwrap(),
            total_available: Decimal::from_str(&self.total_available).unwrap(),
            taxed: Decimal::from_str(&self.taxed).unwrap(),
            currency: Currency::from_str(&self.currency).unwrap(),
            total_earnings: Decimal::from_str(&self.total_earnings).unwrap(),
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
    total_balance: String,
    total_in_trade: String,
    total_available: String,
    taxed: String,
    currency: String,
    total_earnings: String,
}

impl Default for NewAccountOverview {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        NewAccountOverview {
            id: Uuid::new_v4().to_string(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            account_id: "0".to_string(),
            total_balance: "0".to_string(),
            total_in_trade: "0".to_string(),
            total_available: "0".to_string(),
            taxed: "0".to_string(),
            currency: Currency::USD.to_string(),
            total_earnings: "0".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use crate::SqliteDatabase;

    use super::*;
    use diesel_migrations::*;
    use model::DatabaseFactory;
    use rust_decimal_macros::dec;

    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        // This will run the necessary migrations.
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }

    fn create_factory() -> Box<dyn DatabaseFactory> {
        Box::new(SqliteDatabase::new_from(Arc::new(Mutex::new(
            establish_connection(),
        ))))
    }

    #[test]
    fn test_create_overview() {
        let db = create_factory();

        let account = db
            .write_account_db()
            .create_account(
                "Test Account",
                "Some description",
                model::Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("Failed to create account");
        let mut db = db.write_account_overview_db();
        let overview = db
            .create_account_overview(&account, &Currency::BTC)
            .expect("Failed to create overview");

        assert_eq!(overview.account_id, account.id);
        assert_eq!(overview.currency, Currency::BTC);
        assert_eq!(overview.total_balance, dec!(0));
        assert_eq!(overview.total_in_trade, dec!(0));
        assert_eq!(overview.total_available, dec!(0));
        assert_eq!(overview.taxed, dec!(0));
    }

    #[test]
    fn test_read_overviews() {
        let db = create_factory();

        let account = db
            .write_account_db()
            .create_account(
                "Test Account",
                "Some description",
                model::Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("Failed to create account");
        let mut write_db = db.write_account_overview_db();

        let overview_btc = write_db
            .create_account_overview(&account, &Currency::BTC)
            .expect("Failed to create overview");
        let overview_usd = write_db
            .create_account_overview(&account, &Currency::USD)
            .expect("Failed to create overview");

        let mut db = db.read_account_overview_db();
        let overviews = db
            .read_account_overview(account.id)
            .expect("Failed to read overviews");

        assert_eq!(overviews.len(), 2);
        assert_eq!(overviews[0], overview_btc);
        assert_eq!(overviews[1], overview_usd);
    }

    #[test]
    fn test_update() {
        let db = create_factory();

        let account = db
            .write_account_db()
            .create_account(
                "Test Account",
                "Some description",
                model::Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("Failed to create account");
        let mut db = db.write_account_overview_db();
        let overview = db
            .create_account_overview(&account, &Currency::BTC)
            .expect("Failed to create overview");

        let updated_overview = db
            .update_account_overview(&overview, dec!(200), dec!(1), dec!(203), dec!(44.2))
            .expect("Failed to update overview");

        assert_eq!(updated_overview.total_balance, dec!(200));
        assert_eq!(updated_overview.total_available, dec!(203));
        assert_eq!(updated_overview.total_in_trade, dec!(1));
        assert_eq!(updated_overview.taxed, dec!(44.2));
    }
}
