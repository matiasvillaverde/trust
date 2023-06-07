use crate::schema::accounts;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::error;
use trust_model::ReadAccountDB;
use trust_model::{Account, WriteAccountDB};
use uuid::Uuid;

/// AccountDB is a struct that contains methods for interacting with the
/// accounts table in the database.
/// The methods in this struct are used by the worker to create and read
/// accounts.
///

pub struct AccountDB {
    pub connection: Arc<Mutex<SqliteConnection>>,
}

impl WriteAccountDB for AccountDB {
    fn create_account(&mut self, name: &str, description: &str) -> Result<Account, Box<dyn Error>> {
        let uuid = Uuid::new_v4().to_string();
        let now = Utc::now().naive_utc();

        let new_account = NewAccount {
            id: uuid,
            created_at: now,
            updated_at: now,
            deleted_at: None,
            name: name.to_lowercase(),
            description: description.to_lowercase(),
        };

        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap();

        let account = diesel::insert_into(accounts::table)
            .values(&new_account)
            .get_result::<AccountSQLite>(connection)
            .map(|account| account.domain_model())
            .map_err(|error| {
                error!("Error creating account: {:?}", error);
                error
            })?;
        Ok(account)
    }
}

impl ReadAccountDB for AccountDB {
    fn read_account(&mut self, name: &str) -> Result<Account, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap();

        let account = accounts::table
            .filter(accounts::name.eq(name.to_lowercase()))
            .first::<AccountSQLite>(connection)
            .map(|account| account.domain_model())
            .map_err(|error| {
                error!("Error reading account: {:?}", error);
                error
            })?;
        Ok(account)
    }

    fn read_account_id(&mut self, id: Uuid) -> Result<Account, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap();

        let account = accounts::table
            .filter(accounts::id.eq(id.to_string()))
            .first::<AccountSQLite>(connection)
            .map(|account| account.domain_model())
            .map_err(|error| {
                error!("Error reading account: {:?}", error);
                error
            })?;
        Ok(account)
    }

    fn read_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn Error>> {
        let connection: &mut SqliteConnection = &mut self.connection.lock().unwrap();
        let accounts = accounts::table
            .filter(accounts::deleted_at.is_null())
            .load::<AccountSQLite>(connection)
            .map(|accounts| {
                accounts
                    .into_iter()
                    .map(|account| account.domain_model())
                    .collect()
            })
            .map_err(|error| {
                error!("Error reading all accounts: {:?}", error);
                error
            })?;
        Ok(accounts)
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
}

impl AccountSQLite {
    fn domain_model(self) -> Account {
        Account {
            id: Uuid::parse_str(&self.id).unwrap(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
            name: self.name,
            description: self.description,
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteDatabase;
    use diesel_migrations::*;
    use trust_model::DatabaseFactory;
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
            .create_account("Test Account", "This is a test account")
            .expect("Error creating account");
        assert_eq!(account.name, "test account"); // it should be lowercase
        assert_eq!(account.description, "this is a test account"); // it should be lowercase
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
            .create_account("Test Account", "This is a test account")
            .expect("Error creating account");
        // Read the account record by name
        let read_account = db
            .read_account("Test Account")
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
            .create_account("Test Account", "This is a test account")
            .expect("Error creating account");
        // Read the account record by name
        let read_account = db
            .read_account_id(created_account.id)
            .expect("Account should be found");
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
        db.create_account(name, "This is a test account")
            .expect("Error creating account");
        // Create a new account record with the same name
        db.create_account(name, "This is a test account")
            .expect_err("Error creating account with same name");
    }
    #[test]
    fn test_read_account_not_found() {
        let conn = establish_connection();
        let mut db = AccountDB {
            connection: Arc::new(Mutex::new(conn)),
        };
        db.read_account("Non existent account")
            .expect_err("Account should not be found");
    }
    #[test]
    fn test_read_all_accounts() {
        let db = create_factory(establish_connection());
        let created_accounts = vec![
            db.write_account_db()
                .create_account("Test Account 1", "This is a test account")
                .expect("Error creating account"),
            db.write_account_db()
                .create_account("Test Account 2", "This is a test account")
                .expect("Error creating account"),
            db.write_account_db()
                .create_account("Test Account 3", "This is a test account")
                .expect("Error creating account"),
        ];

        // Read all account records
        let accounts = db
            .read_account_db()
            .read_all_accounts()
            .expect("Error reading all accounts");
        assert_eq!(accounts, created_accounts);
    }
}
