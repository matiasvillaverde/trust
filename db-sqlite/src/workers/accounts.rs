use crate::error::{ConversionError, IntoDomainModel, IntoDomainModels};
use crate::schema::accounts;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use model::AccountRead;
use model::{Account, AccountWrite, Environment};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::error;
use uuid::Uuid;

/// Database worker for account operations
pub struct AccountDB {
    pub connection: Arc<Mutex<SqliteConnection>>,
}

impl std::fmt::Debug for AccountDB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountDB")
            .field("connection", &"Arc<Mutex<SqliteConnection>>")
            .finish()
    }
}

impl AccountWrite for AccountDB {
    fn create(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
    ) -> Result<Account, Box<dyn Error>> {
        let uuid = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let new_account = NewAccount {
            id: uuid,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            name: name.to_lowercase(),
            description: description.to_lowercase(),
            environment: environment.to_string(),
            taxes_percentage: taxes_percentage.to_string(),
            earnings_percentage: earnings_percentage.to_string(),
        };

        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {}", e);
            std::process::exit(1);
        });

        diesel::insert_into(accounts::table)
            .values(&new_account)
            .get_result::<AccountSQLite>(connection)
            .map_err(|error| {
                error!("Error creating account: {:?}", error);
                error
            })?
            .into_domain_model()
    }
}

impl AccountRead for AccountDB {
    fn for_name(&mut self, name: &str) -> Result<Account, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {}", e);
            std::process::exit(1);
        });

        accounts::table
            .filter(accounts::name.eq(name.to_lowercase()))
            .first::<AccountSQLite>(connection)
            .map_err(|error| {
                error!("Error reading account: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    fn id(&mut self, id: Uuid) -> Result<Account, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {}", e);
            std::process::exit(1);
        });

        accounts::table
            .filter(accounts::id.eq(id.to_string()))
            .first::<AccountSQLite>(connection)
            .map_err(|error| {
                error!("Error reading account: {:?}", error);
                error
            })?
            .into_domain_model()
    }

    fn all(&mut self) -> Result<Vec<Account>, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap_or_else(|e| {
            eprintln!("Failed to acquire connection lock: {}", e);
            std::process::exit(1);
        });
        accounts::table
            .filter(accounts::deleted_at.is_null())
            .load::<AccountSQLite>(connection)
            .map_err(|error| {
                error!("Error reading all accounts: {:?}", error);
                error
            })?
            .into_domain_models()
    }
}

#[derive(Queryable, Identifiable, AsChangeset, Insertable)]
#[diesel(table_name = accounts)]
#[diesel(primary_key(id))]
#[diesel(treat_none_as_null = true)]
pub struct AccountSQLite {
    pub id: String,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub deleted_at: Option<NaiveDateTime>,
    pub name: String,
    pub description: String,
    pub environment: String,
    pub taxes_percentage: String,
    pub earnings_percentage: String,
}

impl TryFrom<AccountSQLite> for Account {
    type Error = ConversionError;

    fn try_from(value: AccountSQLite) -> Result<Self, Self::Error> {
        Ok(Account {
            id: Uuid::parse_str(&value.id)
                .map_err(|_| ConversionError::new("id", "Failed to parse account ID"))?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            deleted_at: value.deleted_at,
            name: value.name,
            description: value.description,
            environment: Environment::from_str(&value.environment)
                .map_err(|_| ConversionError::new("environment", "Failed to parse environment"))?,
            taxes_percentage: Decimal::from_str(&value.taxes_percentage).map_err(|_| {
                ConversionError::new("taxes_percentage", "Failed to parse taxes percentage")
            })?,
            earnings_percentage: Decimal::from_str(&value.earnings_percentage).map_err(|_| {
                ConversionError::new("earnings_percentage", "Failed to parse earnings percentage")
            })?,
        })
    }
}

impl IntoDomainModel<Account> for AccountSQLite {
    fn into_domain_model(self) -> Result<Account, Box<dyn Error>> {
        self.try_into().map_err(Into::into)
    }
}

#[derive(Insertable)]
#[diesel(table_name = accounts)]
#[diesel(treat_none_as_null = true)]
struct NewAccount {
    id: String,
    created_at: NaiveDateTime,
    updated_at: NaiveDateTime,
    deleted_at: Option<NaiveDateTime>,
    name: String,
    description: String,
    environment: String,
    taxes_percentage: String,
    earnings_percentage: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteDatabase;
    use diesel_migrations::*;
    use model::DatabaseFactory;
    use rust_decimal_macros::dec;
    pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
    // Declare a test database connection
    fn establish_connection() -> SqliteConnection {
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        // This will run the necessary migrations.
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        connection
    }
    fn create_factory(connection: SqliteConnection) -> Box<dyn DatabaseFactory> {
        Box::new(SqliteDatabase::new_from(Arc::new(Mutex::new(connection))))
    }

    #[test]
    fn test_create_account() {
        let conn: SqliteConnection = establish_connection();
        let mut db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        // Create a new account record
        let account = db
            .create(
                "Test Account",
                "This is a test account",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect("Error creating account");
        assert_eq!(account.name, "test account"); // it should be lowercase
        assert_eq!(account.description, "this is a test account"); // it should be lowercase
        assert_eq!(account.environment, Environment::Paper);
        assert_eq!(account.deleted_at, None);
    }
    #[test]
    fn test_read_account() {
        let conn = establish_connection();
        let mut db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        // Create a new account record
        let created_account = db
            .create(
                "Test Account",
                "This is a test account",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect("Error creating account");
        // Read the account record by name
        let read_account = db
            .for_name("Test Account")
            .expect("Account should be found");
        assert_eq!(read_account, created_account);
    }
    #[test]
    fn test_read_account_id() {
        let conn = establish_connection();
        let mut db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        // Create a new account record
        let created_account = db
            .create(
                "Test Account",
                "This is a test account",
                Environment::Paper,
                dec!(20),
                dec!(80),
            )
            .expect("Error creating account");
        // Read the account record by name
        let read_account = db.id(created_account.id).expect("Account should be found");
        assert_eq!(read_account, created_account);
    }
    #[test]
    fn test_create_account_same_name() {
        let conn = establish_connection();
        let mut db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        let name = "Test Account";
        // Create a new account record
        db.create(
            name,
            "This is a test account",
            Environment::Paper,
            dec!(20),
            dec!(80),
        )
        .expect("Error creating account");
        // Create a new account record with the same name
        db.create(
            name,
            "This is a test account",
            Environment::Paper,
            dec!(20),
            dec!(80),
        )
        .expect_err("Error creating account with same name");
    }
    #[test]
    fn test_read_account_not_found() {
        let conn = establish_connection();
        let mut db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        db.for_name("Non existent account")
            .expect_err("Account should not be found");
    }
    #[test]
    fn test_read_all_accounts() {
        let db = create_factory(establish_connection());
        let created_accounts = vec![
            db.account_write()
                .create(
                    "Test Account 1",
                    "This is a test account",
                    Environment::Paper,
                    dec!(20),
                    dec!(80),
                )
                .expect("Error creating account"),
            db.account_write()
                .create(
                    "Test Account 2",
                    "This is a test account",
                    Environment::Paper,
                    dec!(20),
                    dec!(80),
                )
                .expect("Error creating account"),
            db.account_write()
                .create(
                    "Test Account 3",
                    "This is a test account",
                    Environment::Paper,
                    dec!(20),
                    dec!(80),
                )
                .expect("Error creating account"),
        ];

        // Read all account records
        let accounts = db.account_read().all().expect("Error reading all accounts");
        assert_eq!(accounts, created_accounts);
    }
}
