use axum::{
    body::Bytes,
    extract::State,
    http::{Method, StatusCode, Uri},
    response::{IntoResponse, Response},
    Json, Router,
};
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
    pub hold: AtomicBool,
    pub release: Semaphore,
    pub orders: Mutex<Vec<Value>>,
    pub fail_activities: AtomicBool,
}
impl Default for BrokerState {
    fn default() -> Self {
        Self {
            submissions: AtomicUsize::new(0),
            hold: AtomicBool::new(false),
            release: Semaphore::new(0),
            orders: Mutex::new(Vec::new()),
            fail_activities: AtomicBool::new(false),
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
            order["status"] = json!(if filled == "0" {
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
    let value = match (method.as_str(), uri.path()) {
        ("GET", "/iserver/auth/status") => {
            json!({"authenticated":true,"connected":true,"established":true})
        }
        ("GET", "/iserver/accounts") => {
            json!({"isPaper":true,"accounts":["DU_TEST"],"selectedAccount":"DU_TEST"})
        }
        ("POST", "/iserver/account") => json!({"acctId":"DU_TEST"}),
        ("GET", "/iserver/secdef/search") => {
            json!([{"symbol":"TEST","conid":"1234","secType":"STK","currency":"USD","exchange":"SMART"}])
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
                json!({"orderId":i+1,"order_ref":order["cOID"],"leg":order["referrer"],"price":order["price"],"filledQuantity":"0","status":"Submitted"})
            }).collect();
            *state.orders.lock().unwrap() = orders;
            state.submissions.fetch_add(1, Ordering::SeqCst);
            if state.hold.load(Ordering::SeqCst) {
                state.release.acquire().await.unwrap().forget();
            }
            json!([{"order_id":"1","order_status":"Submitted"}])
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
        let rsa = openssl::rsa::Rsa::generate(2048).unwrap();
        let key = EncodingKey::from_rsa_pem(&rsa.private_key_to_pem().unwrap()).unwrap();
        let jwks = dir.path().join("jwks.json");
        std::fs::write(&jwks, json!({"keys":[{"kty":"RSA","alg":"RS256","use":"sig","kid":"e2e","n":URL_SAFE_NO_PAD.encode(rsa.n().to_vec()),"e":URL_SAFE_NO_PAD.encode(rsa.e().to_vec())}]}).to_string()).unwrap();
        let config = json!({"listen":"127.0.0.1:0","public_url":"http://localhost:8000","issuer":"http://localhost:9999/issuer","jwks_file":jwks,"database":dir.path().join("financial.db"),"journal":dir.path().join("control.db"),"reconcile_interval_seconds":0,"request_timeout_ms":timeout_ms,"queue_timeout_ms":timeout_ms,"queue_capacity":queue,"grants":{"owner":{"all_accounts":true,"scopes":SCOPES},"reader":{"all_accounts":true,"scopes":["trust:read"]}}});
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
    pub fn token(&self, subject: &str) -> String {
        let now = chrono::Utc::now().timestamp();
        let claims = json!({"iss":self.config["issuer"],"aud":"http://localhost:8000/mcp","sub":subject,"iat":now,"exp":now+900,"scope":if subject=="owner"{SCOPES.join(" ")}else{"trust:read".into()},"client_id":"e2e-agent"});
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("e2e".into());
        encode(&header, &claims, &self.key).unwrap()
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
            .bearer_auth(self.token("owner"))
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
        let instrument=self.ok(p,"instrument_create",json!({"symbol":"TEST","category":"stock","broker":"ibkr","idempotency_key":format!("instrument-{suffix}")})).await["id"].clone();
        let trade=self.ok(p,"trade_prepare",json!({"account_id":account,"instrument_id":instrument,"side":side,"quantity":qty,"entry":"100","stop":if side=="long"{"95"}else{"105"},"target":if side=="long"{"110"}else{"90"},"idempotency_key":format!("trade-{suffix}")})).await["id"].clone();
        (account, trade)
    }
    pub fn journal(&self) -> rusqlite::Connection {
        rusqlite::Connection::open(self.dir.path().join("control.db")).unwrap()
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
