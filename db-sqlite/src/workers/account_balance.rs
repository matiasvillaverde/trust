use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
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

/// Database worker for account balance operations
pub struct AccountBalanceDB {
    pub connection: Arc<Mutex<SqliteConnection>>,
}

impl std::fmt::Debug for AccountBalanceDB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountBalanceDB")
            .field("connection", &"Arc<Mutex<SqliteConnection>>")
            .finish()
    }
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

        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });

        diesel::insert_into(accounts_balances::table)
            .values(&new_account_balance)
            .get_result::<AccountBalanceSQLite>(connection)
            .map_err(|error| {
                error!("Error creating balance: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    fn update(
        &mut self,
        balance: &AccountBalance,
        total_balance: Decimal,
        total_in_trade: Decimal,
        total_available: Decimal,
        total_taxed: Decimal,
    ) -> Result<AccountBalance, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });
        let now = Utc::now().naive_utc();
        diesel::update(accounts_balances::table)
            .filter(accounts_balances::id.eq(&balance.id.to_string()))
            .set((
                accounts_balances::updated_at.eq(now),
                accounts_balances::total_balance.eq(total_balance.to_string()),
                accounts_balances::total_available.eq(total_available.to_string()),
                accounts_balances::total_in_trade.eq(total_in_trade.to_string()),
                accounts_balances::taxed.eq(total_taxed.to_string()),
            ))
            .execute(connection)
            .map_err(|error| {
                error!("Error updating balance: {:?}", error);
                error
            })?;

        let mut updated = *balance;
        updated.updated_at = now;
        updated.total_balance = total_balance;
        updated.total_in_trade = total_in_trade;
        updated.total_available = total_available;
        updated.taxed = total_taxed;
        Ok(updated)
    }
}

impl AccountBalanceRead for AccountBalanceDB {
    fn for_account(&mut self, account_id: Uuid) -> Result<Vec<AccountBalance>, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });
        accounts_balances::table
            .filter(accounts_balances::account_id.eq(account_id.to_string()))
            .filter(accounts_balances::deleted_at.is_null())
            .load::<AccountBalanceSQLite>(connection)
            .map_err(|error| {
                error!("Error reading balances: {:?}", error);
                error
            })?
            .into_domain_models()
    }

    fn for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountBalance, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {e}");
            std::process::exit(1);
        });
        accounts_balances::table
            .filter(accounts_balances::account_id.eq(account_id.to_string()))
            .filter(accounts_balances::currency.eq(currency.to_string()))
            .filter(accounts_balances::deleted_at.is_null())
            .first::<AccountBalanceSQLite>(connection)
            .map_err(|error| {
                error!("Error reading balance: {:?}", error);
                error
            })?
            .into_domain_model()
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

impl TryFrom<AccountBalanceSQLite> for AccountBalance {
    type Error = ConversionError;

    fn try_from(value: AccountBalanceSQLite) -> Result<Self, Self::Error> {
        use std::str::FromStr;
        Ok(AccountBalance {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse balance ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            account_id: Uuid::parse_str(&value.account_id)
                .map_err(|_| ConversionError::new("account_id", "Failed to parse account ID"))?,
            total_balance: Decimal::from_str(&value.total_balance).map_err(|_| {
                ConversionError::new("total_balance", "Failed to parse total balance")
            })?,
            total_in_trade: Decimal::from_str(&value.total_in_trade).map_err(|_| {
                ConversionError::new("total_in_trade", "Failed to parse total in trade")
            })?,
            total_available: Decimal::from_str(&value.total_available).map_err(|_| {
                ConversionError::new("total_available", "Failed to parse total available")
            })?,
            taxed: Decimal::from_str(&value.taxed)
                .map_err(|_| ConversionError::new("taxed", "Failed to parse taxed amount"))?,
            currency: Currency::from_str(&value.currency)
                .map_err(|_| ConversionError::new("currency", "Failed to parse currency"))?,
            total_earnings: Decimal::from_str(&value.total_earnings).map_err(|_| {
                ConversionError::new("total_earnings", "Failed to parse total earnings")
            })?,
        })
    }
}

impl IntoDomainModel<AccountBalance> for AccountBalanceSQLite {
    fn into_domain_model(self) -> Result<AccountBalance, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Debug, Insertable)]
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
        assert_eq!(
            balances.first().expect("Expected first balance"),
            &balance_btc
        );
        assert_eq!(
            balances.get(1).expect("Expected second balance"),
            &balance_usd
        );
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
