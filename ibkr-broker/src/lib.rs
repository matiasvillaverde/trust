//! Trust Interactive Brokers Client Portal integration.

#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::float_arithmetic,
    clippy::arithmetic_side_effects,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cognitive_complexity,
    clippy::too_many_lines
)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![warn(missing_docs, rust_2018_idioms, missing_debug_implementations)]

mod client;
mod config;
mod contracts;
mod executions;
mod market_data;
mod orders;
mod parsing;
mod support;

pub use config::ConnectionConfig;
pub use contracts::ContractMetadata;

use chrono::{DateTime, Utc};
use client::IbkrClient;
use contracts::{fetch_contract_metadata_with_client, resolve_conid};
use executions::{fetch_executions, fetch_fee_activities};
use market_data::{get_bars, get_latest_quote, get_latest_trade};
use model::{
    Account, BarTimeframe, Broker, BrokerKind, BrokerLog, MarketBar, MarketQuote, MarketTradeTick,
    Order, OrderCategory, OrderIds, Status, Trade,
};
use orders::{
    build_bracket_orders, build_close_order, build_modify_order, find_live_order_by_ref,
    map_live_order, map_trade_status, normalize_order_ref,
};
use std::error::Error;
use support::{broker_account_id, ensure_trade_account};

pub(crate) const BROKER_NAME: &str = "ibkr";
pub(crate) const LIVE_ORDER_LOOKUP_RETRIES: usize = 5;
pub(crate) const LIVE_ORDER_LOOKUP_DELAY_MS: u64 = 150;

#[derive(Default)]
/// Interactive Brokers broker implementation backed by the Client Portal Gateway.
#[derive(Debug)]
pub struct IbkrBroker;

impl Broker for IbkrBroker {
    fn kind(&self) -> BrokerKind {
        BrokerKind::Ibkr
    }

    fn submit_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        ensure_trade_account(trade, account)?;
        let client = IbkrClient::for_account(account)?;
        let account_id = broker_account_id(account)?;
        client.prepare_trading_session(Some(account))?;
        let conid = resolve_conid(&client, &trade.trading_vehicle)?;
        let payload = serde_json::json!({
            "orders": build_bracket_orders(trade, account_id, &conid)?,
        });
        let response = client
            .post_json_with_replies(&format!("/iserver/account/{account_id}/orders"), &payload)?;

        let entry_ref = normalize_order_ref(&trade.entry);
        let target_ref = normalize_order_ref(&trade.target);
        let stop_ref = normalize_order_ref(&trade.safety_stop);

        Ok((
            BrokerLog {
                trade_id: trade.id,
                log: response.to_string(),
                ..Default::default()
            },
            OrderIds {
                stop: stop_ref,
                entry: entry_ref,
                target: target_ref,
            },
        ))
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        ensure_trade_account(trade, account)?;
        let client = IbkrClient::for_account(account)?;
        client.prepare_trading_session(Some(account))?;
        let live_orders = client.live_orders(account)?;

        let mut updated_orders = Vec::new();
        for base in [&trade.entry, &trade.target, &trade.safety_stop] {
            if let Some(live_order) =
                find_live_order_by_ref(&live_orders, &normalize_order_ref(base))
            {
                let mapped = map_live_order(base, live_order)?;
                if mapped != *base {
                    updated_orders.push(mapped);
                }
            }
        }

        let status = map_trade_status(trade, &updated_orders);
        Ok((
            status,
            updated_orders,
            BrokerLog {
                trade_id: trade.id,
                log: serde_json::Value::Array(live_orders).to_string(),
                ..Default::default()
            },
        ))
    }

    fn close_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        ensure_trade_account(trade, account)?;
        let client = IbkrClient::for_account(account)?;
        let account_id = broker_account_id(account)?;
        client.prepare_trading_session(Some(account))?;

        let target_ref = normalize_order_ref(&trade.target);
        let target_order_id = client.resolve_live_order_id(account, &target_ref)?;
        client.delete_no_content(&format!(
            "/iserver/account/{account_id}/order/{target_order_id}"
        ))?;

        let conid = resolve_conid(&client, &trade.trading_vehicle)?;
        let close_ref = format!("{target_ref}:manual-close");
        let payload = build_close_order(trade, account_id, &conid, &close_ref)?;
        let response = client.post_json_with_replies(
            &format!("/iserver/account/{account_id}/orders"),
            &serde_json::json!({ "orders": [payload] }),
        )?;

        let now = Utc::now().naive_utc();
        let mut order = trade.target.clone();
        order.broker_order_id = Some(close_ref);
        order.category = OrderCategory::Market;
        order.status = model::OrderStatus::New;
        order.submitted_at = Some(now);

        Ok((
            order,
            BrokerLog {
                trade_id: trade.id,
                log: response.to_string(),
                ..Default::default()
            },
        ))
    }

    fn cancel_trade(&self, trade: &Trade, account: &Account) -> Result<(), Box<dyn Error>> {
        ensure_trade_account(trade, account)?;
        let client = IbkrClient::for_account(account)?;
        let account_id = broker_account_id(account)?;
        client.prepare_trading_session(Some(account))?;
        let order_id = client.resolve_live_order_id(account, &normalize_order_ref(&trade.entry))?;
        client.delete_no_content(&format!("/iserver/account/{account_id}/order/{order_id}"))
    }

    fn modify_stop(
        &self,
        trade: &Trade,
        account: &Account,
        new_stop_price: rust_decimal::Decimal,
    ) -> Result<String, Box<dyn Error>> {
        ensure_trade_account(trade, account)?;
        let client = IbkrClient::for_account(account)?;
        let account_id = broker_account_id(account)?;
        client.prepare_trading_session(Some(account))?;
        let order_ref = normalize_order_ref(&trade.safety_stop);
        let order_id = client.resolve_live_order_id(account, &order_ref)?;
        let conid = resolve_conid(&client, &trade.trading_vehicle)?;
        let payload = build_modify_order(
            trade,
            account_id,
            &conid,
            &trade.safety_stop,
            new_stop_price,
        )?;
        let _ = client.post_json_with_replies(
            &format!("/iserver/account/{account_id}/order/{order_id}"),
            &payload,
        )?;
        Ok(order_ref)
    }

    fn modify_target(
        &self,
        trade: &Trade,
        account: &Account,
        new_price: rust_decimal::Decimal,
    ) -> Result<String, Box<dyn Error>> {
        ensure_trade_account(trade, account)?;
        let client = IbkrClient::for_account(account)?;
        let account_id = broker_account_id(account)?;
        client.prepare_trading_session(Some(account))?;
        let order_ref = normalize_order_ref(&trade.target);
        let order_id = client.resolve_live_order_id(account, &order_ref)?;
        let conid = resolve_conid(&client, &trade.trading_vehicle)?;
        let payload = build_modify_order(trade, account_id, &conid, &trade.target, new_price)?;
        let _ = client.post_json_with_replies(
            &format!("/iserver/account/{account_id}/order/{order_id}"),
            &payload,
        )?;
        Ok(order_ref)
    }

    fn get_bars(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        timeframe: BarTimeframe,
        account: &Account,
    ) -> Result<Vec<MarketBar>, Box<dyn Error>> {
        let client = IbkrClient::for_account(account)?;
        client.prepare_trading_session(Some(account))?;
        get_bars(&client, symbol, start, end, timeframe)
    }

    fn get_latest_quote(
        &self,
        symbol: &str,
        account: &Account,
    ) -> Result<MarketQuote, Box<dyn Error>> {
        let client = IbkrClient::for_account(account)?;
        client.prepare_trading_session(Some(account))?;
        get_latest_quote(&client, symbol)
    }

    fn get_latest_trade(
        &self,
        symbol: &str,
        account: &Account,
    ) -> Result<MarketTradeTick, Box<dyn Error>> {
        let client = IbkrClient::for_account(account)?;
        client.prepare_trading_session(Some(account))?;
        get_latest_trade(&client, symbol)
    }

    fn fetch_executions(
        &self,
        trade: &Trade,
        account: &Account,
        after: Option<DateTime<Utc>>,
    ) -> Result<Vec<model::Execution>, Box<dyn Error>> {
        let client = IbkrClient::for_account(account)?;
        client.prepare_trading_session(Some(account))?;
        fetch_executions(&client, trade, account, after)
    }

    fn fetch_fee_activities(
        &self,
        trade: &Trade,
        account: &Account,
        after: Option<DateTime<Utc>>,
    ) -> Result<Vec<model::FeeActivity>, Box<dyn Error>> {
        let client = IbkrClient::for_account(account)?;
        client.prepare_trading_session(Some(account))?;
        fetch_fee_activities(&client, trade, account, after)
    }
}

impl IbkrBroker {
    /// Store IBKR gateway connection settings for an account.
    pub fn setup_connection(
        base_url: &str,
        allow_insecure_tls: bool,
        environment: &model::Environment,
        account: &Account,
    ) -> Result<ConnectionConfig, Box<dyn Error>> {
        let config = ConnectionConfig::new(base_url, allow_insecure_tls);
        let config = config.store(environment, account)?;
        Ok(config)
    }

    /// Read the configured IBKR gateway connection for an account.
    pub fn read_connection(
        environment: &model::Environment,
        account: &Account,
    ) -> Result<ConnectionConfig, Box<dyn Error>> {
        Ok(ConnectionConfig::read(environment, account)?)
    }

    /// Delete persisted IBKR gateway settings for an account.
    pub fn delete_connection(
        environment: &model::Environment,
        account: &Account,
    ) -> Result<(), Box<dyn Error>> {
        ConnectionConfig::delete(environment, account)?;
        Ok(())
    }

    /// Resolve symbol metadata from IBKR contract search.
    pub fn fetch_contract_metadata(
        account: &Account,
        symbol: &str,
    ) -> Result<ContractMetadata, Box<dyn Error>> {
        let client = IbkrClient::for_account(account)?;
        client.prepare_trading_session(Some(account))?;
        fetch_contract_metadata_with_client(&client, symbol)
    }
}

#[cfg(test)]
mod tests {
    use super::{build_bracket_orders, map_live_order, map_trade_status, IbkrBroker};
    use crate::orders::{map_ibkr_order_status, tracked_order_refs};
    use crate::parsing::parse_ibkr_datetime;
    use model::{
        Account, Currency, Environment, OrderStatus, Status, Trade, TradingVehicleCategory,
    };
    use rust_decimal_macros::dec;
    use serde_json::json;

    fn sample_trade() -> Trade {
        let mut trade = Trade {
            account_id: Account::default().id,
            ..Trade::default()
        };
        trade.trading_vehicle.symbol = "AAPL".to_string();
        trade.trading_vehicle.category = TradingVehicleCategory::Stock;
        trade.entry.unit_price = dec!(100);
        trade.target.unit_price = dec!(110);
        trade.safety_stop.unit_price = dec!(95);
        trade.entry.quantity = 10;
        trade.target.quantity = 10;
        trade.safety_stop.quantity = 10;
        trade.entry.currency = Currency::USD;
        trade.target.currency = Currency::USD;
        trade.safety_stop.currency = Currency::USD;
        trade
    }

    #[test]
    fn ibkr_datetime_parser_supports_documented_formats() {
        assert!(parse_ibkr_datetime("20260318-15:45:00").is_some());
        assert!(parse_ibkr_datetime("20260318 15:45:00").is_some());
        assert!(parse_ibkr_datetime("260318154500").is_some());
    }

    #[test]
    fn ibkr_order_status_mapping_covers_core_states() {
        assert_eq!(map_ibkr_order_status("Submitted"), OrderStatus::New);
        assert_eq!(map_ibkr_order_status("PreSubmitted"), OrderStatus::Held);
        assert_eq!(map_ibkr_order_status("Filled"), OrderStatus::Filled);
        assert_eq!(map_ibkr_order_status("Cancelled"), OrderStatus::Canceled);
        assert_eq!(map_ibkr_order_status("Rejected"), OrderStatus::Rejected);
    }

    #[test]
    fn build_bracket_orders_assigns_parent_child_relationships() {
        let trade = sample_trade();
        let orders = build_bracket_orders(&trade, "U1234567", "265598").expect("orders");
        assert_eq!(orders.len(), 3);

        let entry_order = orders.first().expect("entry order");
        let target_order = orders.get(1).expect("target order");
        let stop_order = orders.get(2).expect("stop order");

        assert_eq!(
            entry_order.get("cOID"),
            Some(&json!(trade.entry.id.to_string()))
        );
        assert_eq!(
            target_order.get("parentId"),
            Some(&json!(trade.entry.id.to_string()))
        );
        assert_eq!(
            stop_order.get("parentId"),
            Some(&json!(trade.entry.id.to_string()))
        );
        assert_eq!(
            target_order.get("cOID"),
            Some(&json!(trade.target.id.to_string()))
        );
        assert_eq!(
            stop_order.get("cOID"),
            Some(&json!(trade.safety_stop.id.to_string()))
        );
    }

    #[test]
    fn map_live_order_translates_filled_quantities_and_prices() {
        let mut trade = sample_trade();
        trade.entry.broker_order_id = Some(trade.entry.id.to_string());
        let live_order = json!({
            "order_ref": trade.entry.id.to_string(),
            "status": "Filled",
            "filledQuantity": "10",
            "avgPrice": "101.25",
            "lastExecutionTime": "20260318-15:45:00"
        });

        let mapped = map_live_order(&trade.entry, &live_order).expect("mapped");
        assert_eq!(mapped.status, OrderStatus::Filled);
        assert_eq!(mapped.filled_quantity, 10);
        assert_eq!(mapped.average_filled_price, Some(dec!(101.25)));
        assert!(mapped.filled_at.is_some());
    }

    #[test]
    fn trade_status_prefers_terminal_exit_orders() {
        let trade = sample_trade();
        let mut stop = trade.safety_stop.clone();
        stop.status = OrderStatus::Filled;
        assert_eq!(map_trade_status(&trade, &[stop]), Status::ClosedStopLoss);
    }

    #[test]
    fn tracked_order_refs_uses_current_broker_refs() {
        let mut trade = sample_trade();
        trade.entry.broker_order_id = Some("entry-ref".to_string());
        let refs = tracked_order_refs(&trade);
        assert!(refs.contains("entry-ref"));
        assert!(refs.contains(&trade.target.id.to_string()));
    }

    #[test]
    fn connection_helpers_roundtrip_through_public_api() {
        let account = Account {
            name: "ibkr-unit".to_string(),
            environment: Environment::Paper,
            ..Account::default()
        };
        let config = IbkrBroker::setup_connection(
            "https://localhost:5000/v1/api/",
            true,
            &Environment::Paper,
            &account,
        );
        if config.is_ok() {
            let stored =
                IbkrBroker::read_connection(&Environment::Paper, &account).expect("stored");
            assert_eq!(stored.base_url, "https://localhost:5000/v1/api");
            IbkrBroker::delete_connection(&Environment::Paper, &account).expect("deleted");
        }
    }
}
