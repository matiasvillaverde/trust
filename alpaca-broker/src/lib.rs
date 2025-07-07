//! Trust Alpaca Broker Implementation
//!
//! This crate provides the Alpaca broker API integration for the Trust
//! financial trading application.

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

use model::{Account, Broker, BrokerLog, Environment, Order, OrderIds, Status, Trade};
use std::error::Error;
use uuid::Uuid;

mod cancel_trade;
mod close_trade;
mod keys;
mod modify_stop;
mod modify_target;
mod order_mapper;
mod submit_trade;
mod sync_trade;
pub use keys::Keys;

#[derive(Default)]
/// Alpaca broker implementation
#[derive(Debug)]
pub struct AlpacaBroker;

/// Generic Broker API
impl Broker for AlpacaBroker {
    fn submit_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        submit_trade::submit_sync(trade, account)
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        sync_trade::sync(trade, account)
    }

    fn close_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        close_trade::close(trade, account)
    }

    fn cancel_trade(&self, trade: &Trade, account: &Account) -> Result<(), Box<dyn Error>> {
        println!("Canceling trade: {trade:?}");
        cancel_trade::cancel(trade, account)
    }

    fn modify_stop(
        &self,
        trade: &Trade,
        account: &Account,
        new_stop_price: rust_decimal::Decimal,
    ) -> Result<Uuid, Box<dyn Error>> {
        modify_stop::modify(trade, account, new_stop_price)
    }

    fn modify_target(
        &self,
        trade: &Trade,
        account: &Account,
        new_target_price: rust_decimal::Decimal,
    ) -> Result<Uuid, Box<dyn Error>> {
        modify_target::modify(trade, account, new_target_price)
    }
}

/// Alpaca-specific Broker API
impl AlpacaBroker {
    pub fn setup_keys(
        key_id: &str,
        secret: &str,
        url: &str,
        environment: &Environment,
        account: &Account,
    ) -> Result<Keys, Box<dyn Error>> {
        let keys = Keys::new(key_id, secret, url);
        let keys = keys.store(environment, &account.name)?;
        Ok(keys)
    }

    pub fn read_keys(environment: &Environment, account: &Account) -> Result<Keys, Box<dyn Error>> {
        let keys = Keys::read(environment, &account.name)?;
        Ok(keys)
    }

    pub fn delete_keys(environment: &Environment, account: &Account) -> Result<(), Box<dyn Error>> {
        Keys::delete(environment, &account.name)?;
        Ok(())
    }
}
