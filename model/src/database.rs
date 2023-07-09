use crate::{
    Account, AccountOverview, BrokerLog, Currency, Environment, Order, OrderAction, OrderCategory,
    Rule, RuleLevel, RuleName, Status, Trade, TradeCategory, TradeOverview, TradingVehicle,
    TradingVehicleCategory, Transaction, TransactionCategory,
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
    fn account_read(&self) -> Box<dyn AccountRead>;
    fn account_write(&self) -> Box<dyn AccountWrite>;
    fn account_overview_read(&self) -> Box<dyn AccountOverviewRead>;
    fn account_overview_write(&self) -> Box<dyn AccountOverviewWrite>;
    fn order_read(&self) -> Box<dyn OrderRead>;
    fn order_write(&self) -> Box<dyn OrderWrite>;
    fn transaction_read(&self) -> Box<dyn ReadTransactionDB>;
    fn transaction_write(&self) -> Box<dyn WriteTransactionDB>;
    fn trade_read(&self) -> Box<dyn ReadTradeDB>;
    fn trade_write(&self) -> Box<dyn WriteTradeDB>;
    fn trade_overview_write(&self) -> Box<dyn WriteTradeOverviewDB>;
    fn rule_read(&self) -> Box<dyn ReadRuleDB>;
    fn rule_write(&self) -> Box<dyn WriteRuleDB>;
    fn trading_vehicle_read(&self) -> Box<dyn ReadTradingVehicleDB>;
    fn trading_vehicle_write(&self) -> Box<dyn WriteTradingVehicleDB>;
    fn log_read(&self) -> Box<dyn ReadBrokerLogsDB>;
    fn log_write(&self) -> Box<dyn WriteBrokerLogsDB>;
}
 // TODO: Rename
pub trait AccountRead {
    fn for_name(&mut self, name: &str) -> Result<Account, Box<dyn Error>>;
    fn id(&mut self, id: Uuid) -> Result<Account, Box<dyn Error>>;
    fn all(&mut self) -> Result<Vec<Account>, Box<dyn Error>>;
}

pub trait AccountWrite {
    fn create(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
    ) -> Result<Account, Box<dyn Error>>;
}

pub trait AccountOverviewRead {
    fn for_account(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountOverview>, Box<dyn Error>>;

    fn for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>>;
}

pub trait AccountOverviewWrite {
    fn create(
        &mut self,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>>;

    fn update(
        &mut self,
        overview: &AccountOverview,
        balance: Decimal,
        in_trade: Decimal,
        available: Decimal,
        taxed: Decimal,
    ) -> Result<AccountOverview, Box<dyn Error>>;
}

pub trait OrderRead {
    fn for_id(&mut self, id: Uuid) -> Result<Order, Box<dyn Error>>;
}

pub trait OrderWrite {
    fn create(
        &mut self,
        trading_vehicle: &TradingVehicle,
        quantity: i64,
        price: Decimal,
        currency: &Currency,
        action: &OrderAction,
        category: &OrderCategory,
    ) -> Result<Order, Box<dyn Error>>;
    fn submit_of(
        &mut self,
        order: &Order,
        broker_order_id: Uuid,
    ) -> Result<Order, Box<dyn Error>>;
    fn filling_of(&mut self, order: &Order) -> Result<Order, Box<dyn Error>>;
    fn closing_of(&mut self, order: &Order) -> Result<Order, Box<dyn Error>>;
    fn update(&mut self, order: &Order) -> Result<Order, Box<dyn Error>>;
}

pub trait ReadTransactionDB {
    fn all_account_transactions_excluding_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    fn all_account_transactions_funding_in_submitted_trades(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    fn read_all_account_transactions_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    fn all_trade_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    fn all_trade_funding_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    fn all_trade_taxes_transactions(
        &mut self,
        trade_id: Uuid,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    fn all_transaction_excluding_current_month_and_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    fn all_transactions(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;
}

pub trait WriteTransactionDB {
    fn create_transaction(
        &mut self,
        account: &Account,
        amount: Decimal,
        currency: &Currency,
        category: TransactionCategory,
    ) -> Result<Transaction, Box<dyn Error>>;
}

// Trade DB

pub trait ReadTradeDB {
    fn all_open_trades_for_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Trade>, Box<dyn Error>>;

    fn read_trades_with_status(
        &mut self,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn Error>>;

    fn read_trade(&mut self, id: Uuid) -> Result<Trade, Box<dyn Error>>;
}

pub struct DraftTrade {
    pub account: Account,
    pub trading_vehicle: TradingVehicle,
    pub quantity: i64,
    pub currency: Currency,
    pub category: TradeCategory,
}

pub trait WriteTradeDB {
    fn create_trade(
        &mut self,
        draft: DraftTrade,
        stop: &Order,
        entry: &Order,
        target: &Order,
    ) -> Result<Trade, Box<dyn Error>>;

    fn update_trade_status(
        &mut self,
        status: Status,
        trade: &Trade,
    ) -> Result<Trade, Box<dyn Error>>;
}

pub trait WriteTradeOverviewDB {
    fn update_trade_overview(
        &mut self,
        trade: &Trade,
        funding: Decimal,
        capital_in_market: Decimal,
        capital_out_market: Decimal,
        taxed: Decimal,
        total_performance: Decimal,
    ) -> Result<TradeOverview, Box<dyn Error>>;
}

// Rule DB
pub trait WriteRuleDB {
    fn create_rule(
        &mut self,
        account: &Account,
        name: &RuleName,
        description: &str,
        priority: u32,
        level: &RuleLevel,
    ) -> Result<Rule, Box<dyn Error>>;

    fn make_rule_inactive(&mut self, rule: &Rule) -> Result<Rule, Box<dyn Error>>;
}

pub trait ReadRuleDB {
    fn read_all_rules(&mut self, account_id: Uuid) -> Result<Vec<Rule>, Box<dyn Error>>;
    fn rule_for_account(
        &mut self,
        account_id: Uuid,
        name: &RuleName,
    ) -> Result<Rule, Box<dyn Error>>;
}

// Trading Vehicle DB
pub trait ReadTradingVehicleDB {
    fn read_all_trading_vehicles(&mut self) -> Result<Vec<TradingVehicle>, Box<dyn Error>>;
    fn read_trading_vehicle(&mut self, id: Uuid) -> Result<TradingVehicle, Box<dyn Error>>;
}

pub trait WriteTradingVehicleDB {
    fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: &str,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn Error>>;
}

pub trait WriteBrokerLogsDB {
    fn create_log(&mut self, log: &str, trade: &Trade) -> Result<BrokerLog, Box<dyn Error>>;
}

pub trait ReadBrokerLogsDB {
    fn read_all_logs_for_trade(&mut self, trade_id: Uuid)
        -> Result<Vec<BrokerLog>, Box<dyn Error>>;
}
