#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
#[path = "service_contract/manual_tools.rs"]
mod manual_tools;
#[path = "support/risk.rs"]
mod risk;
#[path = "support/schema.rs"]
mod schema;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use model::{Account, Broker, BrokerKind, BrokerLog, Order, OrderIds, Status, Trade};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
};
use tower::ServiceExt;
use trust_service::{
    auth::{Authenticator, Grant, Identity, Role, SCOPES},
    config::Config,
    journal::{Claim, Journal, Reply},
    runtime::{Service, Worker},
};
use uuid::Uuid;

fn key() -> &'static (Vec<u8>, Value) {
    static KEY: OnceLock<(Vec<u8>, Value)> = OnceLock::new();
    KEY.get_or_init(||{let rsa=openssl::rsa::Rsa::generate(2048).unwrap();let jwk=json!({"kty":"RSA","use":"sig","alg":"RS256","kid":"test-key","n":URL_SAFE_NO_PAD.encode(rsa.n().to_vec()),"e":URL_SAFE_NO_PAD.encode(rsa.e().to_vec())});(rsa.private_key_to_pem().unwrap(),json!({"keys":[jwk]}))})
}

#[test]
fn pinned_jwks_rejects_ambiguous_private_and_invalid_verification_material() {
    let f = Fixture::new();
    for (field, value) in [
        ("kid", json!("")),
        ("kid", json!("x".repeat(257))),
        ("kid", json!("bad\nkey")),
        ("d", json!("private-key-marker")),
        ("k", json!("symmetric-key-marker")),
        ("n", json!(URL_SAFE_NO_PAD.encode([1u8; 128]))),
        ("n", json!(URL_SAFE_NO_PAD.encode([1u8; 1025]))),
        ("n", json!("invalid%encoding")),
        ("n", json!(URL_SAFE_NO_PAD.encode([0u8; 256]))),
        ("e", json!("Ag")),
        ("e", json!("AAEAAQ")),
        ("e", json!(URL_SAFE_NO_PAD.encode([255u8; 9]))),
        ("e", json!("invalid%encoding")),
    ] {
        let mut jwk = key().1["keys"][0].clone();
        jwk[field] = value;
        std::fs::write(&f.config.jwks_file, json!({"keys":[jwk]}).to_string()).unwrap();
        let error = Authenticator::new(f.config.clone()).err().unwrap();
        assert_eq!(error.code, "invalid_argument", "{field}");
        assert!(!error.to_string().contains("key-marker"));
    }
    let jwk = key().1["keys"][0].clone();
    std::fs::write(
        &f.config.jwks_file,
        json!({"keys":[jwk.clone(),jwk]}).to_string(),
    )
    .unwrap();
    assert!(Authenticator::new(f.config.clone()).is_err());
}

#[test]
fn jwt_keys_are_bound_to_signature_use_algorithm_and_verification_operation() {
    let f = Fixture::new();
    for (field, value) in [
        ("use", json!("enc")),
        ("alg", json!("RS384")),
        ("key_ops", json!(["encrypt"])),
        ("key_ops", json!(["verify", "verify"])),
        ("key_ops", json!(["verify", "decrypt"])),
        ("key_ops", json!([])),
    ] {
        let mut excluded = key().1["keys"][0].clone();
        excluded["kid"] = json!("excluded");
        excluded[field] = value;
        std::fs::write(
            &f.config.jwks_file,
            json!({"keys":[key().1["keys"][0],excluded.clone()]}).to_string(),
        )
        .unwrap();
        let auth = Authenticator::new(f.config.clone()).unwrap();
        assert!(auth.authenticate(&f.token()).is_ok());
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("excluded".into());
        let token = encode(
            &header,
            &f.claims(),
            &EncodingKey::from_rsa_pem(&key().0).unwrap(),
        )
        .unwrap();
        assert!(auth.authenticate(&token).is_err(), "{field}");
        std::fs::write(&f.config.jwks_file, json!({"keys":[excluded]}).to_string()).unwrap();
        assert!(Authenticator::new(f.config.clone()).is_err());
    }
    let mut jwk = key().1["keys"][0].clone();
    jwk["key_ops"] = json!(["verify"]);
    std::fs::write(&f.config.jwks_file, json!({"keys":[jwk]}).to_string()).unwrap();
    assert!(Authenticator::new(f.config.clone())
        .unwrap()
        .authenticate(&f.token())
        .is_ok());
}

#[test]
fn valid_signature_does_not_accept_unsupported_critical_or_encrypted_headers() {
    let f = Fixture::new();
    let auth = Authenticator::new(f.config.clone()).unwrap();
    for (field, value) in [
        ("crit", json!(["unknown-extension"])),
        ("crit", json!([])),
        ("zip", json!("DEF")),
        ("enc", json!("A128GCM")),
        ("b64", json!(false)),
    ] {
        let mut header = serde_json::to_value(Header::new(Algorithm::RS256)).unwrap();
        header["kid"] = json!("test-key");
        header[field] = value;
        let header: Header = serde_json::from_value(header).unwrap();
        let token = encode(
            &header,
            &f.claims(),
            &EncodingKey::from_rsa_pem(&key().0).unwrap(),
        )
        .unwrap();
        assert!(auth.authenticate(&token).is_err(), "{field}");
    }
    // Public verification material is pinned at startup; claims and MFA still
    // pass the existing per-request validation below, without token-result caching.
    std::fs::write(&f.config.jwks_file, "{}").unwrap();
    assert!(auth.authenticate(&f.token()).is_ok());
    let mut expired = f.claims();
    expired["exp"] = json!(1);
    assert!(auth.authenticate(&f.sign(expired)).is_err());
}

#[tokio::test]
async fn invalid_http_bodies_are_structured_sanitized_and_never_dispatched() {
    let f = Fixture::new();
    let (service, worker) = f.start();
    let app = trust_service::http::router(f.config.clone(), service.clone()).unwrap();
    let token = f.token();
    for (content_type, body, declared, expected) in [
        (
            "application/json",
            "{\"private-marker\":".to_owned(),
            false,
            StatusCode::BAD_REQUEST,
        ),
        (
            "text/plain",
            "private-marker".into(),
            false,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
        ),
        (
            "application/json",
            "x".repeat(131073),
            false,
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
        (
            "application/json",
            "x".repeat(131073),
            true,
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        let mut request = Request::post("/v1/tools/capital_deposit")
            .header("host", "localhost:8000")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", content_type);
        if declared {
            request = request.header("content-length", body.len());
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        let request_id = response.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .to_owned();
        let bytes = to_bytes(response.into_body(), 10000).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["request_id"], request_id);
        assert_eq!(value["error"]["code"], "invalid_argument");
        assert_eq!(value["error"]["retryable"], false);
        assert!(value["operation_id"].is_null());
        assert!(!String::from_utf8_lossy(&bytes).contains("private-marker"));
    }
    assert_eq!(service.metrics.completed.load(Ordering::Relaxed), 0);
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn mcp_catalogues_reject_invalid_cursors_and_rate_limits_explain_safe_retry() {
    let f = Fixture::new();
    let (service, worker) = f.start();
    let app = trust_service::http::router(f.config.clone(), service.clone()).unwrap();
    let token = f.token();
    for method in ["tools/list", "resources/list", "prompts/list"] {
        let (status, _, value) = http(
            app.clone(),
            Some(&token),
            "/mcp",
            Some(json!({"jsonrpc":"2.0","id":1,"method":method,"params":{"cursor":"invalid"}})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["error"]["code"], -32602);
    }
    for _ in 3..120 {
        let (status, _, _) = http(
            app.clone(),
            Some(&token),
            "/mcp",
            Some(json!({"jsonrpc":"2.0","id":1,"method":"ping"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, headers, value) = http(
        app.clone(),
        Some(&token),
        "/mcp",
        Some(json!({"jsonrpc":"2.0","id":1,"method":"ping"})),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(headers["retry-after"], "60");
    assert_eq!(value["error"]["retryable"], true);
    assert!(value["operation_id"].is_null());
    assert_eq!(http(app, None, "/readyz", None).await.0, StatusCode::OK);
    assert_eq!(service.metrics.completed.load(Ordering::Relaxed), 0);
    worker.shutdown().await.unwrap();
}
struct Fixture {
    _directory: tempfile::TempDir,
    config: Arc<Config>,
    calls: Arc<AtomicUsize>,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(
            directory.path(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        let jwks = directory.path().join("jwks.json");
        std::fs::write(&jwks, key().1.to_string()).unwrap();
        let config:Config=serde_json::from_value(json!({"listen":"127.0.0.1:0","public_url":"http://localhost:8000","issuer":"http://localhost:9999/issuer","jwks_file":jwks,"database":directory.path().join("financial.db"),"journal":directory.path().join("control.db"),"reconcile_interval_seconds":0,"grants":{"owner":{"role":"owner","all_accounts":true,"scopes":SCOPES}}})).unwrap();
        config.validate().unwrap();
        Self {
            _directory: directory,
            config: Arc::new(config),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn claims(&self) -> Value {
        let now = chrono::Utc::now().timestamp();
        json!({"iss":self.config.issuer,"aud":self.config.resource(),"sub":"owner","iat":now,"exp":now+900,"auth_time":now,"acr":"urn:trust:owner:mfa","scope":SCOPES.join(" "),"client_id":"test-agent"})
    }
    fn sign(&self, claims: Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".into());
        encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_pem(&key().0).unwrap(),
        )
        .unwrap()
    }
    fn token(&self) -> String {
        self.sign(self.claims())
    }
    fn identity(&self) -> Identity {
        Authenticator::new(self.config.clone())
            .unwrap()
            .authenticate(&self.token())
            .unwrap()
    }
    fn start(&self) -> (Service, Worker) {
        let database = self.config.database.clone();
        let calls = self.calls.clone();
        Service::start(self.config.clone(), move || {
            Ok(trust_core::TrustFacade::new(
                Box::new(db_sqlite::SqliteDatabase::new(database.to_str().unwrap())),
                Box::new(UncertainBroker(calls)),
            ))
        })
        .unwrap()
    }
    async fn call(&self, service: &Service, name: &str, args: Value) -> Reply {
        let reply = service
            .call(self.identity(), name.into(), args, Uuid::new_v4())
            .await;
        schema::validate(name, &serde_json::to_value(&reply).unwrap());
        reply
    }
    async fn account(&self, service: &Service, name: &str) -> Uuid {
        let reply=self.call(service,"account_create",json!({"name":name,"description":"service contract","environment":"paper","broker":"ibkr","broker_account_id":"DU_TEST","idempotency_key":format!("create-{name}")})).await;
        assert!(reply.error.is_none(), "{reply:?}");
        let id = reply.data.unwrap()["id"].as_str().unwrap().parse().unwrap();
        let policy=self.call(service,"account_safety_policy_set",json!({"account_id":id,"currency":"USD","max_gross_notional":"100000","max_order_notional":"10000","max_daily_loss":"500","max_drawdown":"1000","max_snapshot_age_seconds":120,"controls":risk::controls(),"idempotency_key":format!("safety-{name}")})).await;
        assert!(policy.error.is_none(), "{policy:?}");
        id
    }
}
struct UncertainBroker(Arc<AtomicUsize>);

#[tokio::test]
async fn metrics_distinguish_replay_denial_and_domain_execution() {
    let f = Fixture::new();
    let (service, worker) = f.start();
    let account = f.account(&service, "metrics").await;
    let args = json!({"account_id":account,"amount":"100","idempotency_key":"metrics-deposit"});
    let original = f.call(&service, "capital_deposit", args.clone()).await;
    assert!(original.error.is_none());
    let replay = f.call(&service, "capital_deposit", args.clone()).await;
    assert_eq!(original.operation_id, replay.operation_id);
    let mut denied = f.identity();
    denied.scopes.clear();
    let denied = service
        .call(denied, "capital_deposit".into(), args, Uuid::new_v4())
        .await;
    assert_eq!(denied.error.unwrap().code, "forbidden");
    assert_eq!(
        f.call(&service, "caller-secret-tool", json!({}))
            .await
            .error
            .unwrap()
            .code,
        "unknown_tool"
    );
    assert!(f
        .call(&service, "service_status", json!({}))
        .await
        .error
        .is_none());
    let metrics = service.prometheus();
    for expected in [
        "trust_operations_total 7\n",
        "trust_replays_total 1\n",
        "trust_stage_duration_seconds_count{stage=\"domain\"} 4\n",
        "trust_stage_duration_seconds_count{stage=\"journal_claim\"} 4\n",
        "trust_stage_duration_seconds_count{stage=\"journal_finish\"} 6\n",
        "trust_tool_duration_seconds_count{tool=\"capital_deposit\"} 3\n",
        "trust_tool_errors_total{tool=\"capital_deposit\"} 1\n",
    ] {
        assert!(metrics.contains(expected), "missing {expected}");
    }
    assert!(!metrics.contains("caller-secret"));
    assert!(!metrics.contains("metrics-deposit"));
    let journal = rusqlite::Connection::open(&f.config.journal).unwrap();
    let deposits: i64 = journal
        .query_row(
            "SELECT COUNT(*) FROM operations WHERE tool='capital_deposit'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(deposits, 1);
    assert!(f
        .call(&service, "audit_verify", json!({}))
        .await
        .error
        .is_none());
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn monitoring_scope_cannot_read_finances_or_mutate_and_requires_global_grant() {
    for global in [false, true] {
        let mut f = Fixture::new();
        let mut config = (*f.config).clone();
        config.grants.insert(
            "monitor".into(),
            Grant {
                role: Role::Agent,
                all_accounts: global,
                accounts: [Uuid::new_v4()].into_iter().collect(),
                scopes: ["trust:observe".into()].into_iter().collect(),
                allow_live: false,
            },
        );
        f.config = Arc::new(config);
        let (service, worker) = f.start();
        let app = trust_service::http::router(f.config.clone(), service.clone()).unwrap();
        let mut claims = f.claims();
        claims["sub"] = json!("monitor");
        claims.as_object_mut().unwrap().remove("auth_time");
        claims.as_object_mut().unwrap().remove("acr");
        // Extra issuer scopes cannot exceed the configured monitoring-only grant.
        let token = f.sign(claims.clone());
        let (status, _, _) = http(app.clone(), Some(&token), "/metrics", None).await;
        assert_eq!(
            status,
            if global {
                StatusCode::OK
            } else {
                StatusCode::FORBIDDEN
            }
        );
        claims["scope"] = json!("trust:read");
        let missing_scope = f.sign(claims);
        assert_eq!(
            http(app.clone(), Some(&missing_scope), "/metrics", None)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        let (_, _, tools) = http(app.clone(), Some(&token), "/v1/tools", None).await;
        assert!(tools["tools"].as_array().unwrap().is_empty());
        for tool in trust_service::catalog::tools() {
            let identity = Authenticator::new(f.config.clone())
                .unwrap()
                .authenticate(&token)
                .unwrap();
            assert!(
                !identity.permits(tool.scope),
                "monitor permitted {}",
                tool.name
            );
        }
        for (tool, args) in [
            ("accounts_list", json!({})),
            (
                "account_create",
                json!({"name":"denied", "description":"denied", "environment":"paper", "broker":"ibkr", "idempotency_key":"monitor-denied-intent"}),
            ),
        ] {
            let (_, _, reply) = http(app.clone(), Some(&token), "/mcp", Some(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":tool,"arguments":args}}))).await;
            assert_eq!(
                reply["result"]["structuredContent"]["error"]["code"],
                "forbidden"
            );
        }
        let journal = rusqlite::Connection::open(&f.config.journal).unwrap();
        let operations: i64 = journal
            .query_row("SELECT COUNT(*) FROM operations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(operations, 0);
        worker.shutdown().await.unwrap();
    }
}

impl Broker for UncertainBroker {
    fn risk_market_data(
        &self,
        account: &Account,
        instrument_id: &str,
        _: u32,
    ) -> Result<model::portfolio_risk::RiskMarketData, Box<dyn std::error::Error>> {
        Ok(risk::market(
            model::account_safety::BrokerAccountScope::from_account(account)?,
            instrument_id,
        ))
    }
    fn account_snapshot(
        &self,
        account: &Account,
    ) -> Result<model::account_safety::BrokerAccountSnapshot, Box<dyn std::error::Error>> {
        let now = chrono::Utc::now();
        Ok(model::account_safety::BrokerAccountSnapshot {
            scope: model::account_safety::BrokerAccountScope::from_account(account)?,
            currency: "USD".into(),
            observed_at: now,
            received_at: now,
            complete: true,
            consistent: true,
            equity: Decimal::new(10000, 0),
            cash: Decimal::new(10000, 0),
            available_funds: Decimal::new(10000, 0),
            realized_pnl: Decimal::ZERO,
            unrealized_pnl: Decimal::ZERO,
            positions: vec![],
            working_orders: vec![],
            margin: Some(risk::margin()),
        })
    }
    fn kind(&self) -> BrokerKind {
        BrokerKind::Ibkr
    }
    fn submit_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn std::error::Error>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err("broker response lost after acceptance".into())
    }
    fn sync_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn std::error::Error>> {
        Err("offline".into())
    }
    fn close_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn std::error::Error>> {
        Err("offline".into())
    }
    fn cancel_trade(&self, _: &Trade, _: &Account) -> Result<(), Box<dyn std::error::Error>> {
        Err("offline".into())
    }
    fn modify_stop(
        &self,
        _: &Trade,
        _: &Account,
        _: Decimal,
    ) -> Result<String, Box<dyn std::error::Error>> {
        Err("offline".into())
    }
    fn modify_target(
        &self,
        _: &Trade,
        _: &Account,
        _: Decimal,
    ) -> Result<String, Box<dyn std::error::Error>> {
        Err("offline".into())
    }
}
async fn http(
    app: Router,
    token: Option<&str>,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut request = Request::builder()
        .uri(path)
        .header("host", "localhost:8000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-11-25");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let request = if let Some(body) = body {
        request
            .method("POST")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    } else {
        request.body(Body::empty()).unwrap()
    };
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 2_000_000).await.unwrap();
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw":String::from_utf8_lossy(&bytes)}));
    (status, headers, value)
}

#[test]
fn jwt_requires_issuer_audience_algorithm_lifetime_and_grant() {
    let fixture = Fixture::new();
    let auth = Authenticator::new(fixture.config.clone()).unwrap();
    assert!(auth.authenticate(&fixture.token()).is_ok());
    let token = fixture.token();
    let mut parts: Vec<_> = token.split('.').map(str::to_owned).collect();
    let mut signature = URL_SAFE_NO_PAD.decode(&parts[2]).unwrap();
    signature[0] ^= 1;
    parts[2] = URL_SAFE_NO_PAD.encode(signature);
    assert!(auth.authenticate(&parts.join(".")).is_err());
    for (field, bad) in [
        ("iss", json!("https://attacker.invalid")),
        ("aud", json!("other-resource")),
        ("sub", json!("ungranted")),
        ("exp", json!(1)),
        ("iat", json!(chrono::Utc::now().timestamp() + 60)),
    ] {
        let mut claims = fixture.claims();
        claims[field] = bad;
        assert!(auth.authenticate(&fixture.sign(claims)).is_err());
    }
    let token = encode(
        &Header::new(Algorithm::HS256),
        &fixture.claims(),
        &EncodingKey::from_secret(b"not-the-rsa-key"),
    )
    .unwrap();
    assert!(auth.authenticate(&token).is_err());
    let mut claims = fixture.claims();
    claims["scope"] = json!("trust:read trust:unknown");
    let identity = auth.authenticate(&fixture.sign(claims)).unwrap();
    assert!(identity.permits("trust:read"));
    assert!(!identity.permits("trust:trade"));
    assert!(!identity.permits("trust:unknown"));
}

#[tokio::test]
async fn real_http_mcp_auth_discovery_tools_resources_and_prompts() {
    let fixture = Fixture::new();
    let (service, worker) = fixture.start();
    let app = trust_service::http::router(fixture.config.clone(), service).unwrap();
    let (status, headers, _) = http(
        app.clone(),
        None,
        "/mcp",
        Some(json!({"jsonrpc":"2.0","id":1,"method":"tools/list"})),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(headers["www-authenticate"]
        .to_str()
        .unwrap()
        .contains("resource_metadata"));
    let (_, _, metadata) = http(
        app.clone(),
        None,
        "/.well-known/oauth-protected-resource/mcp",
        None,
    )
    .await;
    assert_eq!(metadata["resource"], fixture.config.resource());
    let token = fixture.token();
    let (_,_,init)=http(app.clone(),Some(&token),"/mcp",Some(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"contract-test","version":"1"}}}))).await;
    assert!(init.get("result").is_some(), "{init}");
    let (_, _, tools) = http(
        app.clone(),
        Some(&token),
        "/mcp",
        Some(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})),
    )
    .await;
    assert_eq!(
        tools["result"]["tools"].as_array().unwrap().len(),
        trust_service::catalog::tools().len()
    );
    let (_,_,status)=http(app.clone(),Some(&token),"/mcp",Some(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"service_status","arguments":{}}}))).await;
    assert_eq!(
        status["result"]["structuredContent"]["data"]["service"], "trust",
        "{status}"
    );
    for (method, params, field) in [
        (
            "resources/read",
            json!({"uri":"trust://skills/trust-risk"}),
            "contents",
        ),
        ("prompts/get", json!({"name":"trust-risk"}), "messages"),
    ] {
        let (_, _, result) = http(
            app.clone(),
            Some(&token),
            "/mcp",
            Some(json!({"jsonrpc":"2.0","id":4,"method":method,"params":params})),
        )
        .await;
        assert!(result["result"][field].as_array().is_some(), "{result}");
    }
    let mut claims = fixture.claims();
    claims["scope"] = json!("trust:read");
    let reader = fixture.sign(claims);
    let (_, _, tools) = http(
        app.clone(),
        Some(&reader),
        "/mcp",
        Some(json!({"jsonrpc":"2.0","id":5,"method":"tools/list"})),
    )
    .await;
    assert!(!tools.to_string().contains("trade_submit"));
    let (_,_,denied)=http(app,Some(&reader),"/mcp",Some(json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"account_create","arguments":{"name":"denied","description":"denied","environment":"paper","broker":"ibkr","idempotency_key":"denied-create"}}}))).await;
    assert_eq!(denied["result"]["isError"], true);
    assert_eq!(
        denied["result"]["structuredContent"]["error"]["code"],
        "forbidden"
    );
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn mcp_skill_references_discover_read_and_preserve_owner_visibility() {
    let fixture = Fixture::new();
    let (service, worker) = fixture.start();
    let app = trust_service::http::router(fixture.config.clone(), service).unwrap();
    for owner in [false, true] {
        let mut claims = fixture.claims();
        if !owner {
            claims["scope"] = json!("trust:read");
        }
        let token = fixture.sign(claims);
        for method in ["resources/list", "prompts/list"] {
            let (_, _, reply) = http(
                app.clone(),
                Some(&token),
                "/mcp",
                Some(json!({"jsonrpc":"2.0","id":1,"method":method})),
            )
            .await;
            assert!(reply["error"].is_null(), "{reply}");
            let items = reply["result"][if method == "resources/list" {
                "resources"
            } else {
                "prompts"
            }]
            .as_array()
            .unwrap();
            assert_eq!(
                items
                    .iter()
                    .any(|item| item["name"].as_str().unwrap().starts_with("trust-admin")),
                owner
            );
            assert!(items.iter().all(|item| item["description"]
                .as_str()
                .is_some_and(|text| !text.is_empty())));
        }
        for name in ["trust-risk", "trust-operations", "trust-admin"] {
            let metadata = trust_service::skills::catalogue()
                .iter()
                .find(|skill| skill.name == name)
                .unwrap();
            for file in &metadata.files {
                let (_, _, resource) = http(app.clone(), Some(&token), "/mcp",
                    Some(json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":file.uri}}))).await;
                let (_, _, call) = http(app.clone(), Some(&token), "/mcp",
                    Some(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"skill_read","arguments":{"name":name,"path":file.path}}}))).await;
                let reply = &call["result"]["structuredContent"];
                schema::validate("skill_read", reply);
                if !owner && name == "trust-admin" {
                    assert!(resource["error"].is_object());
                    assert_eq!(reply["error"]["code"], "forbidden");
                } else {
                    assert!(resource["error"].is_null(), "{resource}");
                    assert!(reply["error"].is_null(), "{reply}");
                    assert_eq!(
                        resource["result"]["contents"][0]["text"],
                        reply["data"]["content"]
                    );
                    assert_eq!(reply["data"]["sha256"], file.sha256);
                    assert_eq!(reply["data"]["uri"], file.uri);
                }
            }
        }
        let (_, _, listing) = http(app.clone(), Some(&token), "/mcp",
            Some(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"skills_list","arguments":{}}}))).await;
        let reply = &listing["result"]["structuredContent"];
        schema::validate("skills_list", reply);
        assert_eq!(
            reply["data"]["items"].as_array().unwrap().len(),
            if owner { 3 } else { 2 }
        );
    }
    assert_eq!(
        fixture.calls.load(Ordering::SeqCst),
        0,
        "Skill discovery must never invoke a broker"
    );
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn host_origin_body_and_query_controls_precede_operations() {
    let fixture = Fixture::new();
    let (service, worker) = fixture.start();
    let app = trust_service::http::router(fixture.config.clone(), service).unwrap();
    for (host, origin, uri, body, status) in [
        (
            "attacker.invalid",
            None,
            "/v1/tools",
            String::new(),
            StatusCode::FORBIDDEN,
        ),
        (
            "localhost:8000",
            Some("https://attacker.invalid"),
            "/v1/tools",
            String::new(),
            StatusCode::FORBIDDEN,
        ),
        (
            "localhost:8000",
            None,
            "/v1/tools?access_token=redacted",
            String::new(),
            StatusCode::BAD_REQUEST,
        ),
        (
            "localhost:8000",
            None,
            "/v1/tools/service_status",
            "x".repeat(131073),
            StatusCode::PAYLOAD_TOO_LARGE,
        ),
    ] {
        let mut request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("host", host)
            .header("authorization", format!("Bearer {}", fixture.token()))
            .header("content-type", "application/json");
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let response = app
            .clone()
            .oneshot(request.body(Body::from(body)).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), status);
    }
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn deposits_are_exact_idempotent_and_durable_across_restart() {
    let fixture = Fixture::new();
    let (service, worker) = fixture.start();
    let account = fixture.account(&service, "primary").await;
    let args = json!({"account_id":account,"amount":"1234.567890123456789","currency":"usd","idempotency_key":"deposit-once"});
    let first = fixture
        .call(&service, "capital_deposit", args.clone())
        .await;
    assert!(first.error.is_none(), "{first:?}");
    let repeated = fixture
        .call(&service, "capital_deposit", args.clone())
        .await;
    assert_eq!(first.operation_id, repeated.operation_id);
    assert_eq!(first.data, repeated.data);
    let mut conflict = args.clone();
    conflict["amount"] = json!("1");
    assert_eq!(
        fixture
            .call(&service, "capital_deposit", conflict)
            .await
            .error
            .unwrap()
            .code,
        "idempotency_conflict"
    );
    worker.shutdown().await.unwrap();
    let (service, worker) = fixture.start();
    let replayed = fixture.call(&service, "capital_deposit", args).await;
    assert_eq!(first.operation_id, replayed.operation_id);
    let balances = fixture
        .call(&service, "balances_get", json!({"account_id":account}))
        .await;
    assert!(balances
        .data
        .unwrap()
        .to_string()
        .contains("1234.567890123456789"));
    let transactions = fixture
        .call(&service, "transactions_list", json!({"account_id":account}))
        .await;
    assert_eq!(transactions.data.unwrap()["total"], 1);
    let operation = fixture
        .call(&service, "operation_get", json!({"key":"deposit-once"}))
        .await;
    assert_eq!(operation.data.unwrap()["state"], "completed");
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn account_scopes_live_gates_and_secondary_accounts_are_enforced() {
    let fixture = Fixture::new();
    let (service, worker) = fixture.start();
    let a = fixture.account(&service, "alpha").await;
    let b = fixture.account(&service, "beta").await;
    let mut restricted = fixture.identity();
    restricted.grant.all_accounts = false;
    restricted.grant.accounts = [a].into_iter().collect();
    for (tool, args) in [
        (
            "account_halt",
            json!({"account_id":a,"reason":"shared account","idempotency_key":"restricted-halt"}),
        ),
        (
            "account_resume",
            json!({"account_id":a,"reason":"shared account","idempotency_key":"restricted-resume"}),
        ),
        (
            "account_cashflow_record",
            json!({"account_id":a,"amount":"1","reference":"verified-reference","idempotency_key":"restricted-flow"}),
        ),
    ] {
        let denied = service
            .call(restricted.clone(), tool.into(), args, Uuid::new_v4())
            .await;
        assert_eq!(denied.error.unwrap().code, "forbidden", "{tool}");
        assert!(denied.operation_id.is_none());
    }
    let denied = service
        .call(
            restricted.clone(),
            "account_get".into(),
            json!({"account_id":b}),
            Uuid::new_v4(),
        )
        .await;
    assert_eq!(denied.error.unwrap().code, "forbidden");
    let denied=service.call(restricted,"capital_transfer".into(),json!({"account_id":a,"to_account_id":b,"amount":"1","reason":"test","idempotency_key":"transfer-denied"}),Uuid::new_v4()).await;
    assert_eq!(denied.error.unwrap().code, "forbidden");
    let live=fixture.call(&service,"account_create",json!({"name":"live","description":"test","environment":"live","broker":"ibkr","broker_account_id":"U_TEST","idempotency_key":"create-live"})).await;
    assert_eq!(live.error.unwrap().code, "live_disabled");
    let bad_money = fixture
        .call(
            &service,
            "capital_deposit",
            json!({"account_id":a,"amount":1.01,"idempotency_key":"float-denied"}),
        )
        .await;
    assert_eq!(bad_money.error.unwrap().code, "invalid_argument");
    worker.shutdown().await.unwrap();
}

#[test]
fn crash_recovery_never_replays_intent_and_rechecks_all_original_accounts() {
    let fixture = Fixture::new();
    let identity = fixture.identity();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let affected: BTreeSet<_> = [a, b].into_iter().collect();
    let args = json!({"account_id":a,"to_account_id":b,"amount":"1","reason":"test","idempotency_key":"crash-test"});
    let mut journal = Journal::open(&fixture.config.journal).unwrap();
    let id = match journal
        .claim(
            &identity,
            "crash-test",
            "capital_transfer",
            Some(a),
            &args,
            Uuid::new_v4(),
            &affected,
        )
        .unwrap()
    {
        Claim::New(id) => id,
        _ => panic!("new intent expected"),
    };
    drop(journal);
    let mut journal = Journal::open(&fixture.config.journal).unwrap();
    let reply = match journal
        .claim(
            &identity,
            "crash-test",
            "capital_transfer",
            Some(a),
            &args,
            Uuid::new_v4(),
            &affected,
        )
        .unwrap()
    {
        Claim::Existing(reply) => reply,
        _ => panic!("must not execute"),
    };
    assert_eq!(reply.operation_id, Some(id));
    assert_eq!(reply.error.unwrap().code, "outcome_unknown");
    let mut restricted = identity;
    restricted.grant.all_accounts = false;
    restricted.grant.accounts = [a].into_iter().collect();
    assert!(journal.operation(&restricted, "crash-test").is_err());
    // Both participants must remain authorized even for the original actor's audit.
    assert!(journal.list(&restricted, 0, 100).unwrap()["events"]
        .as_array()
        .unwrap()
        .is_empty());
    let events = journal.list(&fixture.identity(), 0, 100).unwrap();
    assert!(events["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["event"]["outcome"] == "recovered_unknown"));
    assert!(journal
        .claim(
            &restricted,
            "crash-test",
            "capital_transfer",
            Some(a),
            &args,
            Uuid::new_v4(),
            &affected
        )
        .is_err());
    assert!(journal.verify().is_ok());
    let db = rusqlite::Connection::open(&fixture.config.journal).unwrap();
    assert!(db.execute("DELETE FROM audit", []).is_err());
    db.execute_batch("DROP TRIGGER audit_no_update; UPDATE audit SET event='{}'")
        .unwrap();
    assert!(journal.verify().is_err());
    drop(journal);
    assert!(Journal::open(&fixture.config.journal).is_err());
}

#[test]
fn safety_state_history_is_atomic_private_and_rejects_deletion_or_rollback() {
    use model::account_safety::{AccountSafetyState, BrokerAccountScope};
    for fault in ["state", "history", "delete", "rollback", "tamper"] {
        let f = Fixture::new();
        let mut journal = Journal::open(&f.config.journal).unwrap();
        let db = rusqlite::Connection::open(&f.config.journal).unwrap();
        let account = Uuid::new_v4();
        let scope = BrokerAccountScope {
            broker: "ibkr".into(),
            environment: "paper".into(),
            account_id: "DU_TEST".into(),
        };
        let mut state = AccountSafetyState::default();
        journal
            .save_safety(&f.identity(), Uuid::new_v4(), account, &scope, &state)
            .unwrap();
        let before = journal.verify().unwrap();
        let original: (String, i64) = db
            .query_row("SELECT state,audit_sequence FROM account_safety", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        state.halt("operator_halt", chrono::Utc::now());
        if matches!(fault, "state" | "history") {
            let table = if fault == "state" {
                "account_safety"
            } else {
                "account_safety_history"
            };
            db.execute_batch(&format!("CREATE TRIGGER injected BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT,'injected'); END")).unwrap();
            assert!(journal
                .save_safety(&f.identity(), Uuid::new_v4(), account, &scope, &state)
                .is_err());
            assert_eq!(journal.verify().unwrap(), before);
            assert!(journal.safety_state(&scope).unwrap().halt.is_none());
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM account_safety_history", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(count, 1);
            continue;
        }
        journal
            .save_safety(&f.identity(), Uuid::new_v4(), account, &scope, &state)
            .unwrap();
        let owner = journal.list(&f.identity(), 0, 100).unwrap();
        assert!(owner["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["safety_evidence"].is_object()));
        let mut agent = f.identity();
        agent.grant.role = Role::Agent;
        assert!(journal.list(&agent, 0, 100).unwrap()["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["safety_evidence"].is_null()));
        match fault {
            "delete" => {
                db.execute("DELETE FROM account_safety", []).unwrap();
            }
            "rollback" => {
                db.execute(
                    "UPDATE account_safety SET state=?1,audit_sequence=?2",
                    rusqlite::params![original.0, original.1],
                )
                .unwrap();
            }
            _ => {
                db.execute("UPDATE account_safety SET state='{}'", [])
                    .unwrap();
            }
        }
        assert_eq!(
            journal.safety_state(&scope).unwrap_err().code,
            "audit_integrity_failed"
        );
    }
}

#[test]
fn operation_resolution_is_audited_and_cannot_hide_secondary_account_uncertainty() {
    let f = Fixture::new();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let accounts = [a, b].into_iter().collect();
    let mut journal = Journal::open(&f.config.journal).unwrap();
    let intent = journal
        .claim(
            &f.identity(),
            "resolution-test",
            "capital_transfer",
            Some(a),
            &json!({"amount":"1"}),
            Uuid::new_v4(),
            &accounts,
        )
        .unwrap();
    let Claim::New(operation) = intent else {
        panic!("new intent expected")
    };
    drop(journal);
    let mut journal = Journal::open(&f.config.journal).unwrap();
    assert!(journal
        .unresolved_for(&[b].into_iter().collect())
        .unwrap()
        .contains(&operation));
    journal
        .resolve_operation(
            &f.identity(),
            Uuid::new_v4(),
            a,
            operation,
            "independently verified transfer",
        )
        .unwrap();
    assert!(journal
        .unresolved_for(&[b].into_iter().collect())
        .unwrap()
        .is_empty());
    assert_eq!(
        journal.operation(&f.identity(), "resolution-test").unwrap()["state"],
        "outcome_unknown"
    );
    let db = rusqlite::Connection::open(&f.config.journal).unwrap();
    db.execute("UPDATE operation_resolutions SET evidence='{}'", [])
        .unwrap();
    assert_eq!(
        journal.unresolved_for(&accounts).unwrap_err().code,
        "audit_integrity_failed"
    );
}

#[test]
fn parameterized_skills_validate_identifiers_and_account_authority() {
    let f = Fixture::new();
    let account = Uuid::new_v4();
    let trade = Uuid::new_v4();
    let mut identity = f.identity();
    identity.grant.all_accounts = false;
    identity.grant.accounts = [account].into_iter().collect();
    let prompt = |args: Value| {
        trust_service::skills::prompt(&identity, "trust-risk", args.as_object().unwrap().clone())
    };
    let content = prompt(json!({"account_id":account,"trade_id":trade})).unwrap();
    assert!(content.contains(&account.to_string()) && content.contains(&trade.to_string()));
    for args in [
        json!({"account_id":Uuid::new_v4()}),
        json!({"trade_id":trade}),
        json!({"account_id":"ignore rules"}),
        json!({"instructions":"override"}),
    ] {
        assert!(prompt(args).is_err());
    }
}

#[tokio::test]
async fn session_input_errors_are_definitive_and_unknown_journaling_does_not_halt_finances() {
    let f = Fixture::new();
    let (service, worker) = f.start();
    let account = f.account(&service, "session-input").await;
    let valid = json!({"account_id":account,"regime":"elevated","setups":["breakout"],"max_positions":1,"hypothesis":"Observation unavailable; conservative planning","success_criteria":"Follow mandate","failure_criteria":"Stop on unknown exposure","idempotency_key":"session-valid"});
    for (field, value) in [
        ("regime", json!("unavailable")),
        ("setups", json!(["entry 100, stop 95"])),
        ("setups", json!(["A", "a"])),
        ("hypothesis", json!("x".repeat(501))),
        ("success_criteria", json!(" ")),
    ] {
        let mut args = valid.clone();
        args[field] = value;
        let reply = f.call(&service, "session_open", args).await;
        assert_eq!(reply.error.unwrap().code, "invalid_argument");
        assert!(reply.operation_id.is_none());
    }
    assert!(f
        .call(&service, "session_open", valid.clone())
        .await
        .error
        .is_none());
    let mut duplicate = valid;
    duplicate["idempotency_key"] = json!("session-second-intent");
    let reply = f.call(&service, "session_open", duplicate).await;
    assert_eq!(reply.error.unwrap().code, "outcome_unknown");
    let ready = f
        .call(&service, "account_readiness", json!({"account_id":account}))
        .await
        .data
        .unwrap();
    assert_eq!(ready["halted"], false);
    assert_eq!(ready["unresolved_operation_count"], 0);
    worker.shutdown().await.unwrap();
    let (service, worker) = f.start();
    let ready = f
        .call(&service, "account_readiness", json!({"account_id":account}))
        .await
        .data
        .unwrap();
    assert_eq!(ready["halted"], false);
    assert_eq!(ready["unresolved_operation_count"], 0);
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn uncertain_broker_submission_is_never_executed_twice() {
    let fixture = Fixture::new();
    let (service, worker) = fixture.start();
    let account = fixture.account(&service, "trading").await;
    for (name, args) in [
        (
            "capital_deposit",
            json!({"account_id":account,"amount":"10000","idempotency_key":"seed-deposit"}),
        ),
        (
            "risk_rule_create",
            json!({"account_id":account,"kind":"per_trade","percentage":"1","priority":"error","description":"risk","idempotency_key":"seed-risk"}),
        ),
        (
            "risk_rule_create",
            json!({"account_id":account,"kind":"per_month","percentage":"5","priority":"error","description":"risk","idempotency_key":"seed-month"}),
        ),
    ] {
        let reply = fixture.call(&service, name, args).await;
        assert!(reply.error.is_none(), "{reply:?}");
    }
    let instrument=fixture.call(&service,"instrument_create",json!({"symbol":"TEST","category":"stock","broker":"ibkr","idempotency_key":"seed-instrument"})).await.data.unwrap()["id"].clone();
    assert!(fixture.call(&service,"instrument_update",json!({"instrument_id":instrument,"broker_asset_id":"1234","idempotency_key":"native-test"})).await.error.is_none());
    let draft=fixture.call(&service,"trade_prepare",json!({"account_id":account,"instrument_id":instrument,"side":"long","quantity":"1","entry":"100","stop":"95","target":"110","idempotency_key":"prepare-test"})).await;
    assert!(draft.error.is_none(), "{draft:?}");
    let trade = draft.data.unwrap()["id"].clone();
    let other = fixture.account(&service, "other").await;
    let mismatch = fixture
        .call(
            &service,
            "trade_get",
            json!({"account_id":other,"trade_id":trade}),
        )
        .await;
    assert_eq!(mismatch.error.unwrap().code, "forbidden");
    let funded = fixture
        .call(
            &service,
            "trade_fund",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"fund-test"}),
        )
        .await;
    assert!(funded.error.is_none(), "{funded:?}");
    let args = json!({"account_id":account,"trade_id":trade,"idempotency_key":"submit-test"});
    let first = fixture.call(&service, "trade_submit", args.clone()).await;
    assert_eq!(first.error.unwrap().code, "outcome_unknown");
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    let repeat = fixture.call(&service, "trade_submit", args.clone()).await;
    assert_eq!(repeat.operation_id, first.operation_id);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    worker.shutdown().await.unwrap();
    let (service, worker) = fixture.start();
    let repeat = fixture.call(&service, "trade_submit", args).await;
    assert_eq!(repeat.operation_id, first.operation_id);
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    worker.shutdown().await.unwrap();
}

#[test]
fn catalogue_has_unique_strict_schemas_and_no_secret_arguments() {
    let mut names = BTreeSet::new();
    for tool in trust_service::catalog::tools() {
        assert!(names.insert(tool.name));
        assert_eq!(tool.input_schema["additionalProperties"], false);
        if tool.mutation {
            assert!(tool.input_schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("idempotency_key")));
        }
        for key in tool.input_schema["properties"].as_object().unwrap().keys() {
            if tool.name == "skill_read" && key == "path" {
                // This is an embedded-file identifier, never a filesystem path.
                let allowed = tool.input_schema["properties"][key]["enum"]
                    .as_array()
                    .unwrap();
                assert_eq!(allowed.len(), trust_service::skills::paths().len());
                assert!(
                    allowed
                        .iter()
                        .all(|value| trust_service::skills::paths()
                            .contains(&value.as_str().unwrap()))
                );
            } else {
                assert!(!["token", "password", "secret", "url", "path", "command"]
                    .contains(&key.as_str()));
            }
        }
    }
}

fn schema_sample(schema: &Value) -> Value {
    if let Some(first) = schema
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|v| v.first())
    {
        return first.clone();
    }
    match schema["type"].as_str().unwrap() {
        "string" => match schema["format"].as_str() {
            Some("uuid") => json!("00000000-0000-0000-0000-000000000001"),
            Some("decimal") => json!("1.25"),
            Some("date-time") => json!("2099-01-01T00:00:00Z"),
            _ => json!("test-value"),
        },
        "integer" => schema.get("minimum").cloned().unwrap_or(json!(0)),
        "boolean" => json!(false),
        "array" => json!([schema_sample(&schema["items"])]),
        "object" => Value::Object(
            schema
                .get("properties")
                .and_then(Value::as_object)
                .map(|props| {
                    props
                        .iter()
                        .map(|(k, v)| (k.clone(), schema_sample(v)))
                        .collect()
                })
                .unwrap_or_default(),
        ),
        other => panic!("new schema kind needs a boundary fixture: {other}"),
    }
}
fn tool_arguments(tool: &trust_service::catalog::ToolSpec) -> Value {
    let required = tool.input_schema["required"].as_array().unwrap();
    Value::Object(
        tool.input_schema["properties"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(name, _)| required.contains(&json!(name)))
            .map(|(name, schema)| (name.clone(), schema_sample(schema)))
            .collect(),
    )
}

#[test]
fn every_tool_rejects_missing_unknown_wrong_type_and_boundary_arguments() {
    use trust_service::catalog::{tools, validate};
    let mut checks = 0_u32;
    for tool in tools() {
        let base = tool_arguments(tool);
        validate(tool, &base).unwrap_or_else(|e| panic!("{}: {e:?}", tool.name));
        for required in tool.input_schema["required"].as_array().unwrap() {
            let mut args = base.clone();
            args.as_object_mut()
                .unwrap()
                .remove(required.as_str().unwrap());
            assert!(
                validate(tool, &args).is_err(),
                "{} missing {required}",
                tool.name
            );
            checks += 1;
        }
        for (name, schema) in tool.input_schema["properties"].as_object().unwrap() {
            let mut args = base.clone();
            args[name] = schema_sample(schema);
            assert!(validate(tool, &args).is_ok(), "{} valid {name}", tool.name);
            let mut invalid = vec![Value::Null, json!({})];
            match schema["type"].as_str().unwrap() {
                "string" => {
                    invalid.extend([
                        json!(1),
                        json!(true),
                        json!("a\0b"),
                        json!("a".repeat(2001)),
                    ]);
                    if schema["minLength"].as_u64().is_some_and(|n| n > 0) {
                        invalid.push(json!(""));
                    }
                    if schema.get("enum").is_some() {
                        invalid.push(json!("not-an-enum-value"));
                    }
                    match schema["format"].as_str() {
                        Some("uuid") => invalid.extend([json!("not-a-uuid"), json!("../private")]),
                        Some("decimal") => invalid.extend([
                            json!("NaN"),
                            json!("Infinity"),
                            json!("1e2"),
                            json!("79228162514264337593543950336"),
                            json!("0.00000000000000000000000000001"),
                            json!("1.12345678901234567890123456789"),
                        ]),
                        _ => {}
                    }
                }
                "integer" => {
                    invalid.extend([json!("1"), json!(-1), json!(1.25), json!(true)]);
                    if let Some(min) = schema["minimum"].as_u64().filter(|n| *n > 0) {
                        invalid.push(json!(min - 1));
                    }
                    if let Some(max) = schema["maximum"].as_u64().and_then(|n| n.checked_add(1)) {
                        invalid.push(json!(max));
                    }
                }
                "boolean" => invalid.extend([json!("false"), json!(0)]),
                "array" => invalid.extend([
                    json!("item"),
                    json!([null]),
                    json!([{}]),
                    json!(vec!["a"; 65]),
                ]),
                "object" => invalid.extend([json!(1), json!("object"), json!({"unexpected":true})]),
                _ => unreachable!(),
            }
            for value in invalid {
                args[name] = value;
                assert!(
                    validate(tool, &args).is_err(),
                    "{} accepted invalid {name}: {}",
                    tool.name,
                    args[name]
                );
                checks += 1;
            }
        }
        let mut unknown = base;
        unknown["unexpected_argument"] = json!(true);
        assert!(validate(tool, &unknown).is_err());
        for value in [json!(null), json!([]), json!(1), json!("arguments")] {
            assert!(validate(tool, &value).is_err());
        }
    }
    assert!(
        checks > 2000,
        "catalogue boundary checks unexpectedly disappeared: {checks}"
    );
    println!("Catalogue boundary assertions: {checks}");
}

#[tokio::test]
async fn every_tool_enforces_scope_before_domain_or_broker_access() {
    let fixture = Fixture::new();
    let (service, worker) = fixture.start();
    for tool in trust_service::catalog::tools() {
        for remove_from_token in [true, false] {
            let mut identity = fixture.identity();
            if remove_from_token {
                identity.scopes.remove(tool.scope);
            } else {
                identity.grant.scopes.remove(tool.scope);
            }
            let reply = service
                .call(
                    identity,
                    tool.name.into(),
                    tool_arguments(tool),
                    Uuid::new_v4(),
                )
                .await;
            assert_eq!(
                reply.error.unwrap().code,
                "forbidden",
                "{} missing scope",
                tool.name
            );
            assert!(reply.operation_id.is_none());
        }
    }
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    let accounts = fixture.call(&service, "accounts_list", json!({})).await;
    assert_eq!(accounts.data.unwrap()["total"], 0);
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn concurrent_duplicates_credit_once_and_distinct_withdrawals_cannot_overspend() {
    let fixture = Fixture::new();
    let (service, worker) = fixture.start();
    let account = fixture.account(&service, "concurrent").await;
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..32 {
        let service = service.clone();
        let identity = fixture.identity();
        tasks.spawn(async move { service.call(identity,"capital_deposit".into(),json!({"account_id":account,"amount":"100.00","idempotency_key":"same-deposit-key"}),Uuid::new_v4()).await });
    }
    let mut operations = BTreeSet::new();
    while let Some(result) = tasks.join_next().await {
        let reply = result.unwrap();
        assert!(reply.error.is_none(), "{reply:?}");
        operations.insert(reply.operation_id.unwrap());
    }
    assert_eq!(operations.len(), 1);
    for i in 0..16 {
        let service = service.clone();
        let identity = fixture.identity();
        tasks.spawn(async move { service.call(identity,"capital_withdraw".into(),json!({"account_id":account,"amount":"70","idempotency_key":format!("withdraw-{i:02}")}),Uuid::new_v4()).await });
    }
    let mut succeeded = 0;
    while let Some(result) = tasks.join_next().await {
        if result.unwrap().error.is_none() {
            succeeded += 1;
        }
    }
    assert_eq!(succeeded, 1);
    let balances = fixture
        .call(&service, "balances_get", json!({"account_id":account}))
        .await;
    assert_eq!(
        balances.data.unwrap()["items"][0]["total_available"]
            .as_str()
            .unwrap()
            .parse::<Decimal>()
            .unwrap(),
        Decimal::from(30)
    );
    let transactions = fixture
        .call(&service, "transactions_list", json!({"account_id":account}))
        .await;
    assert_eq!(transactions.data.unwrap()["total"], 2);
    let audit = fixture.call(&service, "audit_verify", json!({})).await;
    assert!(audit.error.is_none());
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn journal_intent_failure_stops_worker_before_financial_mutation() {
    let fixture = Fixture::new();
    let (service, worker) = fixture.start();
    let account = fixture.account(&service, "intent-failure").await;
    let connection = rusqlite::Connection::open(&fixture.config.journal).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_intent BEFORE INSERT ON operations BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    let reply = fixture
        .call(
            &service,
            "capital_deposit",
            json!({"account_id":account,"amount":"10","idempotency_key":"failed-intent"}),
        )
        .await;
    assert_eq!(reply.error.unwrap().code, "audit_unavailable");
    worker.shutdown().await.unwrap();
    assert!(!service.ready());
    let db = db_sqlite::SqliteDatabase::new(fixture.config.database.to_str().unwrap());
    use model::DatabaseFactory;
    assert!(db
        .transaction_read()
        .all_transactions(account, &model::Currency::USD)
        .unwrap()
        .is_empty());
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn journal_completion_failure_keeps_committed_deposit_unknown_without_replay() {
    let fixture = Fixture::new();
    let (service, worker) = fixture.start();
    let account = fixture.account(&service, "completion-failure").await;
    let connection = rusqlite::Connection::open(&fixture.config.journal).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_finish BEFORE UPDATE ON operations BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    let args = json!({"account_id":account,"amount":"10","idempotency_key":"unknown-completion"});
    let reply = fixture
        .call(&service, "capital_deposit", args.clone())
        .await;
    assert_eq!(reply.error.unwrap().code, "audit_unavailable");
    worker.shutdown().await.unwrap();
    connection
        .execute_batch("DROP TRIGGER fail_finish")
        .unwrap();
    let (service, worker) = fixture.start();
    let replay = fixture.call(&service, "capital_deposit", args).await;
    assert_eq!(replay.error.unwrap().code, "outcome_unknown");
    let transactions = fixture
        .call(&service, "transactions_list", json!({"account_id":account}))
        .await;
    assert_eq!(transactions.data.unwrap()["total"], 1);
    let balances = fixture
        .call(&service, "balances_get", json!({"account_id":account}))
        .await;
    assert_eq!(balances.data.unwrap()["items"][0]["total_available"], "10");
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn second_worker_cannot_open_owned_storage_and_shutdown_rejects_new_work() {
    let fixture = Fixture::new();
    let (service, worker) = fixture.start();
    assert!(Service::start(fixture.config.clone(), || panic!(
        "second factory must never run"
    ))
    .is_err());
    worker.shutdown().await.unwrap();
    let reply = fixture.call(&service, "service_status", json!({})).await;
    assert_eq!(reply.error.unwrap().code, "unavailable");
    let (_, worker) = fixture.start();
    worker.shutdown().await.unwrap();
}

#[test]
fn failed_recovery_audit_rolls_back_state_and_recovery_is_recorded_once() {
    let fixture = Fixture::new();
    let identity = fixture.identity();
    let account = Uuid::new_v4();
    let args = json!({"account_id":account,"amount":"1","idempotency_key":"recovery-test"});
    let mut journal = Journal::open(&fixture.config.journal).unwrap();
    journal
        .claim(
            &identity,
            "recovery-test",
            "capital_deposit",
            Some(account),
            &args,
            Uuid::new_v4(),
            &[account].into_iter().collect(),
        )
        .unwrap();
    drop(journal);
    let db = rusqlite::Connection::open(&fixture.config.journal).unwrap();
    db.execute_batch("CREATE TRIGGER recovery_fault BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'test disk failure'); END").unwrap();
    assert!(Journal::open(&fixture.config.journal).is_err());
    assert_eq!(
        db.query_row("SELECT state FROM operations", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "running"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM audit", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    db.execute_batch("DROP TRIGGER recovery_fault").unwrap();
    let journal = Journal::open(&fixture.config.journal).unwrap();
    assert_eq!(
        journal.operation(&identity, "recovery-test").unwrap()["state"],
        "outcome_unknown"
    );
    assert_eq!(journal.verify().unwrap()["events"], 2);
    drop(journal);
    let journal = Journal::open(&fixture.config.journal).unwrap();
    assert_eq!(journal.verify().unwrap()["events"], 2);
}

#[test]
fn queue_and_request_deadlines_have_validated_bounds() {
    let fixture = Fixture::new();
    for (request, queue) in [(0, 1), (45001, 1), (100, 0), (100, 101)] {
        let mut config = (*fixture.config).clone();
        config.request_timeout_ms = request;
        config.queue_timeout_ms = queue;
        assert!(config.validate().is_err());
    }
    let mut config = (*fixture.config).clone();
    config.request_timeout_ms = 1;
    config.queue_timeout_ms = 1;
    assert!(config.validate().is_ok());
}

#[tokio::test]
async fn detected_runtime_audit_corruption_stops_financial_admission() {
    let fixture = Fixture::new();
    let (service, worker) = fixture.start();
    let account = fixture.account(&service, "integrity").await;
    let db = rusqlite::Connection::open(&fixture.config.journal).unwrap();
    db.execute_batch("DROP TRIGGER audit_no_update; UPDATE audit SET event='{}' WHERE sequence=1")
        .unwrap();
    let reply = fixture.call(&service, "audit_verify", json!({})).await;
    assert_eq!(reply.error.unwrap().code, "audit_integrity_failed");
    assert!(!service.ready());
    let denied = fixture
        .call(
            &service,
            "capital_deposit",
            json!({"account_id":account,"amount":"1","idempotency_key":"after-corruption"}),
        )
        .await;
    assert_eq!(denied.error.unwrap().code, "unavailable");
    assert!(service
        .prometheus()
        .contains("trust_audit_failures_total 1\n"));
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn broker_reconciliation_authority_cannot_record_manual_executions() {
    let f = Fixture::new();
    let (service, worker) = f.start();
    let account = f.account(&service, "scope-split").await;
    let mut actor = f.identity();
    actor.scopes = ["trust:read".to_owned(), "trust:reconcile".to_owned()]
        .into_iter()
        .collect();
    actor.grant.scopes = actor.scopes.clone();
    let reconciled = service
        .call(
            actor.clone(),
            "account_reconcile".into(),
            json!({"account_id":account,"idempotency_key":"real-reconcile-key"}),
            Uuid::new_v4(),
        )
        .await;
    assert!(reconciled.error.is_none(), "{reconciled:?}");
    for tool in [
        "trade_record_fill",
        "trade_record_stop",
        "trade_record_target",
    ] {
        let reply = service.call(actor.clone(), tool.into(), json!({"account_id":account,"trade_id":Uuid::new_v4(),"price":"1","fee":"0","idempotency_key":format!("forbidden-{tool}")}), Uuid::new_v4()).await;
        assert_eq!(reply.error.unwrap().code, "forbidden");
        assert!(reply.operation_id.is_none());
    }
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn service_requires_explicit_trade_and_monthly_policy() {
    let f = Fixture::new();
    let (service, worker) = f.start();
    let account = f.account(&service, "required-policy").await;
    let deposit = f
        .call(
            &service,
            "capital_deposit",
            json!({"account_id":account,"amount":"10000","idempotency_key":"policy-deposit"}),
        )
        .await;
    assert!(deposit.error.is_none());
    let instrument = f.call(&service, "instrument_create", json!({"symbol":"POLICY","category":"stock","broker":"ibkr","idempotency_key":"policy-instrument"})).await.data.unwrap()["id"].clone();
    assert!(f.call(&service,"instrument_update",json!({"instrument_id":instrument,"broker_asset_id":"1234","idempotency_key":"native-test"})).await.error.is_none());
    let trade = f.call(&service, "trade_prepare", json!({"account_id":account,"instrument_id":instrument,"side":"long","quantity":"1","entry":"100","stop":"95","target":"110","idempotency_key":"policy-prepare"})).await.data.unwrap()["id"].clone();
    for (index, kind) in ["per_trade", "per_month"].into_iter().enumerate() {
        let denied = f.call(&service, "trade_fund", json!({"account_id":account,"trade_id":trade,"idempotency_key":format!("missing-policy-{index}")})).await;
        assert_eq!(denied.error.unwrap().code, "risk_policy_missing");
        let rule = f.call(&service, "risk_rule_create", json!({"account_id":account,"kind":kind,"percentage":"2","priority":"error","description":"explicit cap","idempotency_key":format!("policy-{kind}")})).await;
        assert!(rule.error.is_none());
    }
    let funded = f
        .call(
            &service,
            "trade_fund",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"policy-fund-allowed"}),
        )
        .await;
    assert!(funded.error.is_none(), "{funded:?}");
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    worker.shutdown().await.unwrap();
}

#[test]
fn owner_configuration_requires_one_human_and_rejects_privileged_agent_grants() {
    let f = Fixture::new();
    let mut config = (*f.config).clone();
    config.grants.get_mut("owner").unwrap().role = Role::Agent;
    assert!(config.validate().is_err());
    for scope in ["trust:policy", "trust:admin", "trust:manual"] {
        let grant: Grant =
            serde_json::from_value(json!({"all_accounts":true,"scopes":[scope]})).unwrap();
        assert_eq!(grant.role, Role::Agent);
        assert!(grant.validate().is_err());
    }
    config = (*f.config).clone();
    config
        .grants
        .insert("second-owner".into(), config.grants["owner"].clone());
    assert!(config.validate().is_err());
    config = (*f.config).clone();
    for age in [0, 901, u64::MAX] {
        config.owner_authentication.max_age_seconds = age;
        assert!(config.validate().is_err());
    }
    config.owner_authentication.max_age_seconds = 300;
    for acr in [
        "",
        "two values",
        "quoted\"value",
        "line\nbreak",
        "escape\\value",
    ] {
        config.owner_authentication.acr_values = [acr.into()].into_iter().collect();
        assert!(config.validate().is_err());
    }
    config.owner_authentication.acr_values.clear();
    assert!(config.validate().is_err());
}

#[test]
fn owner_tokens_require_original_recent_authentication_and_exact_mfa_context() {
    let f = Fixture::new();
    let auth = Authenticator::new(f.config.clone()).unwrap();
    let original = f.claims();
    let now = original["iat"].as_u64().unwrap();
    for (field, value) in [
        ("auth_time", Value::Null),
        ("auth_time", json!(now - 300)),
        ("auth_time", json!(now + 1)),
        ("auth_time", json!(u64::MAX)),
        ("acr", Value::Null),
        ("acr", json!("urn:trust:owner:password-only")),
    ] {
        let mut claims = original.clone();
        claims[field] = value;
        let error = auth.authenticate(&f.sign(claims)).unwrap_err();
        assert_eq!(error.code, "insufficient_user_authentication", "{field}");
    }
    // Newly issued access/refresh tokens do not turn an old login into fresh MFA.
    let mut refreshed = original.clone();
    refreshed["auth_time"] = json!(now - 600);
    assert!(auth.authenticate(&f.sign(refreshed)).is_err());
    let mut read_only = original.clone();
    read_only["auth_time"] = Value::Null;
    read_only["acr"] = Value::Null;
    read_only["scope"] = json!("trust:read trust:trade");
    let identity = auth.authenticate(&f.sign(read_only)).unwrap();
    assert!(identity.check("trust:trade", None).is_ok());
    assert!(identity.check("trust:policy", None).is_err());
    assert!(auth
        .authenticate(&f.token())
        .unwrap()
        .check("trust:policy", None)
        .is_ok());
}

#[tokio::test]
async fn agents_cannot_gain_owner_authority_from_claims_scopes_or_tool_arguments() {
    let f = Fixture::new();
    let mut config = (*f.config).clone();
    let agent: Grant = serde_json::from_value(json!({"role":"agent","all_accounts":true,"scopes":["trust:read","trust:preview","trust:trade","trust:reconcile","trust:journal"]})).unwrap();
    config.grants.insert("agent".into(), agent);
    config.validate().unwrap();
    let mut claims = f.claims();
    claims["sub"] = json!("agent");
    claims["role"] = json!("owner");
    claims["owner_authenticated_until"] = json!(u64::MAX);
    let mut identity = Authenticator::new(Arc::new(config))
        .unwrap()
        .authenticate(&f.sign(claims))
        .unwrap();
    assert_eq!(identity.grant.role, Role::Agent);
    assert_eq!(identity.owner_authenticated_until, None);
    // Defense in depth: even an incorrectly constructed broad internal grant
    // cannot turn an agent role into an owner.
    identity.grant.scopes = SCOPES.iter().map(|s| (*s).to_owned()).collect();
    identity.owner_authenticated_until = Some(u64::MAX);
    let (service, worker) = f.start();
    let mut denied = 0;
    for tool in trust_service::catalog::tools()
        .iter()
        .filter(|tool| tool.owner_only)
    {
        assert!(!trust_service::mcp::visible(&identity, tool));
        let reply = service
            .call(
                identity.clone(),
                tool.name.into(),
                tool_arguments(tool),
                Uuid::new_v4(),
            )
            .await;
        assert_eq!(reply.error.unwrap().code, "forbidden", "{}", tool.name);
        assert!(reply.operation_id.is_none());
        denied += 1;
    }
    assert!(denied >= 15);
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        f.call(&service, "accounts_list", json!({}))
            .await
            .data
            .unwrap()["total"],
        0
    );
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn http_and_mcp_challenge_owner_mfa_before_accepting_mutations() {
    let f = Fixture::new();
    let (service, worker) = f.start();
    let app = trust_service::http::router(f.config.clone(), service.clone()).unwrap();
    let mut claims = f.claims();
    claims["auth_time"] = Value::Null;
    let token = f.sign(claims);
    for (path, body) in [
        (
            "/v1/tools/account_create",
            json!({"name":"denied","description":"denied","environment":"paper","broker":"ibkr","idempotency_key":"needs-owner-mfa"}),
        ),
        (
            "/mcp",
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"capital_deposit","arguments":{"account_id":Uuid::new_v4(),"amount":"100","idempotency_key":"needs-owner-mfa"}}}),
        ),
    ] {
        let (status, headers, reply) = http(app.clone(), Some(&token), path, Some(body)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        let challenge = headers["www-authenticate"].to_str().unwrap();
        assert!(challenge.contains("insufficient_user_authentication"));
        assert!(challenge.contains("acr_values=\"urn:trust:owner:mfa\""));
        assert!(challenge.contains("max_age=\"300\""));
        assert_eq!(reply["error"]["code"], "insufficient_user_authentication");
        assert!(reply["operation_id"].is_null());
    }
    assert_eq!(
        f.call(&service, "accounts_list", json!({}))
            .await
            .data
            .unwrap()["total"],
        0
    );
    worker.shutdown().await.unwrap();
}

#[tokio::test]
async fn owner_authentication_is_rechecked_before_dispatch_and_idempotent_replay() {
    let f = Fixture::new();
    let (service, worker) = f.start();
    let account = f.account(&service, "owner-replay").await;
    let args =
        json!({"account_id":account,"amount":"10","idempotency_key":"owner-original-deposit"});
    let first = f.call(&service, "capital_deposit", args.clone()).await;
    assert!(first.error.is_none());
    let mut expired = f.identity();
    expired.owner_authenticated_until = Some(0);
    for key in ["owner-original-deposit", "owner-new-deposit"] {
        let mut request = args.clone();
        request["idempotency_key"] = json!(key);
        let denied = service
            .call(
                expired.clone(),
                "capital_deposit".into(),
                request,
                Uuid::new_v4(),
            )
            .await;
        assert_eq!(
            denied.error.unwrap().code,
            "insufficient_user_authentication"
        );
        assert!(denied.operation_id.is_none());
    }
    let replay = f.call(&service, "capital_deposit", args).await;
    assert_eq!(first.operation_id, replay.operation_id);
    let db = rusqlite::Connection::open(&f.config.journal).unwrap();
    let deposits: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM operations WHERE tool='capital_deposit'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(deposits, 1);
    let event: String = db.query_row("SELECT event FROM audit WHERE json_extract(event, '$.tool')='capital_deposit' AND json_extract(event, '$.outcome')='attempted' LIMIT 1", [], |row| row.get(0)).unwrap();
    let event: Value = serde_json::from_str(&event).unwrap();
    assert_eq!(event["actor_role"], "owner");
    assert!(event["owner_authenticated_until"].as_u64().is_some());
    worker.shutdown().await.unwrap();
}

#[test]
fn nested_risk_policy_rejects_lossy_money_and_untyped_fields_before_dispatch() {
    use trust_service::catalog::{find, validate};
    let tool = find("account_safety_policy_set").unwrap();
    let base = json!({"account_id":Uuid::new_v4(),"currency":"USD","max_gross_notional":"10000","max_order_notional":"1000","max_daily_loss":"100","max_drawdown":"500","max_snapshot_age_seconds":30,"controls":risk::controls(),"idempotency_key":"nested-policy"});
    validate(tool, &base).unwrap();
    for value in [
        json!(1.25),
        json!("NaN"),
        json!("1e2"),
        json!("+1"),
        json!(" 1"),
        json!("79228162514264337593543950336"),
        json!("0.00000000000000000000000000001"),
    ] {
        let mut args = base.clone();
        args["controls"]["max_portfolio_heat_pct"] = value.clone();
        assert!(validate(tool, &args).is_err(), "{value}");
        let mut args = base.clone();
        args["controls"]["instruments"]["1234"]["gap_stress_pct"] = value.clone();
        assert!(validate(tool, &args).is_err(), "{value}");
    }
    for (field, value) in [
        ("unknown", json!("1")),
        ("max_quote_age_seconds", json!(-1)),
        ("instruments", json!([])),
    ] {
        let mut args = base.clone();
        args["controls"][field] = value;
        assert!(validate(tool, &args).is_err(), "{field}");
    }
    let mut args = base;
    args["controls"]["instruments"]["1234"]["valid_until"] = json!("tomorrow");
    assert!(validate(tool, &args).is_err());
}
