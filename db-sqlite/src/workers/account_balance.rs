use crate::schema::accounts_balances;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::SqliteConnection;
use model::{Account, AccountBalance, AccountBalanceRead, AccountBalanceWrite, Currency};
use rust_decimal::Decimal;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::error;
use uuid::Uuid;

pub struct AccountBalanceDB {
    pub connection: Arc<Mutex<SqliteConnection>>,
}

impl AccountBalanceWrite for AccountBalanceDB {
    fn create(
        &mut self,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountBalance, Box<dyn Error>> {
        let new_account_balance = NewAccountBalance {
            account_id: account.id.to_string(),
            currency: currency.to_string(),
            ..Default::default()
        };

        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap();

        let balance = diesel::insert_into(accounts_balances::table)
            .values(&new_account_balance)
            .get_result::<AccountBalanceSQLite>(connection)
            .map(|balance| balance.domain_model())
            .map_err(|error| {
                error!("Error creating balance: {:?}", error);
                error
            })?;
        Ok(balance)
    }

    fn update(
        &mut self,
        balance: &AccountBalance,
        total_balance: Decimal,
        total_in_trade: Decimal,
        total_available: Decimal,
        total_taxed: Decimal,
    ) -> Result<AccountBalance, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap();
        let balance = diesel::update(accounts_balances::table)
            .filter(accounts_balances::id.eq(&balance.id.to_string()))
            .set((
                accounts_balances::updated_at.eq(Utc::now().naive_utc()),
                accounts_balances::total_balance.eq(total_balance.to_string()),
                accounts_balances::total_available.eq(total_available.to_string()),
                accounts_balances::total_in_trade.eq(total_in_trade.to_string()),
                accounts_balances::taxed.eq(total_taxed.to_string()),
            ))
            .get_result::<AccountBalanceSQLite>(connection)
            .map(|o| o.domain_model())
            .map_err(|error| {
                error!("Error updating balance: {:?}", error);
                error
            })?;
        Ok(balance)
    }
}

impl AccountBalanceRead for AccountBalanceDB {
    fn for_account(&mut self, account_id: Uuid) -> Result<Vec<AccountBalance>, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap();
        let balances = accounts_balances::table
            .filter(accounts_balances::account_id.eq(account_id.to_string()))
            .filter(accounts_balances::deleted_at.is_null())
            .load::<AccountBalanceSQLite>(connection)
            .map(|balances| {
                balances
                    .into_iter()
                    .map(|balance| balance.domain_model())
                    .collect()
            })
            .map_err(|error| {
                error!("Error reading balances: {:?}", error);
                error
            })?;
        Ok(balances)
    }

    fn for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountBalance, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap();
        let balances = accounts_balances::table
            .filter(accounts_balances::account_id.eq(account_id.to_string()))
            .filter(accounts_balances::currency.eq(currency.to_string()))
            .filter(accounts_balances::deleted_at.is_null())
            .first::<AccountBalanceSQLite>(connection)
            .map(|balance| balance.domain_model())
            .map_err(|error| {
                error!("Error creating balance: {:?}", error);
                error
            })?;
        Ok(balances)
    }
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = accounts_balances)]
#[diesel(treat_none_as_null = true)]
struct AccountBalanceSQLite {
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

impl AccountBalanceSQLite {
    fn domain_model(self) -> AccountBalance {
        use std::str::FromStr;
        AccountBalance {
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
#[diesel(table_name = accounts_balances)]
pub struct NewAccountBalance {
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

impl Default for NewAccountBalance {
    fn default() -> Self {
        let now = Utc::now().naive_utc();
        NewAccountBalance {
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
    fn test_create_balance() {
        let db = create_factory();

        let account = db
            .account_write()
            .create(
                "Test Account",
                "Some description",
                model::Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("Failed to create account");
        let mut db = db.account_balance_write();
        let balance = db
            .create(&account, &Currency::BTC)
            .expect("Failed to create balance");

        assert_eq!(balance.account_id, account.id);
        assert_eq!(balance.currency, Currency::BTC);
        assert_eq!(balance.total_balance, dec!(0));
        assert_eq!(balance.total_in_trade, dec!(0));
        assert_eq!(balance.total_available, dec!(0));
        assert_eq!(balance.taxed, dec!(0));
    }

    #[test]
    fn test_read_balances() {
        let db = create_factory();

        let account = db
            .account_write()
            .create(
                "Test Account",
                "Some description",
                model::Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("Failed to create account");
        let mut write_db = db.account_balance_write();

        let balance_btc = write_db
            .create(&account, &Currency::BTC)
            .expect("Failed to create balance");
        let balance_usd = write_db
            .create(&account, &Currency::USD)
            .expect("Failed to create balance");

        let mut db = db.account_balance_read();
        let balances = db.for_account(account.id).expect("Failed to read balances");

        assert_eq!(balances.len(), 2);
        assert_eq!(balances[0], balance_btc);
        assert_eq!(balances[1], balance_usd);
    }

    #[test]
    fn test_update() {
        let db = create_factory();

        let account = db
            .account_write()
            .create(
                "Test Account",
                "Some description",
                model::Environment::Paper,
                dec!(20),
                dec!(10),
            )
            .expect("Failed to create account");
        let mut db = db.account_balance_write();
        let balance = db
            .create(&account, &Currency::BTC)
            .expect("Failed to create balance");

        let updated_balance = db
            .update(&balance, dec!(200), dec!(1), dec!(203), dec!(44.2))
            .expect("Failed to update balance");

        assert_eq!(updated_balance.total_balance, dec!(200));
        assert_eq!(updated_balance.total_available, dec!(203));
        assert_eq!(updated_balance.total_in_trade, dec!(1));
        assert_eq!(updated_balance.taxed, dec!(44.2));
    }
}
