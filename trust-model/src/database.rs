use crate::Currency;
use crate::Price;
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
pub trait Database {
    // Accounts
    fn create_account(&mut self, name: &str, description: &str) -> Result<Account, Box<dyn Error>>;
    fn read_account(&mut self, name: &str) -> Result<Account, Box<dyn Error>>;
    fn read_account_overview(
        &mut self,
        account: Account,
    ) -> Result<Vec<AccountOverview>, Box<dyn Error>>;
    fn read_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn Error>>;

    // Prices
    fn create_price(
        &mut self,
        currency: Currency,
        amount: Decimal,
    ) -> Result<Price, Box<dyn Error>>;

    fn read_price(&mut self, id: Uuid) -> Result<Price, Box<dyn Error>>;

    // // Strategy
    // fn create_strategy(
    //     &mut self,
    //     name: &str,
    //     description: &str,
    //     version: u16,
    //     entry_description: &str,
    //     stop_description: &str,
    //     target_description: &str,
    // ) -> Strategy;
    // fn read_strategy(&mut self, name: &str, version: u16) -> Strategy;
    // fn read_all_strategies(&mut self) -> Vec<Strategy>;
}
