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

use model::{
    Account, BarTimeframe, Broker, BrokerLog, Environment, MarketBar, MarketDataChannel,
    MarketDataStreamEvent, MarketQuote, MarketTradeTick, Order, OrderIds, Status, Trade,
};
use std::error::Error;
use uuid::Uuid;

mod asset_lookup;
mod cancel_trade;
mod close_trade;
mod executions;
mod fees;
mod keys;
mod market_data;
mod modify_stop;
mod modify_target;
mod order_mapper;
mod submit_trade;
mod sync_trade;
pub use asset_lookup::AssetMetadata;
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

    fn get_bars(
        &self,
        symbol: &str,
        start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
        timeframe: BarTimeframe,
        account: &Account,
    ) -> Result<Vec<MarketBar>, Box<dyn Error>> {
        market_data::get_bars(symbol, start, end, timeframe, account)
    }

    fn get_latest_quote(
        &self,
        symbol: &str,
        account: &Account,
    ) -> Result<MarketQuote, Box<dyn Error>> {
        market_data::get_latest_quote(symbol, account)
    }

    fn get_latest_trade(
        &self,
        symbol: &str,
        account: &Account,
    ) -> Result<MarketTradeTick, Box<dyn Error>> {
        market_data::get_latest_trade(symbol, account)
    }

    fn stream_market_data(
        &self,
        symbols: &[String],
        channels: &[MarketDataChannel],
        max_events: usize,
        timeout_seconds: u64,
        account: &Account,
    ) -> Result<Vec<MarketDataStreamEvent>, Box<dyn Error>> {
        market_data::stream_market_data(symbols, channels, max_events, timeout_seconds, account)
    }

    fn fetch_executions(
        &self,
        trade: &Trade,
        account: &Account,
        after: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<model::Execution>, Box<dyn Error>> {
        executions::fetch_executions(trade, account, after)
    }

    fn fetch_fee_activities(
        &self,
        trade: &Trade,
        account: &Account,
        after: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<model::FeeActivity>, Box<dyn Error>> {
        fees::fetch_fee_activities(trade, account, after)
    }
}

/// Alpaca-specific Broker API
impl AlpacaBroker {
    /// Setup and store API keys for Alpaca broker
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

    /// Read API keys from keychain for Alpaca broker
    pub fn read_keys(environment: &Environment, account: &Account) -> Result<Keys, Box<dyn Error>> {
        let keys = Keys::read(environment, &account.name)?;
        Ok(keys)
    }

    /// Delete API keys from keychain for Alpaca broker
    pub fn delete_keys(environment: &Environment, account: &Account) -> Result<(), Box<dyn Error>> {
        Keys::delete(environment, &account.name)?;
        Ok(())
    }

    /// Read broker asset metadata for a specific symbol from Alpaca.
    pub fn fetch_asset_metadata(
        account: &Account,
        symbol: &str,
    ) -> Result<AssetMetadata, Box<dyn Error>> {
        asset_lookup::fetch_asset_metadata(account, symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::AlpacaBroker;
    use model::{Account, Broker, Trade};
    use rust_decimal_macros::dec;

    #[test]
    fn broker_cancel_trade_fails_fast_when_entry_order_id_is_missing() {
        let broker = AlpacaBroker;
        let account = Account::default();
        let trade = Trade {
            account_id: account.id,
            ..Trade::default()
        };

        let err = broker
            .cancel_trade(&trade, &account)
            .expect_err("missing entry order id should fail before I/O");
        assert!(err.to_string().contains("Entry order ID is missing"));
    }

    #[test]
    fn broker_modify_stop_fails_fast_when_stop_order_id_is_missing() {
        let broker = AlpacaBroker;
        let account = Account::default();
        let trade = Trade {
            account_id: account.id,
            ..Trade::default()
        };

        let err = broker
            .modify_stop(&trade, &account, dec!(90))
            .expect_err("missing stop order id should fail before I/O");
        assert!(err.to_string().contains("Safety stop order ID is missing"));
    }

    #[test]
    fn broker_modify_target_fails_fast_when_target_order_id_is_missing() {
        let broker = AlpacaBroker;
        let account = Account::default();
        let trade = Trade {
            account_id: account.id,
            ..Trade::default()
        };

        let err = broker
            .modify_target(&trade, &account, dec!(120))
            .expect_err("missing target order id should fail before I/O");
        assert!(err.to_string().contains("Target order ID is missing"));
    }
}
