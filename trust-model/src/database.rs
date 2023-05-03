use crate::Currency;
use crate::Price;
use crate::Transaction;
use crate::TransactionCategory;
use crate::{Account, AccountOverview};
use rust_decimal::Decimal;
use uuid::Uuid;

use std::error::Error;

/// Database trait with all the methods that are needed to interact with the database.
///
/// The trait is used to abstract the database implementation.
/// The trait is used to:
///
/// 1. Make it easier to switch the database implementation.
/// 2. Easier to test the code.
/// 3. Easier to mock the database.
///
/// To prevent the database from being used incorrectly, the trait has the following rules:
/// - Reads can be Uuid
/// - Writes and updates must be Domain Models
pub trait Database {
    // Accounts
    fn new_account(&mut self, name: &str, description: &str) -> Result<Account, Box<dyn Error>>;
    fn read_account(&mut self, name: &str) -> Result<Account, Box<dyn Error>>;
    fn read_account_id(&mut self, id: Uuid) -> Result<Account, Box<dyn Error>>;
    fn read_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn Error>>;

    // Account Overview
    fn new_account_overview(
        &mut self,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>>;

    fn update_account_overview(
        &mut self,
        account: &Account,
        currency: &Currency,
        total_available: Decimal,
        total_balance: Decimal,
    ) -> Result<AccountOverview, Box<dyn Error>>;

    fn read_account_overview(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountOverview>, Box<dyn Error>>;

    fn read_account_overview_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>>;

    // Prices
    fn new_price(&mut self, currency: Currency, amount: Decimal) -> Result<Price, Box<dyn Error>>;

    fn read_price(&mut self, id: Uuid) -> Result<Price, Box<dyn Error>>;

    // Transaction
    fn new_transaction(
        &mut self,
        account: &Account,
        amount: Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>>;
}
