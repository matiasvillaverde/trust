use crate::workers::{
    worker_account::WorkerAccount, worker_price::WorkerPrice, worker_transaction::WorkerTransaction,
};
use diesel::prelude::*;
use std::error::Error;
use trust_model::{
    Account, AccountOverview, Currency, Database, Price, Transaction, TransactionCategory,
};
use uuid::Uuid;

/// SqliteDatabase is a struct that contains methods for interacting with the
/// SQLite database.
pub struct SqliteDatabase {
    /// The connection to the SQLite database.
    connection: SqliteConnection,
}

impl SqliteDatabase {
    /// Create a new SqliteDatabase.
    /// # Arguments
    /// * `url` - A string slice that holds the URL of the SQLite database.
    /// # Returns
    /// * `SqliteDatabase` - The new SqliteDatabase.
    /// # Example
    /// ```
    /// use trust_db_sqlite::SqliteDatabase;
    ///
    /// let database = SqliteDatabase::new("sqlite://production.db");
    /// ```
    pub fn new(url: &str) -> Self {
        let connection: SqliteConnection = Self::establish_connection(url);
        SqliteDatabase { connection }
    }

    #[doc(hidden)]
    pub fn new_in_memory() -> Self {
        use diesel_migrations::*;
        pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
        let mut connection = SqliteConnection::establish(":memory:").unwrap();
        connection.run_pending_migrations(MIGRATIONS).unwrap();
        connection.begin_test_transaction().unwrap();
        SqliteDatabase { connection }
    }

    /// Establish a connection to the SQLite database.
    fn establish_connection(database_url: &str) -> SqliteConnection {
        let db_exists = std::path::Path::new(database_url).exists();
        // Use the database URL to establish a connection to the SQLite database
        let mut connection = SqliteConnection::establish(database_url)
            .unwrap_or_else(|_| panic!("Error connecting to {}", database_url));

        // Run migrations only if it is a new DB
        if !db_exists {
            use diesel_migrations::*;
            pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
            connection.run_pending_migrations(MIGRATIONS).unwrap();
        }

        connection
    }
}

impl Database for SqliteDatabase {
    /// Create a new account record in the database.
    /// The name and description are converted to lowercase before being
    /// inserted into the database.
    /// The created_at and updated_at fields are set to the current time in UTC.
    /// The deleted_at field is set to null.
    /// The id field is set to a new UUID.
    /// The account is returned as a domain model.
    /// If there is an error creating the account, an error is returned.
    /// # Arguments
    /// * `name` - A string slice that holds the name of the account. It must be unique.
    /// * `description` - A string slice that holds the description of the account.
    /// # Returns
    /// * `Result<Account, Box<dyn Error>>` - The account that was created.
    /// # Example
    /// ```
    /// use trust_db_sqlite::SqliteDatabase;
    /// use trust_model::database::Database;
    ///
    /// let mut database = SqliteDatabase::new_in_memory(); // This is for demonstration purposes only.
    /// let account = database.create_account("My Account", "My Description").unwrap();
    /// assert_eq!(account.name, "my account");
    /// ```
    fn create_account(&mut self, name: &str, description: &str) -> Result<Account, Box<dyn Error>> {
        let account = WorkerAccount::create_account(&mut self.connection, name, description);
        account
    }

    /// Read an account record from the database.
    /// The name is converted to lowercase before being used to query the database.
    /// The account is returned as a domain model.
    /// If there is an error reading the account, an error is returned.
    /// # Arguments
    /// * `name` - A string slice that holds the name of the account.
    /// # Returns
    /// * `Result<Account, Box<dyn Error>>` - The account that was read.
    /// # Example
    /// ```
    /// use trust_db_sqlite::SqliteDatabase;
    /// use trust_model::database::Database;
    ///
    /// let mut database = SqliteDatabase::new_in_memory(); // This is for demonstration purposes only.
    /// let account = database.create_account("My Account", "My Description").unwrap();
    /// let account = database.read_account("My Account").unwrap();
    /// assert_eq!(account.name, "my account");
    /// ```
    fn read_account(&mut self, name: &str) -> Result<Account, Box<dyn Error>> {
        let account = WorkerAccount::read_account(&mut self.connection, name);
        account
    }

    fn read_account_id(&mut self, id: Uuid) -> Result<Account, Box<dyn Error>> {
        WorkerAccount::read(&mut self.connection, id)
    }

    fn read_account_overview(
        &mut self,
        _account: Account,
    ) -> Result<Vec<AccountOverview>, Box<dyn Error>> {
        unimplemented!();
    }

    fn new_account_overview(
        &mut self,
        account: &Account,
    ) -> Result<AccountOverview, Box<dyn Error>> {
        unimplemented!();
    }
    fn update_account_overview(
        &mut self,
        account: &Account,
        overview: &AccountOverview,
    ) -> Result<(), Box<dyn Error>> {
        unimplemented!();
    }

    /// Read all account records from the database.
    /// The accounts are returned as domain models.
    /// Deleted accounts are not returned.
    /// If there is an error reading the accounts, an error is returned.
    /// # Returns
    /// * `Result<Vec<Account>, Box<dyn Error>>` - The accounts that were read.
    /// # Example
    /// ```
    /// use trust_db_sqlite::SqliteDatabase;
    /// use trust_model::database::Database;
    ///
    /// let mut database = SqliteDatabase::new_in_memory(); // This is for demonstration purposes only.
    /// let account = database.create_account("My Account", "My Description").unwrap();
    /// let accounts = database.read_all_accounts().unwrap();
    /// assert_eq!(accounts.len(), 1);
    /// ```
    fn read_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn Error>> {
        let accounts = WorkerAccount::read_all_accounts(&mut self.connection);
        accounts
    }

    fn create_price(
        &mut self,
        currency: Currency,
        amount: rust_decimal::Decimal,
    ) -> Result<Price, Box<dyn Error>> {
        WorkerPrice::new(&mut self.connection, &currency, amount)
    }

    fn read_price(&mut self, id: uuid::Uuid) -> Result<Price, Box<dyn Error>> {
        WorkerPrice::read(&mut self.connection, id)
    }

    fn create_transaction(
        &mut self,
        account: &Account,
        amount: rust_decimal::Decimal,
        currency: Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>> {
        WorkerTransaction::create_transaction(
            &mut self.connection,
            account.id,
            amount,
            currency,
            category,
        )
    }
}
