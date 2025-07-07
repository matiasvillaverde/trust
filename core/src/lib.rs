//! Trust Core Crate - Business Logic and Risk Management
//!
//! This crate contains the core business logic, calculators, and validators
//! for the Trust financial trading application.

// === FINANCIAL APPLICATION SAFETY LINTS ===
// These lint rules are critical for financial applications where precision,
// safety, and reliability are paramount. Violations can lead to financial losses.

#![deny(
    // Error handling safety - force proper error handling
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,

    // Financial precision safety - prevent calculation errors
    clippy::float_arithmetic,
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,

    // Code quality enforcement
    clippy::cognitive_complexity,
    clippy::too_many_lines,
)]
// Allow unwrap and expect in test code only
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
// Standard Rust lints for code quality
#![warn(missing_docs, rust_2018_idioms, missing_debug_implementations)]

use calculators_trade::QuantityCalculator;
use model::{
    Account, AccountBalance, Broker, BrokerLog, Currency, DatabaseFactory, DraftTrade, Environment,
    Order, Rule, RuleLevel, RuleName, Status, Trade, TradeBalance, TradingVehicle,
    TradingVehicleCategory, Transaction, TransactionCategory,
};
use rust_decimal::Decimal;
use uuid::Uuid;

pub struct TrustFacade {
    factory: Box<dyn DatabaseFactory>,
    broker: Box<dyn Broker>,
}

/// Trust is the main entry point for interacting with the core library.
/// It is a facade that provides a simple interface for interacting with the
/// core library.
impl TrustFacade {
    /// Creates a new instance of Trust.
    pub fn new(factory: Box<dyn DatabaseFactory>, broker: Box<dyn Broker>) -> Self {
        TrustFacade { factory, broker }
    }

    /// Creates a new account.
    pub fn create_account(
        &mut self,
        name: &str,
        description: &str,
        environment: Environment,
        taxes_percentage: Decimal,
        earnings_percentage: Decimal,
    ) -> Result<Account, Box<dyn std::error::Error>> {
        self.factory.account_write().create(
            name,
            description,
            environment,
            taxes_percentage,
            earnings_percentage,
        )
    }

    pub fn search_account(&mut self, name: &str) -> Result<Account, Box<dyn std::error::Error>> {
        self.factory.account_read().for_name(name)
    }

    pub fn search_all_accounts(&mut self) -> Result<Vec<Account>, Box<dyn std::error::Error>> {
        self.factory.account_read().all()
    }

    pub fn search_all_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        self.factory.rule_read().read_all_rules(account_id)
    }

    pub fn create_transaction(
        &mut self,
        account: &Account,
        category: &TransactionCategory,
        amount: Decimal,
        currency: &Currency,
    ) -> Result<(Transaction, AccountBalance), Box<dyn std::error::Error>> {
        commands::transaction::create(&mut *self.factory, category, amount, currency, account.id)
    }

    pub fn search_balance(
        &mut self,
        account_id: Uuid,
        currency: &Currency,
    ) -> Result<AccountBalance, Box<dyn std::error::Error>> {
        self.factory
            .account_balance_read()
            .for_currency(account_id, currency)
    }

    pub fn search_all_balances(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<AccountBalance>, Box<dyn std::error::Error>> {
        self.factory.account_balance_read().for_account(account_id)
    }

    pub fn create_rule(
        &mut self,
        account: &Account,
        name: &RuleName,
        description: &str,
        level: &RuleLevel,
    ) -> Result<Rule, Box<dyn std::error::Error>> {
        commands::rule::create(&mut *self.factory, account, name, description, level)
    }

    pub fn deactivate_rule(&mut self, rule: &Rule) -> Result<Rule, Box<dyn std::error::Error>> {
        self.factory.rule_write().make_rule_inactive(rule)
    }

    pub fn search_rules(
        &mut self,
        account_id: Uuid,
    ) -> Result<Vec<Rule>, Box<dyn std::error::Error>> {
        self.factory.rule_read().read_all_rules(account_id)
    }

    pub fn create_trading_vehicle(
        &mut self,
        symbol: &str,
        isin: &str,
        category: &TradingVehicleCategory,
        broker: &str,
    ) -> Result<TradingVehicle, Box<dyn std::error::Error>> {
        self.factory
            .trading_vehicle_write()
            .create_trading_vehicle(symbol, isin, category, broker)
    }

    pub fn search_trading_vehicles(
        &mut self,
    ) -> Result<Vec<TradingVehicle>, Box<dyn std::error::Error>> {
        self.factory
            .trading_vehicle_read()
            .read_all_trading_vehicles()
    }

    pub fn calculate_maximum_quantity(
        &mut self,
        account_id: Uuid,
        entry_price: Decimal,
        stop_price: Decimal,
        currency: &Currency,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        QuantityCalculator::maximum_quantity(
            account_id,
            entry_price,
            stop_price,
            currency,
            &mut *self.factory,
        )
    }

    pub fn create_trade(
        &mut self,
        trade: DraftTrade,
        stop_price: Decimal,
        entry_price: Decimal,
        target_price: Decimal,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        commands::trade::create_trade(
            trade,
            stop_price,
            entry_price,
            target_price,
            &mut *self.factory,
        )
    }

    pub fn search_trades(
        &mut self,
        account_id: Uuid,
        status: Status,
    ) -> Result<Vec<Trade>, Box<dyn std::error::Error>> {
        self.factory
            .trade_read()
            .read_trades_with_status(account_id, status)
    }

    // Trade Steps

    pub fn fund_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(Trade, Transaction, AccountBalance, TradeBalance), Box<dyn std::error::Error>>
    {
        commands::trade::fund(trade, &mut *self.factory)
    }

    pub fn submit_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(Trade, BrokerLog), Box<dyn std::error::Error>> {
        commands::trade::submit(trade, &mut *self.factory, &mut *self.broker)
    }

    pub fn sync_trade(
        &mut self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn std::error::Error>> {
        commands::trade::sync_with_broker(trade, account, &mut *self.factory, &mut *self.broker)
    }

    pub fn fill_trade(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<(Trade, Transaction), Box<dyn std::error::Error>> {
        commands::trade::fill_trade(trade, fee, self.factory.as_mut())
    }

    pub fn stop_trade(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<(Transaction, Transaction, TradeBalance, AccountBalance), Box<dyn std::error::Error>>
    {
        commands::trade::stop_acquired(trade, fee, &mut *self.factory)
    }

    pub fn close_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeBalance, BrokerLog), Box<dyn std::error::Error>> {
        commands::trade::close(trade, &mut *self.factory, &mut *self.broker)
    }

    pub fn cancel_funded_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeBalance, AccountBalance, Transaction), Box<dyn std::error::Error>> {
        commands::trade::cancel_funded(trade, &mut *self.factory)
    }

    pub fn cancel_submitted_trade(
        &mut self,
        trade: &Trade,
    ) -> Result<(TradeBalance, AccountBalance, Transaction), Box<dyn std::error::Error>> {
        commands::trade::cancel_submitted(trade, &mut *self.factory, &mut *self.broker)
    }

    pub fn target_acquired(
        &mut self,
        trade: &Trade,
        fee: Decimal,
    ) -> Result<(Transaction, Transaction, TradeBalance, AccountBalance), Box<dyn std::error::Error>>
    {
        commands::trade::target_acquired(trade, fee, &mut *self.factory)
    }

    pub fn modify_stop(
        &mut self,
        trade: &Trade,
        account: &Account,
        new_stop_price: Decimal,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        commands::trade::modify_stop(
            trade,
            account,
            new_stop_price,
            &mut *self.broker,
            &mut *self.factory,
        )
    }

    pub fn modify_target(
        &mut self,
        trade: &Trade,
        account: &Account,
        new_target_price: Decimal,
    ) -> Result<Trade, Box<dyn std::error::Error>> {
        commands::trade::modify_target(
            trade,
            account,
            new_target_price,
            &mut *self.broker,
            &mut *self.factory,
        )
    }
}

mod calculators_account;
mod calculators_trade;
mod commands;
mod mocks;
mod validators;
