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
    Order, OrderCategory, OrderIds, Status, Trade, TradingVehicleCategory,
};
use orders::{
    build_bracket_orders, build_close_order, build_modify_order, find_live_order_by_ref,
    map_live_order, map_trade_status, normalize_order_ref, validate_bracket_trade,
};
use serde_json::Value;
use std::error::Error;
use support::{broker_account_id, ensure_trade_account};

pub(crate) const BROKER_NAME: &str = "ibkr";
pub(crate) const LIVE_ORDER_LOOKUP_RETRIES: usize = 5;
pub(crate) const LIVE_ORDER_LOOKUP_DELAY_MS: u64 = 150;

fn ensure_market_data_symbol(symbol: &str) -> Result<(), Box<dyn Error>> {
    if symbol.trim().is_empty() {
        return Err("Symbol cannot be empty".into());
    }
    Ok(())
}

fn ensure_bar_window(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<(), Box<dyn Error>> {
    if end <= start {
        return Err("Bar end time must be after start time".into());
    }
    Ok(())
}

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
        validate_bracket_trade(trade)?;
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
        ensure_market_data_symbol(symbol)?;
        ensure_bar_window(start, end)?;
        let client = IbkrClient::for_account(account)?;
        client.prepare_trading_session(Some(account))?;
        get_bars(&client, symbol, start, end, timeframe)
    }

    fn get_latest_quote(
        &self,
        symbol: &str,
        account: &Account,
    ) -> Result<MarketQuote, Box<dyn Error>> {
        ensure_market_data_symbol(symbol)?;
        let client = IbkrClient::for_account(account)?;
        client.prepare_trading_session(Some(account))?;
        get_latest_quote(&client, symbol)
    }

    fn get_latest_trade(
        &self,
        symbol: &str,
        account: &Account,
    ) -> Result<MarketTradeTick, Box<dyn Error>> {
        ensure_market_data_symbol(symbol)?;
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
        ensure_trade_account(trade, account)?;
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
        ensure_trade_account(trade, account)?;
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

    /// Verify the Client Portal Gateway is authenticated and can select the account.
    pub fn preflight_gateway(account: &Account) -> Result<(), Box<dyn Error>> {
        let client = IbkrClient::for_account(account)?;
        client.prepare_trading_session(Some(account))
    }

    /// Preview a Trust bracket order through IBKR `/whatif` without submitting it.
    pub fn preview_trade(account: &Account, trade: &Trade) -> Result<Value, Box<dyn Error>> {
        ensure_trade_account(trade, account)?;
        validate_bracket_trade(trade)?;
        let client = IbkrClient::for_account(account)?;
        let account_id = broker_account_id(account)?;
        client.prepare_trading_session(Some(account))?;
        let conid = resolve_conid(&client, &trade.trading_vehicle)?;
        let _ = client.get_json_value(
            "/iserver/marketdata/snapshot",
            &[("conids", conid.to_string())],
        )?;
        let payload = serde_json::json!({
            "orders": build_bracket_orders(trade, account_id, &conid)?,
        });
        let response = client.post_json_value(
            &format!("/iserver/account/{account_id}/orders/whatif"),
            &payload,
        )?;
        reject_order_preview_error(&response)?;
        Ok(response)
    }

    /// Resolve symbol metadata from IBKR contract search.
    pub fn fetch_contract_metadata(
        account: &Account,
        symbol: &str,
    ) -> Result<ContractMetadata, Box<dyn Error>> {
        Self::fetch_contract_metadata_for_category(account, symbol, TradingVehicleCategory::Stock)
    }

    /// Resolve symbol metadata from IBKR contract search for a Trust category.
    pub fn fetch_contract_metadata_for_category(
        account: &Account,
        symbol: &str,
        category: TradingVehicleCategory,
    ) -> Result<ContractMetadata, Box<dyn Error>> {
        ensure_market_data_symbol(symbol)?;
        let client = IbkrClient::for_account(account)?;
        client.prepare_trading_session(Some(account))?;
        fetch_contract_metadata_with_client(&client, symbol, category)
    }
}

fn reject_order_preview_error(response: &Value) -> Result<(), Box<dyn Error>> {
    if let Some(error) = response
        .get("error")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|error| !error.is_empty())
    {
        return Err(format!("IBKR what-if preview rejected order: {error}").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_bracket_orders, build_close_order, build_modify_order, map_live_order,
        map_trade_status, validate_bracket_trade, IbkrBroker,
    };
    use crate::orders::{map_ibkr_order_status, tracked_order_refs};
    use crate::parsing::{
        decimal_field, decimal_field_any, decimal_field_optional_any, parse_epoch_datetime,
        parse_ibkr_datetime, string_field_optional, trade_timestamp, u64_field_optional_any,
    };
    use crate::support::{broker_account_id, ensure_trade_account};
    use chrono::{TimeZone, Utc};
    use model::{
        Account, BarTimeframe, Broker, BrokerKind, Currency, Environment, OrderStatus, Status,
        Trade, TradingVehicleCategory,
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
        assert!(parse_ibkr_datetime("2026-03-18T15:45:00Z").is_none());
    }

    #[test]
    fn ibkr_trade_timestamp_uses_trade_time_date_time_then_updated() {
        let trade_time = json!({
            "trade_time": "20260318-15:45:00",
            "date_time": "20260317 15:45:00",
            "_updated": 1773848700000i64
        });
        assert_eq!(
            trade_timestamp(&trade_time),
            parse_ibkr_datetime("20260318-15:45:00")
        );

        let date_time = json!({
            "date_time": "20260317 15:45:00",
            "_updated": 1773848700000i64
        });
        assert_eq!(
            trade_timestamp(&date_time),
            parse_ibkr_datetime("20260317 15:45:00")
        );

        let updated = json!({ "_updated": "1773848700000" });
        assert_eq!(
            trade_timestamp(&updated).map(|value| value.and_utc().timestamp_millis()),
            Some(1773848700000)
        );

        assert!(trade_timestamp(&json!({ "trade_time": "not-a-date" })).is_none());
    }

    #[test]
    fn ibkr_payload_field_parsers_accept_broker_value_shapes() {
        let payload = json!({
            "price": "1,234.56",
            "fallback_price": 42.5,
            "quantity_text": "1,000",
            "quantity_number": 2500,
            "flag": true,
            "updated": 1773848700000i64
        });

        assert_eq!(
            string_field_optional(&payload, "flag"),
            Some("true".to_string())
        );
        assert_eq!(
            decimal_field(&payload, "price").expect("decimal with grouping"),
            dec!(1234.56)
        );
        assert_eq!(
            decimal_field_any(&payload, &["missing", "fallback_price"]).expect("fallback decimal"),
            dec!(42.5)
        );
        assert!(decimal_field(&payload, "missing").is_err());
        assert_eq!(
            decimal_field_optional_any(&payload, &["missing", "also_missing"]),
            None
        );
        assert_eq!(
            u64_field_optional_any(&payload, &["missing", "quantity_text"]),
            Some(1000)
        );
        assert_eq!(
            u64_field_optional_any(&payload, &["missing", "quantity_number"]),
            Some(2500)
        );
        assert_eq!(
            parse_epoch_datetime(payload.get("updated")).map(|value| value.timestamp_millis()),
            Some(1773848700000)
        );
        assert!(parse_epoch_datetime(Some(&json!(false))).is_none());
    }

    #[test]
    fn broker_account_id_requires_non_blank_value() {
        let mut account = Account {
            name: "ibkr-missing-id".to_string(),
            ..Account::default()
        };
        let error = broker_account_id(&account).expect_err("missing broker account id");
        assert!(error.to_string().contains("ibkr-missing-id"));

        account.broker_account_id = Some("  ".to_string());
        assert!(broker_account_id(&account).is_err());

        account.broker_account_id = Some("U1234567".to_string());
        assert_eq!(
            broker_account_id(&account).expect("broker account id"),
            "U1234567"
        );
    }

    #[test]
    fn ensure_trade_account_rejects_cross_account_usage() {
        let account = Account::default();
        let mut trade = sample_trade();
        trade.account_id = account.id;
        assert!(ensure_trade_account(&trade, &account).is_ok());

        let other_account = Account::default();
        let error = ensure_trade_account(&trade, &other_account).expect_err("account mismatch");
        assert_eq!(
            error.to_string(),
            "Trade account does not match the broker account"
        );
    }

    #[test]
    fn ibkr_broker_reports_kind_and_metadata_wrapper_validates_symbol() {
        let broker = IbkrBroker;
        let account = Account::default();

        assert_eq!(broker.kind(), BrokerKind::Ibkr);

        let error = IbkrBroker::fetch_contract_metadata(&account, " ")
            .expect_err("blank symbol should fail before client setup");
        assert_eq!(error.to_string(), "Symbol cannot be empty");
    }

    #[test]
    fn ibkr_broker_market_data_validates_request_before_client_setup() {
        let broker = IbkrBroker;
        let account = Account::default();
        let start = Utc.with_ymd_and_hms(2026, 5, 7, 13, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 5, 7, 14, 0, 0).unwrap();

        let bars_err = broker
            .get_bars(" ", start, end, BarTimeframe::OneMinute, &account)
            .expect_err("blank symbol should fail before client setup");
        assert_eq!(bars_err.to_string(), "Symbol cannot be empty");

        let window_err = broker
            .get_bars("AAPL", end, start, BarTimeframe::OneMinute, &account)
            .expect_err("invalid window should fail before client setup");
        assert_eq!(
            window_err.to_string(),
            "Bar end time must be after start time"
        );

        let quote_err = broker
            .get_latest_quote("\t", &account)
            .expect_err("blank quote symbol should fail before client setup");
        assert_eq!(quote_err.to_string(), "Symbol cannot be empty");

        let trade_err = broker
            .get_latest_trade("\n", &account)
            .expect_err("blank trade symbol should fail before client setup");
        assert_eq!(trade_err.to_string(), "Symbol cannot be empty");

        let metadata_err = IbkrBroker::fetch_contract_metadata_for_category(
            &account,
            "",
            TradingVehicleCategory::Stock,
        )
        .expect_err("blank metadata symbol should fail before client setup");
        assert_eq!(metadata_err.to_string(), "Symbol cannot be empty");
    }

    #[test]
    fn ibkr_broker_account_scoped_feeds_validate_account_before_client_setup() {
        let broker = IbkrBroker;
        let account = Account::default();
        let trade = sample_trade();

        let executions_err = broker
            .fetch_executions(&trade, &account, None)
            .expect_err("account mismatch should fail before client setup");
        assert_eq!(
            executions_err.to_string(),
            "Trade account does not match the broker account"
        );

        let fees_err = broker
            .fetch_fee_activities(&trade, &account, None)
            .expect_err("account mismatch should fail before client setup");
        assert_eq!(
            fees_err.to_string(),
            "Trade account does not match the broker account"
        );
    }

    #[test]
    fn ibkr_order_status_mapping_covers_core_states() {
        assert_eq!(
            map_ibkr_order_status("Submitted").expect("submitted"),
            OrderStatus::New
        );
        assert_eq!(
            map_ibkr_order_status("PreSubmitted").expect("presubmitted"),
            OrderStatus::Held
        );
        assert_eq!(
            map_ibkr_order_status("Filled").expect("filled"),
            OrderStatus::Filled
        );
        assert_eq!(
            map_ibkr_order_status("Cancelled").expect("cancelled"),
            OrderStatus::Canceled
        );
        assert_eq!(
            map_ibkr_order_status("Rejected").expect("rejected"),
            OrderStatus::Rejected
        );
        assert!(map_ibkr_order_status("SomeNewIBKRStatus").is_err());
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
    fn build_bracket_orders_sets_security_type_for_etfs_and_bonds() {
        let mut etf = sample_trade();
        etf.trading_vehicle.category = TradingVehicleCategory::Etf;
        let etf_orders = build_bracket_orders(&etf, "U1234567", "756733").expect("etf orders");
        let etf_entry = etf_orders.first().expect("etf entry order");
        assert_eq!(etf_entry.get("secType"), Some(&json!("STK")));

        let mut bond = sample_trade();
        bond.trading_vehicle.symbol = "9128285M8".to_string();
        bond.trading_vehicle.category = TradingVehicleCategory::Bond;
        let bond_orders = build_bracket_orders(&bond, "U1234567", "123456").expect("bond orders");
        let bond_entry = bond_orders.first().expect("bond entry order");
        assert_eq!(bond_entry.get("secType"), Some(&json!("BOND")));
    }

    #[test]
    fn build_close_and_modify_orders_preserve_refs_and_exit_side() {
        let trade = sample_trade();

        let close =
            build_close_order(&trade, "U1234567", "265598", "manual-close-ref").expect("close");
        assert_eq!(close.get("cOID"), Some(&json!("manual-close-ref")));
        assert_eq!(close.get("orderType"), Some(&json!("MKT")));
        assert_eq!(close.get("side"), Some(&json!("SELL")));
        assert_eq!(close.get("quantity"), Some(&json!(10)));

        let modify_target =
            build_modify_order(&trade, "U1234567", "265598", &trade.target, dec!(111.25))
                .expect("modify target");
        assert_eq!(modify_target.get("orderType"), Some(&json!("LMT")));
        assert_eq!(modify_target.get("price"), Some(&json!("111.25")));
        assert_eq!(
            modify_target.get("cOID"),
            Some(&json!(trade.target.id.to_string()))
        );

        let modify_stop =
            build_modify_order(&trade, "U1234567", "265598", &trade.safety_stop, dec!(96.5))
                .expect("modify stop");
        assert_eq!(modify_stop.get("orderType"), Some(&json!("STP")));
        assert_eq!(modify_stop.get("price"), Some(&json!("96.5")));
    }

    #[test]
    fn validate_bracket_trade_rejects_zero_quantity_and_price() {
        let mut zero_quantity = sample_trade();
        zero_quantity.entry.quantity = 0;
        let quantity_error =
            validate_bracket_trade(&zero_quantity).expect_err("zero quantity rejected");
        assert!(quantity_error.to_string().contains("quantity"));

        let mut zero_price = sample_trade();
        zero_price.safety_stop.unit_price = dec!(0);
        let price_error = validate_bracket_trade(&zero_price).expect_err("zero price rejected");
        assert!(price_error.to_string().contains("price"));
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
