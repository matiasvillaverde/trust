use crate::Currency;
use crate::OrderAction;
use crate::Price;
use crate::Target;
use crate::TradingVehicle;
use crate::TradingVehicleCategory;
use crate::Transaction;
use crate::TransactionCategory;
use crate::{
    Account, AccountOverview, Order, Rule, RuleLevel, RuleName, Trade, TradeCategory, TradeOverview,
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
    fn read_account_db(&self) -> Box<dyn ReadAccountDB>;
    fn write_account_db(&self) -> Box<dyn WriteAccountDB>;
    fn read_account_overview_db(&self) -> Box<dyn ReadAccountOverviewDB>;
    fn write_account_overview_db(&self) -> Box<dyn WriteAccountOverviewDB>;
    fn read_order_db(&self) -> Box<dyn ReadOrderDB>;
    fn write_order_db(&self) -> Box<dyn WriteOrderDB>;
    fn read_price_db(&self) -> Box<dyn ReadPriceDB>;
    fn write_price_db(&self) -> Box<dyn WritePriceDB>;
    fn read_transaction_db(&self) -> Box<dyn ReadTransactionDB>;
    fn write_transaction_db(&self) -> Box<dyn WriteTransactionDB>;
    fn read_trade_db(&self) -> Box<dyn ReadTradeDB>;
    fn write_trade_db(&self) -> Box<dyn WriteTradeDB>;
    fn write_trade_overview_db(&self) -> Box<dyn WriteTradeOverviewDB>;
    fn read_rule_db(&self) -> Box<dyn ReadRuleDB>;
    fn write_rule_db(&self) -> Box<dyn WriteRuleDB>;
    fn read_trading_vehicle_db(&self) -> Box<dyn ReadTradingVehicleDB>;
    fn write_trading_vehicle_db(&self) -> Box<dyn WriteTradingVehicleDB>;
}

pub trait ReadAccountDB {
    fn read_account(&mut self, name: &str) -> Result<Account, Box<dyn Error>>;
    fn read_account_id(&mut self, id: Uuid) -> Result<Account, Box<dyn Error>>;
    fn read_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn Error>>;
}

pub trait WriteAccountDB {
    fn new_account(&mut self, name: &str, description: &str) -> Result<Account, Box<dyn Error>>;
}

pub trait ReadAccountOverviewDB {
    fn read_account_overview(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountOverview>, Box<dyn Error>>;

    fn read_account_overview_currency(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>>;
}

pub trait WriteAccountOverviewDB {
    fn new_account_overview(
        &mut self,
        account: &Account,
        currency: &Currency,
    ) -> Result<AccountOverview, Box<dyn Error>>;

    fn update_account_overview(
        &mut self,
        account: &Account,
        currency: &Currency,
        total_balance: Decimal,
        total_in_trade: Decimal,
        total_available: Decimal,
        taxed: Decimal,
    ) -> Result<AccountOverview, Box<dyn Error>>;
}

pub trait ReadOrderDB {
    fn read_order(&mut self, id: Uuid) -> Result<Order, Box<dyn Error>>;
}

pub trait WriteOrderDB {
    fn create_order(
        &mut self,
        trading_vehicle: &TradingVehicle,
        quantity: i64,
        price: Decimal,
        currency: &Currency,
        action: &OrderAction,
    ) -> Result<Order, Box<dyn Error>>;

    fn create_target(
        &mut self,
        price: Decimal,
        currency: &Currency,
        order: &Order,
        trade: &Trade,
    ) -> Result<Target, Box<dyn Error>>;

    fn record_order_opening(&mut self, order: &Order) -> Result<Order, Box<dyn Error>>;
    fn record_order_closing(&mut self, order: &Order) -> Result<Order, Box<dyn Error>>;
}

pub trait WritePriceDB {
    fn new_price(&mut self, currency: &Currency, amount: Decimal) -> Result<Price, Box<dyn Error>>;
}

pub trait ReadPriceDB {
    fn read_price(&mut self, id: Uuid) -> Result<Price, Box<dyn Error>>;
}

pub trait ReadTransactionDB {
    fn all_account_transactions_excluding_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    fn all_account_transactions_funding_in_open_trades(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    fn read_all_account_transactions_taxes(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    fn all_trade_transactions(&mut self, trade: &Trade)
        -> Result<Vec<Transaction>, Box<dyn Error>>;

    fn all_trade_funding_transactions(
        &mut self,
        trade: &Trade,
    ) -> Result<Vec<Transaction>, Box<dyn Error>>;

    fn all_trade_taxes_transactions(
        &mut self,
        trade: &Trade,
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
    fn new_transaction(
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

    fn all_approved_trades(&mut self, account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>>;

    fn all_open_trades(&mut self, account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>>;

    fn read_trade(&mut self, id: Uuid) -> Result<Trade, Box<dyn Error>>;

    fn read_all_new_trades(&mut self, account_id: Uuid) -> Result<Vec<Trade>, Box<dyn Error>>;
}

pub trait WriteTradeDB {
    fn create_trade(
        &mut self,
        category: &TradeCategory,
        currency: &Currency,
        trading_vehicle: &TradingVehicle,
        safety_stop: &Order,
        entry: &Order,
        account: &Account,
    ) -> Result<Trade, Box<dyn Error>>;

    fn approve_trade(&mut self, trade: &Trade) -> Result<Trade, Box<dyn Error>>;
    fn update_trade_opened_at(&mut self, trade: &Trade) -> Result<Trade, Box<dyn Error>>;
    fn update_trade_closed_at(&mut self, trade: &Trade) -> Result<Trade, Box<dyn Error>>;
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
