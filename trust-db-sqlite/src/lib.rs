use rust_decimal::Decimal;
use trust_model::Currency;
use trust_model::Database;
use trust_model::Price;
use trust_model::Strategy;
use trust_model::{Account, AccountOverview};
use trust_model::{TradingVehicle, TradingVehicleCategory};
use trust_model::{Transaction, TransactionCategory};

struct SqliteDatabase {
    // ...
}

impl Database for SqliteDatabase {
    // Accounts
    fn create_account(&mut self, name: &str, description: &str) -> Account {
        unimplemented!()
    }
    fn read_account(&self, name: &str) -> Account {
        unimplemented!()
    }
    fn read_account_overview(&mut self, account: Account) -> Vec<AccountOverview> {
        unimplemented!()
    }
    fn read_all_accounts(&mut self) -> Vec<Account> {
        unimplemented!()
    }

    // Transactions
    fn read_all_transactions(&mut self, account: Account) -> Vec<Transaction> {
        unimplemented!()
    }
    fn create_transaction(
        &mut self,
        account: &Account,
        amount: &str,
        currency: &str,
        category: TransactionCategory,
    ) -> Transaction {
        unimplemented!()
    }

    // Prices
    fn create_price(&mut self, amount: Decimal, currency: Currency) -> Price {
        unimplemented!()
    }

    // Transaction Vehicles
    fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: &str,
        category: TradingVehicleCategory,
        broker: &str,
    ) -> TradingVehicle {
        unimplemented!()
    }
    fn read_all_trading_vehicles(&mut self) -> Vec<TradingVehicle> {
        unimplemented!()
    }
    fn read_trading_vehicle(&mut self, symbol: &str) -> TradingVehicle {
        unimplemented!()
    }

    // Strategy
    fn create_strategy(
        &mut self,
        name: &str,
        description: &str,
        version: u16,
        entry_description: &str,
        stop_description: &str,
        target_description: &str,
    ) -> Strategy {
        unimplemented!()
    }
    fn read_strategy(&mut self, name: &str, version: u16) -> Strategy {
        unimplemented!()
    }
    fn read_all_strategies(&mut self) -> Vec<Strategy> {
        unimplemented!()
    }
}
