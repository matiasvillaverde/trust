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

/// Account management types and functionality
pub mod account;
/// Broker integration traits and types
pub mod broker;
/// Currency definitions and operations
pub mod currency;
/// Database abstraction layer
pub mod database;
/// Execution (fill) primitives for execution-level accounting and auditability
pub mod execution;
/// Trading level management and risk multipliers
pub mod level;
/// Market data types
pub mod market_data;
/// Distribution rules and profit allocation
pub mod distribution;
/// Order types and order management
pub mod order;
/// Risk management rules and enforcement
pub mod rule;
/// Trading strategy definitions
pub mod strategy;
/// Trade lifecycle and management
pub mod trade;
/// Trade grading types
pub mod trade_grade;
/// Trading vehicle (asset) definitions
pub mod trading_vehicle;
/// Transaction tracking and accounting
pub mod transaction;

// Re-export the types from the model crate.
pub use account::{Account, AccountBalance, AccountHierarchyError, AccountType, Environment};
pub use broker::{Broker, BrokerLog, OrderIds};
pub use currency::Currency;
pub use database::{
    AccountBalanceRead, AccountBalanceWrite, AccountRead, AccountWrite, DatabaseFactory,
    DraftTrade, OrderRead, OrderWrite, ReadBrokerLogsDB, ReadExecutionDB, ReadLevelDB, ReadRuleDB,
    ReadTradeDB, ReadTradeGradeDB, ReadTradingVehicleDB, ReadTransactionDB, WriteBrokerLogsDB,
    WriteExecutionDB, WriteLevelDB, WriteRuleDB, WriteTradeDB, WriteTradeGradeDB,
    WriteTradingVehicleDB, WriteTransactionDB,
    DistributionRead, DistributionWrite,
};
pub use execution::{
    Execution, ExecutionSide, ExecutionSideParseError, ExecutionSource, ExecutionSourceParseError,
};
pub use level::{
    Level, LevelAdjustmentRules, LevelChange, LevelDirection, LevelError, LevelRulesError,
    LevelStatus, LevelStatusParseError, LevelTrigger, LevelTriggerParseError,
};
pub use market_data::{BarTimeframe, MarketBar};
pub use distribution::{
    DistributionError, DistributionExecutionLeg, DistributionExecutionPlan, DistributionHistory,
    DistributionResult, DistributionRules, DistributionRulesNotFound,
};
pub use order::{Order, OrderAction, OrderCategory, OrderStatus, TimeInForce};
pub use rule::{Rule, RuleLevel, RuleName};
pub use strategy::Strategy;
pub use trade::{Status, Trade, TradeBalance, TradeCategory};
pub use trade_grade::{Grade, TradeGrade};
pub use trading_vehicle::{TradingVehicle, TradingVehicleCategory};
pub use transaction::{Transaction, TransactionCategory};
