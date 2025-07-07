//! Trust Model Crate - Core Domain Models
//!
//! This crate defines the core domain models for the Trust financial trading application.
//! All types and traits here enforce strict financial safety standards.

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

pub mod account;
pub mod broker;
pub mod currency;
pub mod database;
pub mod order;
pub mod rule;
pub mod strategy;
pub mod trade;
pub mod trading_vehicle;
pub mod transaction;

// Re-export the types from the model crate.
pub use account::{Account, AccountBalance, Environment};
pub use broker::{Broker, BrokerLog, OrderIds};
pub use currency::Currency;
pub use database::{
    AccountBalanceRead, AccountBalanceWrite, AccountRead, AccountWrite, DatabaseFactory,
    DraftTrade, OrderRead, OrderWrite, ReadBrokerLogsDB, ReadRuleDB, ReadTradeDB,
    ReadTradingVehicleDB, ReadTransactionDB, WriteBrokerLogsDB, WriteRuleDB, WriteTradeDB,
    WriteTradingVehicleDB, WriteTransactionDB,
};
pub use order::{Order, OrderAction, OrderCategory, OrderStatus, TimeInForce};
pub use rule::{Rule, RuleLevel, RuleName};
pub use strategy::Strategy;
pub use trade::{Status, Trade, TradeBalance, TradeCategory};
pub use trading_vehicle::{TradingVehicle, TradingVehicleCategory};
pub use transaction::{Transaction, TransactionCategory};
