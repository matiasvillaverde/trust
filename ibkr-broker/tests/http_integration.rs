use httpmock::prelude::*;
use ibkr_broker::IbkrBroker;
use model::{
    Account, Broker, BrokerKind, Environment, TimeInForce, Trade, TradeCategory,
    TradingVehicleCategory,
};
use rust_decimal_macros::dec;
use serde_json::json;
use std::sync::{Mutex, OnceLock};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn with_mock_gateway<T>(server: &MockServer, run: impl FnOnce() -> T) -> T {
    let _guard = env_lock()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let base_url = format!("{}/v1/api", server.base_url());
    std::env::set_var("TRUST_IBKR_URL", base_url);
    std::env::set_var("TRUST_IBKR_ALLOW_INSECURE_TLS", "true");
    let result = run();
    std::env::remove_var("TRUST_IBKR_URL");
    std::env::remove_var("TRUST_IBKR_ALLOW_INSECURE_TLS");
    result
}

fn account() -> Account {
    Account {
        name: "ibkr-main".to_string(),
        environment: Environment::Paper,
        broker_kind: BrokerKind::Ibkr,
        broker_account_id: Some("U1234567".to_string()),
        ..Account::default()
    }
}

fn trade() -> Trade {
    let mut trade = Trade {
        category: TradeCategory::Long,
        ..Trade::default()
    };
    trade.trading_vehicle.symbol = "AAPL".to_string();
    trade.trading_vehicle.category = TradingVehicleCategory::Stock;
    trade.trading_vehicle.exchange = Some("SMART".to_string());
    trade.entry.quantity = 10;
    trade.target.quantity = 10;
    trade.safety_stop.quantity = 10;
    trade.entry.unit_price = dec!(100);
    trade.target.unit_price = dec!(110);
    trade.safety_stop.unit_price = dec!(95);
    trade.entry.time_in_force = TimeInForce::UntilCanceled;
    trade.target.time_in_force = TimeInForce::UntilCanceled;
    trade.safety_stop.time_in_force = TimeInForce::UntilCanceled;
    trade
}

fn mock_session(server: &MockServer) {
    server.mock(|when, then| {
        when.method(GET).path("/v1/api/iserver/auth/status");
        then.status(200)
            .json_body(json!({ "authenticated": true, "connected": true }));
    });
    server.mock(|when, then| {
        when.method(GET).path("/v1/api/iserver/accounts");
        then.status(200)
            .json_body(json!({ "selectedAccount": "U1234567" }));
    });
    server.mock(|when, then| {
        when.method(POST)
            .path("/v1/api/iserver/account")
            .body_contains("U1234567");
        then.status(200).json_body(json!({ "acctId": "U1234567" }));
    });
}

fn mock_contract_search(server: &MockServer) {
    server.mock(|when, then| {
        when.method(GET)
            .path("/v1/api/iserver/secdef/search")
            .query_param("symbol", "AAPL")
            .query_param("secType", "STK");
        then.status(200).json_body(json!([
            {
                "conid": "265598",
                "symbol": "AAPL",
                "companyName": "Apple Inc",
                "description": "NASDAQ",
                "exchange": "SMART",
                "currency": "USD"
            }
        ]));
    });
}

#[test]
fn submit_trade_posts_bracket_orders_and_returns_local_order_refs() {
    let server = MockServer::start();
    mock_session(&server);
    mock_contract_search(&server);

    let account = account();
    let trade = trade();
    let mut trade = trade;
    trade.account_id = account.id;
    let submit = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/api/iserver/account/U1234567/orders")
            .body_contains(trade.entry.id.to_string())
            .body_contains(trade.target.id.to_string())
            .body_contains(trade.safety_stop.id.to_string())
            .body_contains("\"parentId\"");
        then.status(200).json_body(json!([
            {
                "order_id": "9001",
                "local_order_id": trade.entry.id.to_string(),
                "order_status": "Submitted"
            }
        ]));
    });

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        let (_, ids) = broker.submit_trade(&trade, &account).unwrap();
        assert_eq!(ids.entry, trade.entry.id.to_string());
        assert_eq!(ids.target, trade.target.id.to_string());
        assert_eq!(ids.stop, trade.safety_stop.id.to_string());
    });

    submit.assert();
}

#[test]
fn cancel_trade_resolves_order_id_from_order_ref_before_delete() {
    let server = MockServer::start();
    mock_session(&server);
    let mut trade = trade();
    let account = account();
    trade.account_id = account.id;
    trade.entry.broker_order_id = Some(trade.entry.id.to_string());

    let lookup = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/api/iserver/account/orders")
            .query_param("accountId", "U1234567")
            .query_param("force", "true");
        then.status(200).json_body(json!({
            "orders": [
                {
                    "orderId": "7001",
                    "order_ref": trade.entry.id.to_string(),
                    "status": "Submitted"
                }
            ]
        }));
    });
    let delete = server.mock(|when, then| {
        when.method(DELETE)
            .path("/v1/api/iserver/account/U1234567/order/7001");
        then.status(200).json_body(json!({ "ok": true }));
    });

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        broker.cancel_trade(&trade, &account).unwrap();
    });

    lookup.assert();
    delete.assert();
}

#[test]
fn modify_stop_uses_order_ref_for_lookup_and_returns_same_ref() {
    let server = MockServer::start();
    mock_session(&server);
    mock_contract_search(&server);
    let mut trade = trade();
    let account = account();
    trade.account_id = account.id;
    trade.entry.broker_order_id = Some(trade.entry.id.to_string());
    trade.safety_stop.broker_order_id = Some(trade.safety_stop.id.to_string());

    let lookup = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/api/iserver/account/orders")
            .query_param("accountId", "U1234567")
            .query_param("force", "true");
        then.status(200).json_body(json!({
            "orders": [
                {
                    "orderId": "8001",
                    "order_ref": trade.safety_stop.id.to_string(),
                    "status": "Submitted"
                }
            ]
        }));
    });
    let modify = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/api/iserver/account/U1234567/order/8001")
            .body_contains(trade.safety_stop.id.to_string())
            .body_contains(trade.entry.id.to_string())
            .body_contains("\"price\":\"94.5\"");
        then.status(200).json_body(json!([
            {
                "order_id": "8001",
                "local_order_id": trade.safety_stop.id.to_string(),
                "order_status": "Submitted"
            }
        ]));
    });

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        let returned = broker.modify_stop(&trade, &account, dec!(94.5)).unwrap();
        assert_eq!(returned, trade.safety_stop.id.to_string());
    });

    lookup.assert();
    modify.assert();
}

#[test]
fn sync_trade_maps_live_orders_into_trade_status_and_order_updates() {
    let server = MockServer::start();
    mock_session(&server);
    let mut trade = trade();
    let account = account();
    trade.account_id = account.id;
    trade.entry.broker_order_id = Some(trade.entry.id.to_string());
    trade.target.broker_order_id = Some(trade.target.id.to_string());
    trade.safety_stop.broker_order_id = Some(trade.safety_stop.id.to_string());

    let sync = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/api/iserver/account/orders")
            .query_param("accountId", "U1234567")
            .query_param("force", "true");
        then.status(200).json_body(json!({
            "orders": [
                {
                    "orderId": "9100",
                    "order_ref": trade.entry.id.to_string(),
                    "status": "Filled",
                    "filledQuantity": "10",
                    "avgPrice": "100.25",
                    "lastExecutionTime": "20260318-15:45:00"
                },
                {
                    "orderId": "9101",
                    "order_ref": trade.target.id.to_string(),
                    "status": "PreSubmitted"
                },
                {
                    "orderId": "9102",
                    "order_ref": trade.safety_stop.id.to_string(),
                    "status": "PreSubmitted"
                }
            ]
        }));
    });

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        let (status, updates, _) = broker.sync_trade(&trade, &account).unwrap();
        assert_eq!(status, model::Status::Filled);
        assert!(updates
            .iter()
            .any(|order| order.id == trade.entry.id && order.status == model::OrderStatus::Filled));
    });

    sync.assert();
}

#[test]
fn market_data_and_execution_endpoints_are_normalized() {
    let server = MockServer::start();
    mock_session(&server);
    mock_contract_search(&server);
    let mut trade = trade();
    let account = account();
    trade.account_id = account.id;
    trade.entry.broker_order_id = Some(trade.entry.id.to_string());
    trade.target.broker_order_id = Some(trade.target.id.to_string());
    trade.safety_stop.broker_order_id = Some(trade.safety_stop.id.to_string());

    let history = server.mock(|when, then| {
        when.method(GET)
            .path("/v1/api/iserver/marketdata/history");
        then.status(200).json_body(json!({
            "data": [
                { "o": "100", "h": "101", "l": "99", "c": "100.5", "v": "1000", "t": 1773830400000i64 }
            ]
        }));
    });
    let snapshot = server.mock(|when, then| {
        when.method(GET).path("/v1/api/iserver/marketdata/snapshot");
        then.status(200).json_body(json!([
            {
                "55": "AAPL",
                "84": "100.1",
                "88": "200",
                "86": "100.3",
                "85": "180",
                "31": "100.2",
                "7059": "25",
                "_updated": 1773830400000i64
            }
        ]));
    });
    let account_trades = server.mock(|when, then| {
        when.method(GET).path("/v1/api/iserver/account/trades");
        then.status(200).json_body(json!([
            {
                "execution_id": "exec-1",
                "order_ref": trade.entry.id.to_string(),
                "symbol": "AAPL",
                "side": "BUY",
                "size": "10",
                "price": "100.25",
                "commission": "-1.25",
                "trade_time": "20260318-15:45:00"
            },
            {
                "execution_id": "exec-2",
                "order_ref": "other-order",
                "symbol": "MSFT",
                "side": "BUY",
                "size": "1",
                "price": "1",
                "commission": "-0.10",
                "trade_time": "20260318-15:45:00"
            }
        ]));
    });

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        let end = chrono::DateTime::parse_from_rfc3339("2026-03-18T16:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let start = chrono::DateTime::parse_from_rfc3339("2026-03-17T16:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let bars = broker
            .get_bars("AAPL", start, end, model::BarTimeframe::OneDay, &account)
            .unwrap();
        assert_eq!(bars.len(), 1);

        let quote = broker.get_latest_quote("AAPL", &account).unwrap();
        assert_eq!(quote.symbol, "AAPL");
        assert_eq!(quote.bid_price, dec!(100.1));

        let trade_tick = broker.get_latest_trade("AAPL", &account).unwrap();
        assert_eq!(trade_tick.price, dec!(100.2));

        let executions = broker.fetch_executions(&trade, &account, None).unwrap();
        assert_eq!(executions.len(), 1);
        let expected_order_ref = trade.entry.id.to_string();
        assert_eq!(
            executions[0].broker_order_id.as_deref(),
            Some(expected_order_ref.as_str())
        );

        let fees = broker.fetch_fee_activities(&trade, &account, None).unwrap();
        assert_eq!(fees.len(), 1);
        assert_eq!(fees[0].amount, dec!(1.25));
    });

    history.assert();
    assert_eq!(snapshot.hits(), 2);
    assert_eq!(account_trades.hits(), 2);
}
