//! Security tests for the IBKR broker integration.
//!
//! Each test asserts the expected security behavior directly. These tests should
//! fail on a regression, not pass because a panic was expected.

use ibkr_broker::{ConnectionConfig, IbkrBroker};
use model::{
    Account, Broker, BrokerKind, Environment, TimeInForce, Trade, TradeCategory,
    TradingVehicleCategory,
};
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::str::FromStr;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

// =================================================================
// Shared mock gateway infrastructure
// =================================================================

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn with_mock_gateway<T>(server: &TestServer, run: impl FnOnce() -> T) -> T {
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

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct MockHandle {
    state: Arc<Mutex<MockState>>,
    expectation_id: usize,
}

#[allow(dead_code)]
impl MockHandle {
    fn assert(&self) {
        assert!(
            self.hits() > 0,
            "expected request was not observed: {:?}",
            self.expectation()
        );
    }

    fn hits(&self) -> usize {
        self.expectation().hits
    }

    fn expectation(&self) -> MockExpectation {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .expectations[self.expectation_id]
            .clone()
    }
}

#[derive(Clone, Debug)]
struct MockExpectation {
    method: &'static str,
    path: &'static str,
    query: Vec<(&'static str, String)>,
    body_contains: Vec<String>,
    response_status: u16,
    response_body: String,
    hits: usize,
}

impl MockExpectation {
    fn json(method: &'static str, path: &'static str, body: Value) -> Self {
        Self {
            method,
            path,
            query: Vec::new(),
            body_contains: Vec::new(),
            response_status: 200,
            response_body: body.to_string(),
            hits: 0,
        }
    }

    fn query(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.query.push((name, value.into()));
        self
    }

    fn body_contains(mut self, value: impl Into<String>) -> Self {
        self.body_contains.push(value.into());
        self
    }

    fn matches(&self, request: &TestRequest) -> bool {
        if self.method != request.method || self.path != request.path {
            return false;
        }
        if self
            .query
            .iter()
            .any(|(name, value)| request.query.get(*name) != Some(value))
        {
            return false;
        }
        !self
            .body_contains
            .iter()
            .any(|needle| !request.body.contains(needle))
    }
}

#[derive(Debug)]
struct MockState {
    expectations: Vec<MockExpectation>,
    stop_requested: bool,
}

#[derive(Debug)]
struct TestServer {
    address: String,
    state: Arc<Mutex<MockState>>,
    thread: Option<JoinHandle<()>>,
}

impl TestServer {
    fn start() -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(MockState {
            expectations: Vec::new(),
            stop_requested: false,
        }));
        let thread_state = Arc::clone(&state);

        let thread = thread::spawn(move || loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let _ = handle_connection(stream, &thread_state);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if stop_requested(&thread_state) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        });

        Self {
            address,
            state,
            thread: Some(thread),
        }
    }

    fn base_url(&self) -> &str {
        &self.address
    }

    fn expect(&self, expectation: MockExpectation) -> MockHandle {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let id = state.expectations.len();
        state.expectations.push(expectation);
        MockHandle {
            state: Arc::clone(&self.state),
            expectation_id: id,
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        {
            self.state
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .stop_requested = true;
        }
        let address = self.address.trim_start_matches("http://");
        if let Ok(stream) = TcpStream::connect(address) {
            let _ = stream.shutdown(Shutdown::Both);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Debug)]
struct TestRequest {
    method: String,
    path: String,
    query: HashMap<String, String>,
    body: String,
}

fn stop_requested(state: &Arc<Mutex<MockState>>) -> bool {
    state
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .stop_requested
}

fn handle_connection(
    mut stream: TcpStream,
    state: &Arc<Mutex<MockState>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = match read_request(&stream)? {
        Some(r) => r,
        None => return Ok(()),
    };
    let response = {
        let mut locked = state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(exp) = locked.expectations.iter_mut().find(|e| e.matches(&request)) {
            exp.hits += 1;
            (exp.response_status, exp.response_body.clone())
        } else {
            (
                500,
                format!(
                    "unexpected request: {} {} body={}",
                    request.method, request.path, request.body
                ),
            )
        }
    };
    write!(
        stream,
        "HTTP/1.1 {} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.0, response.1.len(), response.1
    )?;
    stream.flush()?;
    Ok(())
}

fn read_request(stream: &TcpStream) -> Result<Option<TestRequest>, Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(None);
    }
    let request_line = request_line.trim_end_matches(['\r', '\n']);
    if request_line.is_empty() {
        return Ok(None);
    }
    let mut parts = request_line.split_whitespace();
    let Some(method) = parts.next() else {
        return Ok(None);
    };
    let Some(target) = parts.next() else {
        return Ok(None);
    };
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or_default();
            }
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    let body = String::from_utf8_lossy(&body).into_owned();
    let (path, query) = split_target(target);
    Ok(Some(TestRequest {
        method: method.to_string(),
        path: path.to_string(),
        query,
        body,
    }))
}

fn split_target(target: &str) -> (&str, HashMap<String, String>) {
    let Some((path, raw)) = target.split_once('?') else {
        return (target, HashMap::new());
    };
    let mut query = HashMap::new();
    for pair in raw.split('&').filter(|p| !p.is_empty()) {
        let (n, v) = pair.split_once('=').unwrap_or((pair, ""));
        query.insert(n.to_string(), v.to_string());
    }
    (path, query)
}

fn account() -> Account {
    Account {
        name: "ibkr-sec".to_string(),
        environment: Environment::Paper,
        broker_kind: BrokerKind::Ibkr,
        broker_account_id: Some("U1234567".to_string()),
        ..Account::default()
    }
}

fn trade_for(acc: &Account) -> Trade {
    let mut t = Trade {
        account_id: acc.id,
        category: TradeCategory::Long,
        ..Trade::default()
    };
    t.trading_vehicle.symbol = "AAPL".to_string();
    t.trading_vehicle.category = TradingVehicleCategory::Stock;
    t.trading_vehicle.exchange = Some("SMART".to_string());
    t.entry.unit_price = dec!(100);
    t.target.unit_price = dec!(110);
    t.safety_stop.unit_price = dec!(95);
    t.entry.quantity = 10;
    t.target.quantity = 10;
    t.safety_stop.quantity = 10;
    t.entry.time_in_force = TimeInForce::UntilCanceled;
    t.target.time_in_force = TimeInForce::UntilCanceled;
    t.safety_stop.time_in_force = TimeInForce::UntilCanceled;
    t
}

fn mock_session(server: &TestServer) {
    server.expect(MockExpectation::json(
        "GET",
        "/v1/api/iserver/auth/status",
        json!({ "authenticated": true, "connected": true }),
    ));
    server.expect(MockExpectation::json(
        "GET",
        "/v1/api/iserver/accounts",
        json!({ "selectedAccount": "U1234567" }),
    ));
    server.expect(
        MockExpectation::json(
            "POST",
            "/v1/api/iserver/account",
            json!({ "acctId": "U1234567" }),
        )
        .body_contains("U1234567"),
    );
}

// =================================================================
// 1. TLS should be secure by default
// =================================================================

/// Default config should NOT disable TLS verification.
#[test]
fn default_config_should_enforce_tls() {
    let config = ConnectionConfig::default();
    assert!(
        !config.allow_insecure_tls,
        "Default config should enforce TLS verification"
    );
}

/// Missing TLS flag should default to secure (false).
#[test]
fn config_parser_missing_tls_should_default_to_secure() {
    let config = ConnectionConfig::from_str("https://remote-gateway.example.com/v1/api").unwrap();
    assert!(
        !config.allow_insecure_tls,
        "Missing TLS flag should default to secure"
    );
}

// =================================================================
// 2. Contract search should reject mismatched symbols
// =================================================================

/// When no exact symbol match is found, submit_trade should fail
/// instead of silently using the wrong contract.
#[test]
fn contract_search_should_reject_wrong_symbol() {
    let server = TestServer::start();
    mock_session(&server);

    server.expect(
        MockExpectation::json(
            "GET",
            "/v1/api/iserver/secdef/search",
            json!([{ "conid": "999999", "symbol": "WRONG_TICKER", "exchange": "SMART" }]),
        )
        .query("symbol", "AAPL")
        .query("secType", "STK"),
    );

    // Also mock the order endpoint — if the bug exists, submit will
    // reach this endpoint with the wrong conid.
    server.expect(
        MockExpectation::json(
            "POST",
            "/v1/api/iserver/account/U1234567/orders",
            json!([{ "order_id": "9001", "order_status": "Submitted" }]),
        )
        .body_contains("999999"),
    );

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        let a = account();
        let t = trade_for(&a);
        // CORRECT: should fail because AAPL was not found.
        let result = broker.submit_trade(&t, &a);
        assert!(result.is_err(), "Should reject when symbol doesn't match");
    });
}

// =================================================================
// 3. Buying-power warnings should NOT be auto-confirmed
// =================================================================

/// When IBKR sends a buying-power warning, submit_trade should fail
/// instead of blindly confirming.
#[test]
fn should_reject_buying_power_warning() {
    let server = TestServer::start();
    mock_session(&server);

    server.expect(
        MockExpectation::json(
            "GET",
            "/v1/api/iserver/secdef/search",
            json!([{ "conid": "265598", "symbol": "AAPL", "exchange": "SMART" }]),
        )
        .query("symbol", "AAPL"),
    );

    server.expect(
        MockExpectation::json(
            "POST",
            "/v1/api/iserver/account/U1234567/orders",
            json!({
                "id": "reply-danger-123",
                "message": ["WARNING: This order exceeds your account buying power. Proceed?"],
                "isSuppressed": false,
                "messageIds": ["o354"]
            }),
        )
        .body_contains("orders"),
    );

    // If it auto-confirms, it will hit this endpoint — but it shouldn't.
    server.expect(MockExpectation::json(
        "POST",
        "/v1/api/iserver/reply/reply-danger-123",
        json!([{ "order_id": "9001", "order_status": "Submitted" }]),
    ));

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        let a = account();
        let t = trade_for(&a);
        // CORRECT: should fail due to buying-power warning.
        let result = broker.submit_trade(&t, &a);
        assert!(
            result.is_err(),
            "Should reject order when IBKR warns about buying power"
        );
    });
}

// =================================================================
// 4. Zero quantity/price should be rejected locally
// =================================================================

/// Zero-quantity orders should be rejected before hitting the broker.
#[test]
fn should_reject_zero_quantity_orders() {
    let server = TestServer::start();
    mock_session(&server);

    server.expect(
        MockExpectation::json(
            "GET",
            "/v1/api/iserver/secdef/search",
            json!([{ "conid": "265598", "symbol": "AAPL" }]),
        )
        .query("symbol", "AAPL"),
    );

    server.expect(MockExpectation::json(
        "POST",
        "/v1/api/iserver/account/U1234567/orders",
        json!([{ "order_id": "9001", "order_status": "Submitted" }]),
    ));

    let a = account();
    let mut t = trade_for(&a);
    t.entry.quantity = 0;
    t.target.quantity = 0;
    t.safety_stop.quantity = 0;

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        // CORRECT: should fail locally before sending to broker.
        let result = broker.submit_trade(&t, &a);
        assert!(result.is_err(), "Should reject zero-quantity orders");
    });
}

/// Zero-price orders should be rejected before hitting the broker.
#[test]
fn should_reject_zero_price_orders() {
    let server = TestServer::start();
    mock_session(&server);

    server.expect(
        MockExpectation::json(
            "GET",
            "/v1/api/iserver/secdef/search",
            json!([{ "conid": "265598", "symbol": "AAPL" }]),
        )
        .query("symbol", "AAPL"),
    );

    server.expect(MockExpectation::json(
        "POST",
        "/v1/api/iserver/account/U1234567/orders",
        json!([{ "order_id": "9001", "order_status": "Submitted" }]),
    ));

    let a = account();
    let mut t = trade_for(&a);
    t.entry.unit_price = dec!(0);
    t.target.unit_price = dec!(0);
    t.safety_stop.unit_price = dec!(0);

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        let result = broker.submit_trade(&t, &a);
        assert!(result.is_err(), "Should reject zero-price orders");
    });
}

// =================================================================
// 5. Malformed execution side should not parse as Buy
// =================================================================

/// "blocked" should not be interpreted as a Buy execution.
#[test]
fn malformed_execution_side_should_not_be_buy() {
    let server = TestServer::start();
    mock_session(&server);

    let a = account();
    let mut t = trade_for(&a);
    t.entry.broker_order_id = Some(t.entry.id.to_string());
    t.target.broker_order_id = Some(t.target.id.to_string());
    t.safety_stop.broker_order_id = Some(t.safety_stop.id.to_string());

    server.expect(MockExpectation::json(
        "GET",
        "/v1/api/iserver/account/trades",
        json!([{
            "execution_id": "exec-1",
            "order_ref": t.entry.id.to_string(),
            "symbol": "AAPL",
            "side": "blocked",
            "size": "10",
            "price": "100.25",
            "trade_time": "20260318-15:45:00"
        }]),
    ));

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        let error = broker
            .fetch_executions(&t, &a, None)
            .expect_err("'blocked' is not a valid execution side");
        assert!(error.to_string().contains("execution side"));
    });
}

// =================================================================
// 6. Unrecognized status should return error, not Unknown
// =================================================================

/// When IBKR returns unrecognized status strings, sync_trade should
/// return an error instead of silently mapping to Unknown.
#[test]
fn unrecognized_status_should_error() {
    let server = TestServer::start();
    mock_session(&server);

    let a = account();
    let mut t = trade_for(&a);
    t.entry.broker_order_id = Some(t.entry.id.to_string());
    t.target.broker_order_id = Some(t.target.id.to_string());
    t.safety_stop.broker_order_id = Some(t.safety_stop.id.to_string());

    server.expect(
        MockExpectation::json(
            "GET",
            "/v1/api/iserver/account/orders",
            json!({
                "orders": [
                    { "orderId": "9100", "order_ref": t.entry.id.to_string(), "status": "SomeNewIBKRStatus" },
                    { "orderId": "9101", "order_ref": t.target.id.to_string(), "status": "WarnState" },
                    { "orderId": "9102", "order_ref": t.safety_stop.id.to_string(), "status": "SuspendedOrder" }
                ]
            }),
        )
        .query("accountId", "U1234567")
        .query("force", "true"),
    );

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        // CORRECT: should return Err for unrecognized statuses.
        let result = broker.sync_trade(&t, &a);
        assert!(result.is_err(), "Unrecognized order statuses should error");
    });
}

// =================================================================
// 7. Config should reject non-HTTP URL schemes
// =================================================================

/// Only http:// and https:// should be accepted as gateway URLs.
#[test]
fn config_should_reject_non_http_schemes() {
    // CORRECT: file:// scheme should be rejected.
    assert!(
        ConnectionConfig::from_str("file:///etc/passwd false").is_err(),
        "should reject non-HTTP URL scheme"
    );
}
