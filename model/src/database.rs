use crate::{
    Account, AccountBalance, BrokerLog, Currency, Environment, Level, LevelChange, Order,
    OrderAction, OrderCategory, Rule, RuleLevel, RuleName, Status, Trade, TradeBalance,
    TradeCategory, TradingVehicle, TradingVehicleCategory, Transaction, TransactionCategory,
};
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
pub trait DatabaseFactory {
    /// Returns a reader for account data operations
    fn account_read(&self) -> Box<dyn AccountRead>;
    /// Returns a writer for account data operations
    fn account_write(&self) -> Box<dyn AccountWrite>;
    /// Returns a reader for account balance data operations
    fn account_balance_read(&self) -> Box<dyn AccountBalanceRead>;
    /// Returns a writer for account balance data operations
    fn account_balance_write(&self) -> Box<dyn AccountBalanceWrite>;
    /// Returns a reader for order data operations
    fn order_read(&self) -> Box<dyn OrderRead>;
    /// Returns a writer for order data operations
    fn order_write(&self) -> Box<dyn OrderWrite>;
    /// Returns a reader for transaction data operations
    fn transaction_read(&self) -> Box<dyn ReadTransactionDB>;
    /// Returns a writer for transaction data operations
    fn transaction_write(&self) -> Box<dyn WriteTransactionDB>;
    /// Returns a reader for trade data operations
    fn trade_read(&self) -> Box<dyn ReadTradeDB>;
    /// Returns a writer for trade data operations
    fn trade_write(&self) -> Box<dyn WriteTradeDB>;
    /// Returns a writer for trade balance data operations
    fn trade_balance_write(&self) -> Box<dyn WriteAccountBalanceDB>;
    /// Returns a reader for rule data operations
    fn rule_read(&self) -> Box<dyn ReadRuleDB>;
    /// Returns a writer for rule data operations
    fn rule_write(&self) -> Box<dyn WriteRuleDB>;
    /// Returns a reader for trading vehicle data operations
    fn trading_vehicle_read(&self) -> Box<dyn ReadTradingVehicleDB>;
    /// Returns a writer for trading vehicle data operations
    fn trading_vehicle_write(&self) -> Box<dyn WriteTradingVehicleDB>;
    /// Returns a reader for broker log data operations
    fn log_read(&self) -> Box<dyn ReadBrokerLogsDB>;
    /// Returns a writer for broker log data operations
    fn log_write(&self) -> Box<dyn WriteBrokerLogsDB>;
    /// Returns a reader for level data operations
    fn level_read(&self) -> Box<dyn ReadLevelDB>;
    /// Returns a writer for level data operations
    fn level_write(&self) -> Box<dyn WriteLevelDB>;
}

/// Trait for reading account data from the database
pub trait AccountRead {
    /// Retrieves an account by its name
    fn for_name(&mut self, name: &str) -> Result<Account, Box<dyn Error>>;
    /// Retrieves an account by its ID
    fn id(&mut self, id: Uuid) -> Result<Account, Box<dyn Error>>;
    /// Retrieves all accounts from the database
    fn all(&mut self) -> Result<Vec<Account>, Box<dyn Error>>;
}

/// Trait for writing account data to the database
pub trait AccountWrite {
    /// Creates a new account with the specified parameters
    fn create(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
    ) -> Result<Account, Box<dyn Error>>;
}

/// Trait for reading account balance data from the database
pub trait AccountBalanceRead {
    /// Retrieves all account balances for a specific account
    fn for_account(&mut self, account_id: Uuid) -> Result<Vec<AccountBalance>, Box<dyn Error>>;

    /// Retrieves the account balance for a specific currency
    fn for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountBalance, Box<dyn Error>>;
}

/// Trait for writing account balance data to the database
pub trait AccountBalanceWrite {
    /// Creates a new account balance entry for the given account and currency
    fn create(
        &mut self,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountBalance, Box<dyn Error>>;

    /// Updates an existing account balance with new values
    fn update(
        &mut self,
        balance: &AccountBalance,
        balance: Decimal,
        in_trade: Decimal,
        available: Decimal,
        taxed: Decimal,
    ) -> Result<AccountBalance, Box<dyn Error>>;
}

/// Trait for reading order data from the database
pub trait OrderRead {
    /// Retrieves an order by its ID
    fn for_id(&mut self, id: Uuid) -> Result<Order, Box<dyn Error>>;
}

/// Trait for writing order data to the database
pub trait OrderWrite {
    /// Creates a new order with the specified parameters
    fn create(
        &mut self,
        trading_vehicle: &TradingVehicle,
        quantity: i64,
        price: Decimal,
        currency: &Currency,
        action: &OrderAction,
        category: &OrderCategory,
    ) -> Result<Order, Box<dyn Error>>;
    /// Marks an order as submitted with the broker's order ID
    fn submit_of(&mut self, order: &Order, broker_order_id: Uuid) -> Result<Order, Box<dyn Error>>;
    /// Marks an order as being filled
    fn filling_of(&mut self, order: &Order) -> Result<Order, Box<dyn Error>>;
    /// Marks an order as closed
    fn closing_of(&mut self, order: &Order) -> Result<Order, Box<dyn Error>>;
    /// Updates an existing order
    fn update(&mut self, order: &Order) -> Result<Order, Box<dyn Error>>;
    /// Updates the price of an order with the broker's ID
    fn update_price(
        &mut self,
        order: &Order,
        price: Decimal,
        broker_id: Uuid,
    ) -> Result<Order, Box<dyn Error>>;
}

/// Trait for reading transaction data from the database
pub trait ReadTransactionDB {
    /// Retrieves all account transactions excluding tax transactions
    fn all_account_transactions_excluding_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all account transactions that are funding submitted trades
    fn all_account_transactions_funding_in_submitted_trades(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all tax-related transactions for an account
    fn read_all_account_transactions_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all transactions associated with a specific trade
    fn all_trade_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all funding transactions for a specific trade
    fn all_trade_funding_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all tax transactions for a specific trade
    fn all_trade_taxes_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all transactions excluding current month and tax transactions
    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    /// Retrieves all transactions for an account in a specific currency
    fn all_transactions(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;
}

/// Trait for writing transaction data to the database
pub trait WriteTransactionDB {
    /// Creates a new transaction with the specified parameters
    fn create_transaction(
        &mut self,
        account: &Account,
        amount: Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>>;
}

// Trade DB

/// Trait for reading trade data from the database
pub trait ReadTradeDB {
    /// Retrieves all open trades for a specific account and currency
    fn all_open_trades_for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>>;

    /// Retrieves all trades with a specific status for an account
    fn read_trades_with_status(
        &mut self,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn Error>>;

    /// Retrieves a specific trade by its ID
    fn read_trade(&mut self, id: Uuid) -> Result<Trade, Box<dyn Error>>;
}

/// Structure representing a draft trade before it's created in the database
#[derive(Debug)]
pub struct DraftTrade {
    /// The account associated with the trade
    pub account: Account,
    /// The trading vehicle (e.g., stock, option) for the trade
    pub trading_vehicle: TradingVehicle,
    /// The quantity of the trading vehicle
    pub quantity: i64,
    /// The currency used for the trade
    pub currency: Currency,
    /// The category of the trade
    pub category: TradeCategory,
    /// Trade thesis - reasoning behind the trade (max 200 chars)
    pub thesis: Option<String>,
    /// Market sector (e.g., technology, healthcare, finance)
    pub sector: Option<String>,
    /// Asset class (e.g., stocks, options, futures, crypto)
    pub asset_class: Option<String>,
    /// Trading context (e.g., Elliott Wave count, S/R levels, indicators)
    pub context: Option<String>,
}

/// Trait for writing trade data to the database
pub trait WriteTradeDB {
    /// Creates a new trade with the specified draft and orders
    fn create_trade(
        &mut self,
        draft: DraftTrade,
        stop: &Order,
        entry: &Order,
        target: &Order,
    ) -> Result<Trade, Box<dyn Error>>;

    /// Updates the status of an existing trade
    fn update_trade_status(
        &mut self,
        status: Status,
        trade: &Trade,
    ) -> Result<Trade, Box<dyn Error>>;
}

/// Trait for writing trade balance data to the database
pub trait WriteAccountBalanceDB {
    /// Updates the trade balance with performance metrics
    fn update_trade_balance(
        &mut self,
        trade: &Trade,
        funding: Decimal,
        capital_in_market: Decimal,
        capital_out_market: Decimal,
        taxed: Decimal,
        total_performance: Decimal,
    ) -> Result<TradeBalance, Box<dyn Error>>;
}

// Rule DB
/// Trait for writing rule data to the database
pub trait WriteRuleDB {
    /// Creates a new rule with the specified parameters
    fn create_rule(
        &mut self,
        account: &Account,
        name: &RuleName,
        description: &str,
        priority: u32,
        level: &RuleLevel,
    ) -> Result<Rule, Box<dyn Error>>;

    /// Marks a rule as inactive
    fn make_rule_inactive(&mut self, rule: &Rule) -> Result<Rule, Box<dyn Error>>;
}

/// Trait for reading rule data from the database
pub trait ReadRuleDB {
    /// Retrieves all rules for a specific account
    fn read_all_rules(&mut self, account_id: Uuid) -> Result<Vec<Rule>, Box<dyn Error>>;
    /// Retrieves a specific rule by account ID and rule name
    fn rule_for_account(
        &mut self,
        account_id: Uuid,
        name: &RuleName,
    ) -> Result<Rule, Box<dyn Error>>;
}

// Trading Vehicle DB
/// Trait for reading trading vehicle data from the database
pub trait ReadTradingVehicleDB {
    /// Retrieves all trading vehicles from the database
    fn read_all_trading_vehicles(&mut self) -> Result<Vec<TradingVehicle>, Box<dyn Error>>;
    /// Retrieves a specific trading vehicle by its ID
    fn read_trading_vehicle(&mut self, id: Uuid) -> Result<TradingVehicle, Box<dyn Error>>;
}

/// Trait for writing trading vehicle data to the database
pub trait WriteTradingVehicleDB {
    /// Creates a new trading vehicle with the specified parameters
    fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: &str,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn Error>>;
}

/// Trait for writing broker log data to the database
pub trait WriteBrokerLogsDB {
    /// Creates a new log entry for a trade
    fn create_log(&mut self, log: &str, trade: &Trade) -> Result<BrokerLog, Box<dyn Error>>;
}

/// Trait for reading broker log data from the database
pub trait ReadBrokerLogsDB {
    /// Retrieves all logs associated with a specific trade
    fn read_all_logs_for_trade(&mut self, trade_id: Uuid)
        -> Result<Vec<BrokerLog>, Box<dyn Error>>;
}

/// Trait for reading level data from the database
pub trait ReadLevelDB {
    /// Retrieves the current level for an account
    fn level_for_account(&mut self, account_id: Uuid) -> Result<Level, Box<dyn Error>>;
    /// Retrieves all level changes for an account
    fn level_changes_for_account(&mut self, account_id: Uuid) -> Result<Vec<LevelChange>, Box<dyn Error>>;
    /// Retrieves recent level changes for an account within specified days
    fn recent_level_changes(&mut self, account_id: Uuid, days: u32) -> Result<Vec<LevelChange>, Box<dyn Error>>;
}

/// Trait for writing level data to the database
pub trait WriteLevelDB {
    /// Creates an initial level for an account (Level 3 default)
    fn create_default_level(&mut self, account: &Account) -> Result<Level, Box<dyn Error>>;
    /// Updates an existing level
    fn update_level(&mut self, level: &Level) -> Result<Level, Box<dyn Error>>;
    /// Creates a level change record
    fn create_level_change(&mut self, level_change: &LevelChange) -> Result<LevelChange, Box<dyn Error>>;
}
