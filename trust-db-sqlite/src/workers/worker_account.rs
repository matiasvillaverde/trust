use crate::schema::accounts;
use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use std::{error::Error, str::FromStr};
use tracing::error;
use trust_model::Account;
use uuid::Uuid;

/// WorkerAccount is a struct that contains methods for interacting with the
/// accounts table in the database.
/// The methods in this struct are used by the worker to create and read
/// accounts.
pub struct WorkerAccount;

impl WorkerAccount {
    pub fn create_account(
        connection: &mut SqliteConnection,
        name: &str,
        description: &str,
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
        };

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

    pub fn read_account(
        connection: &mut SqliteConnection,
        name: &str,
    ) -> Result<Account, Box<dyn Error>> {
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

    pub fn read(connection: &mut SqliteConnection, id: Uuid) -> Result<Account, Box<dyn Error>> {
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

    pub fn read_all_accounts(
        connection: &mut SqliteConnection,
    ) -> Result<Vec<Account>, Box<dyn Error>> {
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

    fn create_accounts(conn: &mut SqliteConnection) -> Vec<Account> {
        let created_accounts = vec![
            WorkerAccount::create_account(conn, "Test Account 1", "This is a test account")
                .expect("Error creating account"),
            WorkerAccount::create_account(conn, "Test Account 2", "This is a test account")
                .expect("Error creating account"),
            WorkerAccount::create_account(conn, "Test Account 3", "This is a test account")
                .expect("Error creating account"),
        ];

        created_accounts
    }

    #[test]
    fn test_create_account() {
        let mut conn: SqliteConnection = establish_connection();

        // Create a new account record
        let account =
            WorkerAccount::create_account(&mut conn, "Test Account", "This is a test account")
                .expect("Error creating account");

        assert_eq!(account.name, "test account"); // it should be lowercase
        assert_eq!(account.description, "this is a test account"); // it should be lowercase
        assert_eq!(account.deleted_at, None);
    }

    #[test]
    fn test_read_account() {
        let mut conn = establish_connection();

        // Create a new account record
        let created_account =
            WorkerAccount::create_account(&mut conn, "Test Account", "This is a test account")
                .expect("Error creating account");

        // Read the account record by name
        let read_account = WorkerAccount::read_account(&mut conn, "Test Account")
            .expect("Account should be found");

        assert_eq!(read_account, created_account);
    }

    #[test]
    fn test_read_account_id() {
        let mut conn = establish_connection();

        // Create a new account record
        let created_account =
            WorkerAccount::create_account(&mut conn, "Test Account", "This is a test account")
                .expect("Error creating account");

        // Read the account record by name
        let read_account =
            WorkerAccount::read(&mut conn, created_account.id).expect("Account should be found");

        assert_eq!(read_account, created_account);
    }

    #[test]
    fn test_create_account_same_name() {
        let mut conn = establish_connection();

        let name = "Test Account";

        // Create a new account record
        WorkerAccount::create_account(&mut conn, name, "This is a test account")
            .expect("Error creating account");

        // Create a new account record with the same name
        WorkerAccount::create_account(&mut conn, name, "This is a test account")
            .expect_err("Error creating account with same name");
    }

    #[test]
    fn test_read_account_not_found() {
        let mut conn = establish_connection();
        WorkerAccount::read_account(&mut conn, "Non existent account")
            .expect_err("Account should not be found");
    }

    #[test]
    fn test_read_all_accounts() {
        let mut conn = establish_connection();
        let created_accounts = create_accounts(&mut conn);

        // Read all account records
        let accounts =
            WorkerAccount::read_all_accounts(&mut conn).expect("Error reading all accounts");
        assert_eq!(accounts, created_accounts);
    }

    #[test]
    fn test_read_all_accounts_deleted() {
        let mut conn = establish_connection();
        let created_accounts = create_accounts(&mut conn);

        // Create 3 accounts
        let account = created_accounts.first().unwrap();

        // Delete an account record
        diesel::update(accounts::table.find(account.id.to_string()))
            .set((
                accounts::updated_at.eq(Utc::now().naive_utc()),
                accounts::deleted_at.eq(Utc::now().naive_utc()),
            ))
            .execute(&mut conn)
            .expect("Error deleting account");

        // Read all account records
        let accounts =
            WorkerAccount::read_all_accounts(&mut conn).expect("Error reading all accounts");
        assert_eq!(accounts.len(), 2);
    }
}
