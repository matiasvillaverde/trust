use axum::{
    body::Bytes,
    extract::State,
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json, Router,
};
pub mod risk;
mod schema;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
use trust_service::auth::SCOPES;

pub const SENTINEL: &str = "provider-private-token-MUST-NOT-APPEAR";
pub struct BrokerState {
    pub submissions: AtomicUsize,
    pub modifications: AtomicUsize,
    pub hold: AtomicBool,
    pub release: Semaphore,
    pub orders: Mutex<Vec<Value>>,
    pub fail_activities: AtomicBool,
    pub fail_snapshot: AtomicBool,
    pub ledger: Mutex<Option<Value>>,
    pub positions: Mutex<Option<Value>>,
    pub summary: Mutex<Option<Value>>,
    pub quote: Mutex<Option<Value>>,
    pub history: Mutex<Option<Value>>,
}
impl Default for BrokerState {
    fn default() -> Self {
        Self {
            submissions: AtomicUsize::new(0),
            modifications: AtomicUsize::new(0),
            hold: AtomicBool::new(false),
            release: Semaphore::new(0),
            orders: Mutex::new(Vec::new()),
            fail_activities: AtomicBool::new(false),
            fail_snapshot: AtomicBool::new(false),
            ledger: Mutex::new(None),
            positions: Mutex::new(None),
            summary: Mutex::new(None),
            quote: Mutex::new(None),
            history: Mutex::new(None),
        }
    }
}
impl BrokerState {
    pub fn fills(&self, entry: &str, exit: &str, stop: bool) {
        for order in self.orders.lock().unwrap().iter_mut() {
            let leg = order["leg"].as_str().unwrap().to_owned();
            let filled = if leg == "trust-entry" {
                entry
            } else if leg == if stop { "trust-stop" } else { "trust-target" } {
                exit
            } else {
                "0"
            };
            order["filledQuantity"] = json!(filled);
            order["avgPrice"] = order["price"].clone();
            order["status"] = json!(if filled == "0" && exit == "10" {
                "Cancelled"
            } else if filled == "0" {
                "Submitted"
            } else if filled == "10" {
                "Filled"
            } else {
                "partially_filled"
            });
        }
    }
}
pub struct Gateway {
    pub url: String,
    pub state: Arc<BrokerState>,
    task: tokio::task::JoinHandle<()>,
}
impl Gateway {
    pub async fn start() -> Self {
        let state = Arc::new(BrokerState {
            release: Semaphore::new(0),
            ..Default::default()
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new().fallback(gateway).with_state(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { url, state, task }
    }
}
impl Drop for Gateway {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn gateway(
    State(state): State<Arc<BrokerState>>,
    method: Method,
    uri: Uri,
    body: Bytes,
) -> Response {
    let data: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if uri.path().starts_with("/portfolio/") && state.fail_snapshot.load(Ordering::SeqCst) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error":"unavailable"})),
        )
            .into_response();
    }
    let value = match (method.as_str(), uri.path()) {
        ("GET", "/iserver/auth/status") => {
            json!({"authenticated":true,"connected":true,"established":true})
        }
        ("GET", "/iserver/accounts") => {
            json!({"isPaper":true,"accounts":["DU_TEST"],"selectedAccount":"DU_TEST"})
        }
        ("POST", "/iserver/account") => json!({"acctId":"DU_TEST"}),
        ("GET", "/portfolio/accounts") => json!([{"id":"DU_TEST","accountId":"DU_TEST"}]),
        ("GET", "/portfolio/DU_TEST/summary") => state.summary.lock().unwrap().clone().unwrap_or_else(risk::summary),
        ("GET", "/portfolio/DU_TEST/ledger") => state.ledger.lock().unwrap().clone().unwrap_or_else(||json!({"BASE":{"acctcode":"DU_TEST","netliquidationvalue":"10000","cashbalance":"10000","realizedpnl":"0","unrealizedpnl":"0","timestamp":chrono::Utc::now().timestamp()}})),
        ("GET", "/portfolio/DU_TEST/positions/0") => {
            if let Some(positions)=state.positions.lock().unwrap().clone() {return Json(positions).into_response();}
            use rust_decimal::Decimal;
            let orders=state.orders.lock().unwrap();
            let mut quantity=Decimal::ZERO;
            for order in orders.iter() {
                let filled=Decimal::from_str_exact(order["filledQuantity"].as_str().unwrap()).unwrap();
                quantity+=if order["side"]=="BUY" {filled} else {-filled};
            }
            if quantity.is_zero() {json!([])} else {json!([{"acctId":"DU_TEST","conid":"1234","contractDesc":"TEST","currency":"USD","assetClass":"STK","position":quantity.to_string(),"mktValue":(quantity*Decimal::new(100,0)).to_string()}])}
        }
        ("GET", "/iserver/secdef/search") => {
            json!([{"symbol":"TEST","conid":"1234","secType":"STK","currency":"USD","exchange":"SMART"}])
        }
        // Deterministic, timestamped admission evidence from the simulated gateway.
        ("GET", "/iserver/marketdata/snapshot") => state.quote.lock().unwrap().clone().unwrap_or_else(risk::quote),
        ("GET", "/iserver/marketdata/history") => state.history.lock().unwrap().clone().unwrap_or_else(risk::history),
        ("POST", "/iserver/account/DU_TEST/orders/whatif") => {
            assert_eq!(data["orders"].as_array().unwrap().len(), 3);
            json!({"amount":{"amount":"500.00 USD","commission":"1.00 USD","total":"501.00 USD"},"initial":{"change":"500.00 USD"},"maintenance":{"change":"250.00 USD"}})
        }
        ("GET", "/iserver/account/orders") => {
            if uri
                .query()
                .is_some_and(|query| query.split('&').any(|part| part == "force=true"))
            {
                json!([])
            } else {
                json!({"snapshot":true,"orders":state.orders.lock().unwrap().clone()})
            }
        }
        ("POST", "/iserver/account/DU_TEST/orders") => {
            assert_eq!(data["orders"].as_array().unwrap().len(), 3);
            let orders = data["orders"].as_array().unwrap().iter().enumerate().map(|(i, order)| {
                assert_eq!(order["acctId"], "DU_TEST");
                json!({"orderId":i+1,"order_ref":order["cOID"],"leg":order["referrer"],"price":order["price"],"filledQuantity":"0","status":"Submitted","acct":"DU_TEST","conid":"1234","ticker":"TEST","currency":"USD","secType":"STK","side":order["side"],"totalSize":order["quantity"],"orderType":order["orderType"]})
            }).collect();
            *state.orders.lock().unwrap() = orders;
            state.submissions.fetch_add(1, Ordering::SeqCst);
            if state.hold.load(Ordering::SeqCst) {
                state.release.acquire().await.unwrap().forget();
            }
            json!([{"order_id":"1","order_status":"Submitted"}])
        }
        ("POST", path) if path.starts_with("/iserver/account/DU_TEST/order/") => {
            let id = path.rsplit('/').next().unwrap().parse::<u64>().unwrap();
            let mut orders = state.orders.lock().unwrap();
            let order = orders
                .iter_mut()
                .find(|order| order["orderId"] == id)
                .unwrap();
            assert_eq!(data["acctId"], "DU_TEST");
            assert_eq!(data["quantity"], 10);
            if data["orderType"] == "STP" {
                assert_eq!(order["leg"], "trust-stop");
                order["price"] = data["price"].clone();
            } else {
                assert_eq!(data["orderType"], "MKT");
                assert_eq!(order["leg"], "trust-target");
            }
            order["orderType"] = data["orderType"].clone();
            state.modifications.fetch_add(1, Ordering::SeqCst);
            json!([{"order_id":id.to_string(),"order_status":"Submitted"}])
        }
        ("GET", "/iserver/account/trades") => {
            if state.fail_activities.load(Ordering::SeqCst) {
                return (StatusCode::BAD_GATEWAY, SENTINEL).into_response();
            }
            json!([])
        }
        _ => {
            return (
                StatusCode::NOT_FOUND,
                format!("Unexpected simulator route: {method} {uri}"),
            )
                .into_response()
        }
    };
    Json(value).into_response()
}

pub struct Fixture {
    pub dir: tempfile::TempDir,
    config: Value,
    key: EncodingKey,
    pub client: reqwest::Client,
}
impl Fixture {
    pub fn new(timeout_ms: u64, queue: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(
            dir.path(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        let rsa = openssl::rsa::Rsa::generate(2048).unwrap();
        let key = EncodingKey::from_rsa_pem(&rsa.private_key_to_pem().unwrap()).unwrap();
        let jwks = dir.path().join("jwks.json");
        std::fs::write(&jwks, json!({"keys":[{"kty":"RSA","alg":"RS256","use":"sig","kid":"e2e","n":URL_SAFE_NO_PAD.encode(rsa.n().to_vec()),"e":URL_SAFE_NO_PAD.encode(rsa.e().to_vec())}]}).to_string()).unwrap();
        let config = json!({"listen":"127.0.0.1:0","public_url":"http://localhost:8000","issuer":"http://localhost:9999/issuer","jwks_file":jwks,"database":dir.path().join("financial.db"),"journal":dir.path().join("control.db"),"reconcile_interval_seconds":0,"request_timeout_ms":timeout_ms,"queue_timeout_ms":timeout_ms,"queue_capacity":queue,"grants":{"owner":{"role":"owner","all_accounts":true,"scopes":SCOPES},"agent":{"role":"agent","all_accounts":true,"scopes":["trust:read","trust:preview","trust:trade","trust:reconcile","trust:journal"]},"reader":{"all_accounts":true,"scopes":["trust:read"]}}});
        std::fs::write(dir.path().join("config.json"), config.to_string()).unwrap();
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        Self {
            dir,
            config,
            key,
            client,
        }
    }
    pub fn add_agent(&mut self, subject: &str) {
        self.config["grants"][subject] = self.config["grants"]["agent"].clone();
        std::fs::write(self.dir.path().join("config.json"), self.config.to_string()).unwrap();
    }
    pub fn token(&self, subject: &str) -> String {
        let now = chrono::Utc::now().timestamp();
        let claims = json!({"iss":self.config["issuer"],"aud":"http://localhost:8000/mcp","sub":subject,"iat":now,"exp":now+900,"auth_time":now,"acr":"urn:trust:owner:mfa","scope":if subject=="owner" || subject.starts_with("agent"){SCOPES.join(" ")}else{"trust:read".into()},"client_id":"e2e-agent"});
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("e2e".into());
        encode(&header, &claims, &self.key).unwrap()
    }
    pub fn enable_encryption(&mut self) {
        let data = self.dir.path().join("data");
        let secrets = self.dir.path().join("secrets");
        trust_security::private_directory(&data).unwrap();
        trust_security::private_directory(&secrets).unwrap();
        self.config["database"] = json!(data.join("financial.db"));
        self.config["journal"] = json!(data.join("control.db"));
        let mut keys = serde_json::Map::new();
        for name in ["financial", "journal", "backup"] {
            let path = secrets.join(format!("{name}-key.hex"));
            trust_security::DatabaseKey::generate()
                .unwrap()
                .write_new(&path)
                .unwrap();
            keys.insert(format!("{name}_key_file"), json!(path));
        }
        self.config["storage_encryption"] = json!(keys);
        std::fs::write(self.dir.path().join("config.json"), self.config.to_string()).unwrap();
    }
    pub fn spawn(&self, gateway: &Gateway) -> Process {
        let mut child = Command::new(env!("CARGO_BIN_EXE_trust-service"))
            .arg(self.dir.path().join("config.json"))
            .env("TRUST_IBKR_URL", &gateway.url)
            .env("TRUST_IBKR_ALLOW_INSECURE_TLS", "true")
            .env_remove("TRUST_VERIFY_PROJECTION_RECALC")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let logs = Arc::new(Mutex::new(Vec::new()));
        let mut readers = Vec::new();
        for stream in [
            Box::new(child.stdout.take().unwrap()) as Box<dyn std::io::Read + Send>,
            Box::new(child.stderr.take().unwrap()),
        ] {
            let logs = logs.clone();
            readers.push(std::thread::spawn(move || {
                for line in BufReader::new(stream).lines() {
                    let line = line.unwrap();
                    let mut logs = logs.lock().unwrap();
                    assert!(logs.len() < 10000, "unbounded test process logging");
                    logs.push(line);
                }
            }));
        }
        Process {
            child,
            logs,
            readers,
            url: String::new(),
        }
    }
    pub async fn start(&self, gateway: &Gateway) -> Process {
        let mut process = self.spawn(gateway);
        let start = Instant::now();
        loop {
            let address = process
                .logs
                .lock()
                .unwrap()
                .iter()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .find_map(|v| {
                    if v["fields"]["event"] == "service_ready" {
                        v["fields"]["listen"].as_str().map(str::to_owned)
                    } else {
                        None
                    }
                });
            if let Some(address) = address {
                process.url = format!("http://{address}");
                break;
            }
            assert!(
                process.child.try_wait().unwrap().is_none(),
                "process failed: {:?}",
                process.logs.lock().unwrap()
            );
            assert!(start.elapsed() < Duration::from_secs(10), "startup timeout");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        process
    }
    pub fn request(
        &self,
        process: &Process,
        path: &str,
        subject: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let request = self
            .client
            .get(format!("{}{path}", process.url))
            .header("host", "localhost:8000");
        match subject {
            Some(subject) => request.bearer_auth(self.token(subject)),
            None => request,
        }
    }
    pub async fn call(&self, process: &Process, name: &str, args: Value, mcp: bool) -> Value {
        self.try_call(process, name, args, mcp).await.unwrap()
    }
    pub async fn try_call(
        &self,
        process: &Process,
        name: &str,
        args: Value,
        mcp: bool,
    ) -> Result<Value, reqwest::Error> {
        self.call_as(process, "owner", name, args, mcp).await
    }
    pub async fn call_as(
        &self,
        process: &Process,
        subject: &str,
        name: &str,
        args: Value,
        mcp: bool,
    ) -> Result<Value, reqwest::Error> {
        let (path, body) = if mcp {
            (
                "/mcp".to_owned(),
                json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),
            )
        } else {
            (format!("/v1/tools/{name}"), args)
        };
        let response = self
            .client
            .post(format!("{}{path}", process.url))
            .header("host", "localhost:8000")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-11-25")
            .bearer_auth(self.token(subject))
            .json(&body)
            .send()
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        let request_id = response.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let value: Value = response.json().await?;
        let reply = if mcp {
            value["result"]["structuredContent"].clone()
        } else {
            value
        };
        assert_eq!(reply["request_id"], request_id, "{reply}");
        schema::validate(name, &reply);
        Ok(reply)
    }
    pub async fn ok(&self, p: &Process, name: &str, args: Value) -> Value {
        let reply = self.call(p, name, args, true).await;
        assert!(reply["error"].is_null(), "{name}: {reply}");
        reply["data"].clone()
    }
    pub async fn seed(&self, p: &Process, side: &str, qty: &str, suffix: &str) -> (Value, Value) {
        let account=self.ok(p,"account_create",json!({"name":format!("account-{suffix}"),"description":"E2E","environment":"paper","broker":"ibkr","broker_account_id":"DU_TEST","idempotency_key":format!("account-{suffix}")})).await["id"].clone();
        self.ok(p,"capital_deposit",json!({"account_id":account,"amount":"10000","idempotency_key":format!("deposit-{suffix}")})).await;
        for (kind, pct) in [("per_trade", "1"), ("per_month", "5")] {
            self.ok(p,"risk_rule_create",json!({"account_id":account,"kind":kind,"percentage":pct,"priority":"error","description":"E2E risk","idempotency_key":format!("risk-{kind}-{suffix}")})).await;
        }
        self.ok(p,"account_safety_policy_set",json!({"account_id":account,"currency":"USD","max_gross_notional":"100000","max_order_notional":"10000","max_daily_loss":"500","max_drawdown":"1000","max_snapshot_age_seconds":120,"controls":risk::controls(),"idempotency_key":format!("safety-{suffix}")})).await;
        let instrument=self.ok(p,"instrument_create",json!({"symbol":"TEST","category":"stock","broker":"ibkr","idempotency_key":format!("instrument-{suffix}")})).await["id"].clone();
        self.ok(p,"instrument_update",json!({"instrument_id":instrument,"broker_asset_id":"1234","idempotency_key":format!("instrument-native-{suffix}")})).await;
        self.ok(p,"agent_mandate_set",json!({"account_id":account,"subject":"agent","instrument_ids":[instrument],"max_order_notional":"10000","max_total_notional":"100000","max_open_trades":100,"allow_short":true,"risk_budget":risk::budget(),"expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339(),"idempotency_key":format!("mandate-{suffix}")})).await;
        let trade=self.ok(p,"trade_prepare",json!({"account_id":account,"instrument_id":instrument,"side":side,"quantity":qty,"entry":"100","stop":if side=="long"{"95"}else{"105"},"target":if side=="long"{"110"}else{"90"},"idempotency_key":format!("trade-{suffix}")})).await["id"].clone();
        (account, trade)
    }
    pub fn journal(&self) -> rusqlite::Connection {
        let connection =
            rusqlite::Connection::open(self.config["journal"].as_str().unwrap()).unwrap();
        if let Some(path) = self.config["storage_encryption"]["journal_key_file"].as_str() {
            let key = trust_security::DatabaseKey::load(std::path::Path::new(path)).unwrap();
            connection
                .execute_batch(&format!("PRAGMA key = {};", key.sql_literal().as_str()))
                .unwrap();
        }
        connection
    }
    pub async fn accepted(&self, gateway: &Gateway) {
        let start = Instant::now();
        while gateway.state.submissions.load(Ordering::SeqCst) == 0 {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "submission did not reach simulator"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}
pub struct Process {
    pub child: Child,
    pub logs: Arc<Mutex<Vec<String>>>,
    readers: Vec<std::thread::JoinHandle<()>>,
    pub url: String,
}
impl Process {
    pub async fn wait(&mut self) -> std::process::ExitStatus {
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                for reader in self.readers.drain(..) {
                    reader.join().unwrap();
                }
                return status;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "shutdown timeout"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    pub fn signal(&self) {
        assert!(Command::new("kill")
            .arg("-TERM")
            .arg(self.child.id().to_string())
            .status()
            .unwrap()
            .success());
    }
    pub async fn stop(&mut self) {
        self.signal();
        assert!(self.wait().await.success());
    }
    pub fn log_text(&self) -> String {
        self.logs.lock().unwrap().join("\n")
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for reader in self.readers.drain(..) {
            let _ = reader.join();
        }
    }
}
