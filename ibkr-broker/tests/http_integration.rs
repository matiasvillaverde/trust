use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use ibkr_broker::IbkrBroker;
use model::{
    Account, AccountType, Broker, BrokerKind, Currency, DraftTrade, Environment, RuleLevel,
    RuleName, Status, TimeInForce, Trade, TradeCategory, TradingVehicle, TradingVehicleCategory,
    TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

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
struct MockHandle {
    state: Arc<Mutex<MockState>>,
    expectation_id: usize,
}

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
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.expectations[self.expectation_id].clone()
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
    request_count: usize,
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
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap_or_else(|error| panic!("failed to bind test server: {error}"));
        listener
            .set_nonblocking(true)
            .unwrap_or_else(|error| panic!("failed to set nonblocking test server: {error}"));
        let address = format!(
            "http://{}",
            listener
                .local_addr()
                .unwrap_or_else(|error| panic!("failed to read test server address: {error}"))
        );
        let state = Arc::new(Mutex::new(MockState {
            expectations: Vec::new(),
            request_count: 0,
            stop_requested: false,
        }));
        let thread_state = Arc::clone(&state);

        let thread = thread::spawn(move || loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream
                        .set_nonblocking(false)
                        .unwrap_or_else(|error| panic!("failed to set blocking stream: {error}"));
                    if let Err(error) = handle_connection(stream, &thread_state) {
                        panic!("test server request failed: {error}");
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if stop_requested(&thread_state) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("test server accept failed: {error}"),
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
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let expectation_id = state.expectations.len();
        state.expectations.push(expectation);
        MockHandle {
            state: Arc::clone(&self.state),
            expectation_id,
        }
    }

    fn request_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .request_count
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.stop_requested = true;
        }

        let address = self.address.trim_start_matches("http://");
        if let Ok(stream) = TcpStream::connect(address) {
            let _ = stream.shutdown(Shutdown::Both);
        }

        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .unwrap_or_else(|_| panic!("failed to join test server thread"));
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
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .stop_requested
}

fn handle_connection(
    mut stream: TcpStream,
    state: &Arc<Mutex<MockState>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let request = match read_request(&stream)? {
        Some(request) => request,
        None => return Ok(()),
    };

    let response = {
        let mut locked = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        locked.request_count = locked.request_count.saturating_add(1);
        if let Some(expectation) = locked
            .expectations
            .iter_mut()
            .find(|expectation| expectation.matches(&request))
        {
            expectation.hits += 1;
            (
                expectation.response_status,
                expectation.response_body.clone(),
            )
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
        response.0,
        response.1.len(),
        response.1
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
        let mut header_line = String::new();
        reader.read_line(&mut header_line)?;
        let header_line = header_line.trim_end_matches(['\r', '\n']);
        if header_line.is_empty() {
            break;
        }

        if let Some((name, value)) = header_line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse::<usize>().unwrap_or_default();
            }
        }
    }

    let mut body = vec![0_u8; content_length];
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
    let Some((path, raw_query)) = target.split_once('?') else {
        return (target, HashMap::new());
    };

    let mut query = HashMap::new();
    for pair in raw_query.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        query.insert(name.to_string(), value.to_string());
    }

    (path, query)
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

fn mock_contract_search(server: &TestServer) {
    mock_contract_search_for(server, "AAPL", "STK", "265598");
}

fn mock_contract_search_for(server: &TestServer, symbol: &str, sec_type: &str, conid: &str) {
    server.expect(
        MockExpectation::json(
            "GET",
            "/v1/api/iserver/secdef/search",
            json!([
                {
                    "conid": conid,
                    "symbol": symbol,
                    "secType": sec_type,
                    "companyName": "Fixture Inc",
                    "description": "SMART",
                    "exchange": "SMART",
                    "currency": "USD"
                }
            ]),
        )
        .query("symbol", symbol)
        .query("secType", sec_type),
    );
}

fn mock_snapshot_for(server: &TestServer, conid: &str) -> MockHandle {
    server.expect(
        MockExpectation::json(
            "GET",
            "/v1/api/iserver/marketdata/snapshot",
            json!([{ "conid": conid, "31": "100.00" }]),
        )
        .query("conids", conid),
    )
}

fn e2e_trust() -> TrustFacade {
    TrustFacade::new(
        Box::new(SqliteDatabase::new_in_memory()),
        Box::new(IbkrBroker),
    )
}

fn create_ibkr_test_account(
    trust: &mut TrustFacade,
    name: &str,
    deposit: Decimal,
    risk_per_trade_pct: f32,
) -> Account {
    let account = trust
        .create_account_with_profile(
            name,
            "ibkr e2e",
            Environment::Paper,
            dec!(20),
            dec!(10),
            AccountType::Primary,
            None,
            BrokerKind::Ibkr,
            Some("U1234567"),
        )
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            deposit,
            &Currency::USD,
        )
        .unwrap();
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerMonth(100.0),
            "e2e monthly risk cap",
            &RuleLevel::Error,
        )
        .unwrap();
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(risk_per_trade_pct),
            "e2e trade risk cap",
            &RuleLevel::Error,
        )
        .unwrap();
    account
}

fn create_ibkr_test_vehicle(
    trust: &mut TrustFacade,
    symbol: &str,
    category: TradingVehicleCategory,
) -> TradingVehicle {
    trust
        .create_trading_vehicle(symbol, None, &category, "ibkr")
        .unwrap()
}

fn create_ibkr_test_trade(
    trust: &mut TrustFacade,
    account: &Account,
    symbol: &str,
    category: TradingVehicleCategory,
    quantity: i64,
) -> Trade {
    create_ibkr_test_trade_with_prices(
        trust,
        account,
        symbol,
        category,
        quantity,
        (dec!(95), dec!(100), dec!(110)),
    )
}

fn create_ibkr_test_trade_with_prices(
    trust: &mut TrustFacade,
    account: &Account,
    symbol: &str,
    category: TradingVehicleCategory,
    quantity: i64,
    prices: (Decimal, Decimal, Decimal),
) -> Trade {
    let vehicle = create_ibkr_test_vehicle(trust, symbol, category);
    let (stop_price, entry_price, target_price) = prices;
    trust
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity,
                currency: Currency::USD,
                category: TradeCategory::Long,
                thesis: Some("IBKR risk e2e".to_string()),
                sector: None,
                asset_class: Some(category.to_string()),
                context: None,
            },
            stop_price,
            entry_price,
            target_price,
        )
        .unwrap()
}

#[test]
fn trust_facade_rejects_oversized_ibkr_multi_asset_trades_before_broker_io() {
    for (symbol, category) in [
        ("AAPL", TradingVehicleCategory::Stock),
        ("SPY", TradingVehicleCategory::Etf),
        ("9128285M8", TradingVehicleCategory::Bond),
    ] {
        let server = TestServer::start();
        let mut trust = e2e_trust();
        let account =
            create_ibkr_test_account(&mut trust, &format!("risk-{}", category), dec!(100000), 1.0);
        let trade = create_ibkr_test_trade(&mut trust, &account, symbol, category, 600);

        let result = with_mock_gateway(&server, || trust.fund_trade(&trade));

        let error = result.expect_err("oversized trade should fail risk validation");
        assert!(
            error.to_string().contains("Risk per trade exceeded"),
            "unexpected error for {category}: {error}"
        );
        assert_eq!(
            server.request_count(),
            0,
            "risk validation for {category} must reject before broker HTTP I/O"
        );
    }
}

#[test]
fn trust_facade_rejects_invalid_ibkr_multi_asset_risk_geometry_before_broker_io() {
    for (symbol, category) in [
        ("AAPL", TradingVehicleCategory::Stock),
        ("SPY", TradingVehicleCategory::Etf),
        ("9128285M8", TradingVehicleCategory::Bond),
    ] {
        let server = TestServer::start();
        let mut trust = e2e_trust();
        let account = create_ibkr_test_account(
            &mut trust,
            &format!("invalid-risk-{}", category),
            dec!(100000),
            10.0,
        );
        let trade = create_ibkr_test_trade_with_prices(
            &mut trust,
            &account,
            symbol,
            category,
            10,
            (dec!(105), dec!(100), dec!(110)),
        );

        let result = with_mock_gateway(&server, || trust.fund_trade(&trade));

        let error = result.expect_err("invalid stop/entry geometry should fail risk validation");
        assert!(
            error.to_string().contains("Invalid risk setup"),
            "unexpected error for {category}: {error}"
        );
        assert_eq!(
            server.request_count(),
            0,
            "invalid risk geometry for {category} must reject before broker HTTP I/O"
        );
    }
}

#[test]
fn trust_facade_submits_valid_ibkr_stock_etf_and_bond_trades() {
    for (symbol, category, sec_type, conid) in [
        ("AAPL", TradingVehicleCategory::Stock, "STK", "265598"),
        ("SPY", TradingVehicleCategory::Etf, "STK", "756733"),
        ("9128285M8", TradingVehicleCategory::Bond, "BOND", "123456"),
    ] {
        let server = TestServer::start();
        mock_session(&server);
        mock_contract_search_for(&server, symbol, sec_type, conid);

        let mut trust = e2e_trust();
        let account = create_ibkr_test_account(
            &mut trust,
            &format!("submit-{}", category),
            dec!(100000),
            2.0,
        );
        let trade = create_ibkr_test_trade(&mut trust, &account, symbol, category, 10);
        let (funded, _, _, _) = trust.fund_trade(&trade).unwrap();

        let submit = server.expect(
            MockExpectation::json(
                "POST",
                "/v1/api/iserver/account/U1234567/orders",
                json!([
                    {
                        "order_id": "9001",
                        "local_order_id": funded.entry.id.to_string(),
                        "order_status": "Submitted"
                    }
                ]),
            )
            .body_contains(format!("\"secType\":\"{sec_type}\""))
            .body_contains(format!("\"ticker\":\"{symbol}\""))
            .body_contains(funded.entry.id.to_string())
            .body_contains(funded.target.id.to_string())
            .body_contains(funded.safety_stop.id.to_string()),
        );

        with_mock_gateway(&server, || {
            let (submitted, _) = trust.submit_trade(&funded).unwrap();
            assert_eq!(submitted.status, Status::Submitted);
            assert_eq!(
                submitted.entry.broker_order_id,
                Some(funded.entry.id.to_string())
            );
            assert_eq!(
                submitted.target.broker_order_id,
                Some(funded.target.id.to_string())
            );
            assert_eq!(
                submitted.safety_stop.broker_order_id,
                Some(funded.safety_stop.id.to_string())
            );
        });

        submit.assert();
    }
}

#[test]
fn preflight_gateway_checks_authentication_and_account_selection() {
    let server = TestServer::start();
    mock_session(&server);
    let account = account();

    with_mock_gateway(&server, || {
        IbkrBroker::preflight_gateway(&account).expect("preflight should succeed");
    });

    assert_eq!(server.request_count(), 3);
}

#[test]
fn submit_trade_posts_bracket_orders_and_returns_local_order_refs() {
    let server = TestServer::start();
    mock_session(&server);
    mock_contract_search(&server);

    let account = account();
    let mut trade = trade();
    trade.account_id = account.id;
    let submit = server.expect(
        MockExpectation::json(
            "POST",
            "/v1/api/iserver/account/U1234567/orders",
            json!([
                {
                    "order_id": "9001",
                    "local_order_id": trade.entry.id.to_string(),
                    "order_status": "Submitted"
                }
            ]),
        )
        .body_contains(trade.entry.id.to_string())
        .body_contains(trade.target.id.to_string())
        .body_contains(trade.safety_stop.id.to_string())
        .body_contains("\"parentId\""),
    );

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
fn submit_bond_trade_uses_ibkr_bond_security_type() {
    let server = TestServer::start();
    mock_session(&server);
    mock_contract_search_for(&server, "9128285M8", "BOND", "123456");

    let account = account();
    let mut trade = trade();
    trade.account_id = account.id;
    trade.trading_vehicle.symbol = "9128285M8".to_string();
    trade.trading_vehicle.category = TradingVehicleCategory::Bond;

    let submit = server.expect(
        MockExpectation::json(
            "POST",
            "/v1/api/iserver/account/U1234567/orders",
            json!([
                {
                    "order_id": "9002",
                    "local_order_id": trade.entry.id.to_string(),
                    "order_status": "Submitted"
                }
            ]),
        )
        .body_contains("\"secType\":\"BOND\"")
        .body_contains("\"ticker\":\"9128285M8\""),
    );

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        let (_, ids) = broker.submit_trade(&trade, &account).unwrap();
        assert_eq!(ids.entry, trade.entry.id.to_string());
    });

    submit.assert();
}

#[test]
fn preview_trade_posts_snapshot_then_whatif_without_order_submission() {
    let server = TestServer::start();
    mock_session(&server);
    mock_contract_search_for(&server, "SPY", "STK", "756733");
    let snapshot = mock_snapshot_for(&server, "756733");

    let account = account();
    let mut trade = trade();
    trade.account_id = account.id;
    trade.trading_vehicle.symbol = "SPY".to_string();
    trade.trading_vehicle.category = TradingVehicleCategory::Etf;

    let whatif = server.expect(
        MockExpectation::json(
            "POST",
            "/v1/api/iserver/account/U1234567/orders/whatif",
            json!({
                "amount": { "amount": "1000.00 USD", "commission": "1.00 USD", "total": "1001.00 USD" },
                "warn": null,
                "error": null
            }),
        )
        .body_contains("\"orders\"")
        .body_contains("\"secType\":\"STK\"")
        .body_contains("\"ticker\":\"SPY\"")
        .body_contains(trade.entry.id.to_string())
        .body_contains(trade.target.id.to_string())
        .body_contains(trade.safety_stop.id.to_string())
        .body_contains("\"parentId\""),
    );

    with_mock_gateway(&server, || {
        let preview = IbkrBroker::preview_trade(&account, &trade).unwrap();
        assert!(preview.get("amount").is_some());
        assert!(preview.get("error").is_some());
    });

    snapshot.assert();
    whatif.assert();
}

#[test]
fn preview_trade_surfaces_whatif_rejections() {
    let server = TestServer::start();
    mock_session(&server);
    mock_contract_search(&server);
    let snapshot = mock_snapshot_for(&server, "265598");
    let whatif = server.expect(MockExpectation::json(
        "POST",
        "/v1/api/iserver/account/U1234567/orders/whatif",
        json!({ "error": "insufficient margin for preview" }),
    ));

    let account = account();
    let mut trade = trade();
    trade.account_id = account.id;

    with_mock_gateway(&server, || {
        let error = IbkrBroker::preview_trade(&account, &trade).unwrap_err();
        assert!(error.to_string().contains("what-if preview rejected"));
    });

    snapshot.assert();
    whatif.assert();
}

#[test]
fn cancel_trade_resolves_order_id_from_order_ref_before_delete() {
    let server = TestServer::start();
    mock_session(&server);
    let mut trade = trade();
    let account = account();
    trade.account_id = account.id;
    trade.entry.broker_order_id = Some(trade.entry.id.to_string());

    let lookup = server.expect(
        MockExpectation::json(
            "GET",
            "/v1/api/iserver/account/orders",
            json!({
                "orders": [
                    {
                        "orderId": "7001",
                        "order_ref": trade.entry.id.to_string(),
                        "status": "Submitted"
                    }
                ]
            }),
        )
        .query("accountId", "U1234567")
        .query("force", "true"),
    );
    let delete = server.expect(MockExpectation::json(
        "DELETE",
        "/v1/api/iserver/account/U1234567/order/7001",
        json!({ "ok": true }),
    ));

    with_mock_gateway(&server, || {
        let broker = IbkrBroker;
        broker.cancel_trade(&trade, &account).unwrap();
    });

    lookup.assert();
    delete.assert();
}

#[test]
fn modify_stop_uses_order_ref_for_lookup_and_returns_same_ref() {
    let server = TestServer::start();
    mock_session(&server);
    mock_contract_search(&server);
    let mut trade = trade();
    let account = account();
    trade.account_id = account.id;
    trade.entry.broker_order_id = Some(trade.entry.id.to_string());
    trade.safety_stop.broker_order_id = Some(trade.safety_stop.id.to_string());

    let lookup = server.expect(
        MockExpectation::json(
            "GET",
            "/v1/api/iserver/account/orders",
            json!({
                "orders": [
                    {
                        "orderId": "8001",
                        "order_ref": trade.safety_stop.id.to_string(),
                        "status": "Submitted"
                    }
                ]
            }),
        )
        .query("accountId", "U1234567")
        .query("force", "true"),
    );
    let modify = server.expect(
        MockExpectation::json(
            "POST",
            "/v1/api/iserver/account/U1234567/order/8001",
            json!([
                {
                    "order_id": "8001",
                    "local_order_id": trade.safety_stop.id.to_string(),
                    "order_status": "Submitted"
                }
            ]),
        )
        .body_contains(trade.safety_stop.id.to_string())
        .body_contains(trade.entry.id.to_string())
        .body_contains("\"price\":\"94.5\""),
    );

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
    let server = TestServer::start();
    mock_session(&server);
    let mut trade = trade();
    let account = account();
    trade.account_id = account.id;
    trade.entry.broker_order_id = Some(trade.entry.id.to_string());
    trade.target.broker_order_id = Some(trade.target.id.to_string());
    trade.safety_stop.broker_order_id = Some(trade.safety_stop.id.to_string());

    let sync = server.expect(
        MockExpectation::json(
            "GET",
            "/v1/api/iserver/account/orders",
            json!({
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
            }),
        )
        .query("accountId", "U1234567")
        .query("force", "true"),
    );

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
    let server = TestServer::start();
    mock_session(&server);
    mock_contract_search(&server);
    let mut trade = trade();
    let account = account();
    trade.account_id = account.id;
    trade.entry.broker_order_id = Some(trade.entry.id.to_string());
    trade.target.broker_order_id = Some(trade.target.id.to_string());
    trade.safety_stop.broker_order_id = Some(trade.safety_stop.id.to_string());

    let history = server.expect(MockExpectation::json(
        "GET",
        "/v1/api/iserver/marketdata/history",
        json!({
            "data": [
                { "o": "100", "h": "101", "l": "99", "c": "100.5", "v": "1000", "t": 1773830400000i64 }
            ]
        }),
    ));
    let snapshot = server.expect(MockExpectation::json(
        "GET",
        "/v1/api/iserver/marketdata/snapshot",
        json!([
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
        ]),
    ));
    let account_trades = server.expect(MockExpectation::json(
        "GET",
        "/v1/api/iserver/account/trades",
        json!([
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
        ]),
    ));

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
