use rust_decimal::Decimal;
use trust_model::{
    Account, AccountOverview, Currency, Database, Order, Rule, RuleLevel, RuleName, TradeCategory,
    TradingVehicle, TradingVehicleCategory, Transaction, TransactionCategory,
};
use uuid::Uuid;
use workers::{OrderWorker, QuantityWorker, RuleWorker, TransactionWorker};

pub struct Trust {
    database: Box<dyn Database>,
}

/// Trust is the main entry point for interacting with the trust-core library.
/// It is a facade that provides a simple interface for interacting with the
/// trust-core library.
impl Trust {
    /// Creates a new instance of Trust.
    pub fn new(database: Box<dyn Database>) -> Self {
        Trust { database }
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

    pub fn search_all_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        self.database.read_all_rules(account_id)
    }

    pub fn create_transaction(
        &mut self,
        account: &Account,
        category: &TransactionCategory,
        amount: Decimal,
        currency: &Currency,
    ) -> Result<(Transaction, AccountOverview), Box<dyn std::error::Error>> {
        TransactionWorker::create(&mut *self.database, category, amount, currency, account.id)
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

    pub fn create_rule(
        &mut self,
        account: &Account,
        name: &RuleName,
        description: &str,
        level: &RuleLevel,
    ) -> Result<Rule, Box<dyn std::error::Error>> {
        RuleWorker::create_rule(&mut *self.database, account, name, description, level)
    }

    pub fn make_rule_inactive(&mut self, rule: &Rule) -> Result<Rule, Box<dyn std::error::Error>> {
        self.database.make_rule_inactive(rule)
    }

    pub fn read_all_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        self.database.read_all_rules(account_id)
    }

    pub fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: &str,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn std::error::Error>> {
        self.database
            .create_trading_vehicle(symbol, isin, category, broker)
    }

    pub fn read_all_trading_vehicles(
        &mut self,
    ) -> Result<Vec<TradingVehicle>, Box<dyn std::error::Error>> {
        self.database.read_all_trading_vehicles()
    }

    pub fn maximum_quantity(
        &mut self,
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        currency: &Currency,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        QuantityWorker::maximum_quantity(
            account_id,
            entry_price,
            stop_price,
            currency,
            &mut *self.database,
        )
    }

    pub fn create_stop(
        &mut self,
        trading_vehicle_id: Uuid,
        quantity: i64,
        price: Decimal,
        category: &TradeCategory,
        currency: &Currency,
    ) -> Result<Order, Box<dyn std::error::Error>> {
        OrderWorker::create_stop(
            trading_vehicle_id,
            quantity,
            price,
            currency,
            category,
            &mut *self.database,
        )
    }

    pub fn create_entry(
        &mut self,
        trading_vehicle_id: Uuid,
        quantity: i64,
        price: Decimal,
        category: &TradeCategory,
        currency: &Currency,
    ) -> Result<Order, Box<dyn std::error::Error>> {
        OrderWorker::create_entry(
            trading_vehicle_id,
            quantity,
            price,
            currency,
            category,
            &mut *self.database,
        )
    }
}

mod validators;
mod workers;
