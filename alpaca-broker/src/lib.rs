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
    Account, BarTimeframe, Broker, BrokerKind, BrokerLog, Environment, MarketBar,
    MarketDataChannel, MarketDataStreamEvent, MarketQuote, MarketTradeTick, Order, OrderIds,
    Status, Trade,
};
use std::error::Error;

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

fn ensure_trade_account(trade: &Trade, account: &Account) -> Result<(), Box<dyn Error>> {
    if trade.account_id != account.id {
        return Err("Trade account does not match broker account".into());
    }
    Ok(())
}

/// Generic Broker API
impl Broker for AlpacaBroker {
    fn kind(&self) -> BrokerKind {
        BrokerKind::Alpaca
    }

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
    ) -> Result<String, Box<dyn Error>> {
        modify_stop::modify(trade, account, new_stop_price)
    }

    fn modify_target(
        &self,
        trade: &Trade,
        account: &Account,
        new_target_price: rust_decimal::Decimal,
    ) -> Result<String, Box<dyn Error>> {
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
    use chrono::{TimeZone, Utc};
    use model::{Account, BarTimeframe, Broker, MarketDataChannel, Trade};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn mismatched_trade() -> (Account, Trade) {
        let account = Account::default();
        let trade = Trade {
            account_id: Uuid::new_v4(),
            ..Trade::default()
        };
        (account, trade)
    }

    fn assert_account_mismatch(error: Box<dyn std::error::Error>) {
        assert!(error
            .to_string()
            .contains("Trade account does not match broker account"));
    }

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

    #[test]
    fn broker_close_trade_returns_error_when_account_mismatches() {
        let broker = AlpacaBroker;
        let (account, trade) = mismatched_trade();

        let err = broker
            .close_trade(&trade, &account)
            .expect_err("account mismatch should fail before I/O");

        assert_account_mismatch(err);
    }

    #[test]
    fn broker_submit_trade_returns_error_when_account_mismatches() {
        let broker = AlpacaBroker;
        let (account, trade) = mismatched_trade();

        let err = broker
            .submit_trade(&trade, &account)
            .expect_err("account mismatch should fail before I/O");

        assert_account_mismatch(err);
    }

    #[test]
    fn broker_sync_trade_returns_error_when_account_mismatches() {
        let broker = AlpacaBroker;
        let (account, trade) = mismatched_trade();

        let err = broker
            .sync_trade(&trade, &account)
            .expect_err("account mismatch should fail before I/O");

        assert_account_mismatch(err);
    }

    #[test]
    fn broker_fetch_executions_returns_error_when_account_mismatches() {
        let broker = AlpacaBroker;
        let (account, trade) = mismatched_trade();

        let err = broker
            .fetch_executions(&trade, &account, None)
            .expect_err("account mismatch should fail before I/O");

        assert_account_mismatch(err);
    }

    #[test]
    fn broker_fetch_fee_activities_returns_error_when_account_mismatches() {
        let broker = AlpacaBroker;
        let (account, trade) = mismatched_trade();

        let err = broker
            .fetch_fee_activities(&trade, &account, None)
            .expect_err("account mismatch should fail before I/O");

        assert_account_mismatch(err);
    }

    #[test]
    fn broker_stream_market_data_validates_request_before_credentials() {
        let broker = AlpacaBroker;
        let account = Account::default();
        let channels = vec![MarketDataChannel::Quotes];

        let err = broker
            .stream_market_data(&[], &channels, 1, 1, &account)
            .expect_err("empty symbols should fail before I/O");

        assert_eq!(
            err.to_string(),
            "At least one symbol is required for streaming"
        );
    }

    #[test]
    fn broker_market_data_methods_validate_request_before_credentials() {
        let broker = AlpacaBroker;
        let account = Account::default();
        let start = Utc.with_ymd_and_hms(2026, 5, 7, 13, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 5, 7, 14, 0, 0).unwrap();

        let bars_err = broker
            .get_bars(" \t", start, end, BarTimeframe::OneMinute, &account)
            .expect_err("blank symbol should fail before I/O");
        assert_eq!(bars_err.to_string(), "Symbol cannot be empty");

        let window_err = broker
            .get_bars("AAPL", end, start, BarTimeframe::OneMinute, &account)
            .expect_err("invalid time window should fail before I/O");
        assert_eq!(
            window_err.to_string(),
            "Bar end time must be after start time"
        );

        let quote_err = broker
            .get_latest_quote("", &account)
            .expect_err("blank quote symbol should fail before I/O");
        assert_eq!(quote_err.to_string(), "Symbol cannot be empty");

        let trade_err = broker
            .get_latest_trade("\n", &account)
            .expect_err("blank trade symbol should fail before I/O");
        assert_eq!(trade_err.to_string(), "Symbol cannot be empty");
    }

    #[test]
    fn broker_fetch_asset_metadata_validates_symbol_before_credentials() {
        let account = Account::default();

        let err = AlpacaBroker::fetch_asset_metadata(&account, " ")
            .expect_err("blank symbol should fail before I/O");

        assert_eq!(err.to_string(), "Symbol cannot be empty");
    }
}
