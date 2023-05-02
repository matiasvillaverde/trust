use rust_decimal::Decimal;
use transaction_worker::TransactionWorker;
use trust_model::{Account, AccountOverview, Currency, Database, Transaction, TransactionCategory};
use uuid::Uuid;

pub struct Trust {
    database: Box<dyn Database>,
}

/// Trust is the main entry point for interacting with the trust-core library.
/// It is a facade that provides a simple interface for interacting with the
/// trust-core library.
impl Trust {
    /// Creates a new instance of Trust.
    pub fn new(database: Box<dyn Database>) -> Self {
        Trust { database: database }
    }

    /// Creates a new account.
    pub fn create_account(
        &mut self,
        name: &str,
        description: &str,
    ) -> Result<Account, Box<dyn std::error::Error>> {
        self.database.new_account(name, description)
    }

    pub fn search_account(&mut self, name: &str) -> Result<Account, Box<dyn std::error::Error>> {
        self.database.read_account(name)
    }

    pub fn search_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn std::error::Error>> {
        self.database.read_all_accounts()
    }

    pub fn create_transaction(
        &mut self,
        account: &Account,
        category: &TransactionCategory,
        amount: Decimal,
        currency: &Currency,
    ) -> Result<(Transaction, AccountOverview), Box<dyn std::error::Error>> {
        TransactionWorker::new(&mut *self.database, category, amount, currency, account.id)
    }

    pub fn search_overview(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn std::error::Error>> {
        self.database
            .read_account_overview_currency(account_id, currency)
    }

    pub fn search_all_overviews(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountOverview>, Box<dyn std::error::Error>> {
        self.database.read_account_overview(account_id)
    }
}

mod transaction_validator;
mod transaction_worker;
