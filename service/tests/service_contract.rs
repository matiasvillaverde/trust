#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
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
    auth::{Authenticator, Identity, SCOPES},
    config::Config,
    journal::{Claim, Journal, Reply},
    runtime::{Service, Worker},
};
use uuid::Uuid;

fn key() -> &'static (Vec<u8>, Value) {
    static KEY: OnceLock<(Vec<u8>, Value)> = OnceLock::new();
    KEY.get_or_init(||{let rsa=openssl::rsa::Rsa::generate(2048).unwrap();let jwk=json!({"kty":"RSA","use":"sig","alg":"RS256","kid":"test-key","n":URL_SAFE_NO_PAD.encode(rsa.n().to_vec()),"e":URL_SAFE_NO_PAD.encode(rsa.e().to_vec())});(rsa.private_key_to_pem().unwrap(),json!({"keys":[jwk]}))})
}
struct Fixture {
    _directory: tempfile::TempDir,
    config: Arc<Config>,
    calls: Arc<AtomicUsize>,
}
impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let jwks = directory.path().join("jwks.json");
        std::fs::write(&jwks, key().1.to_string()).unwrap();
        let config:Config=serde_json::from_value(json!({"listen":"127.0.0.1:0","public_url":"http://localhost:8000","issuer":"http://localhost:9999/issuer","jwks_file":jwks,"database":directory.path().join("financial.db"),"journal":directory.path().join("control.db"),"reconcile_interval_seconds":0,"grants":{"owner":{"all_accounts":true,"scopes":SCOPES}}})).unwrap();
        config.validate().unwrap();
        Self {
            _directory: directory,
            config: Arc::new(config),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
    fn claims(&self) -> Value {
        let now = chrono::Utc::now().timestamp();
        json!({"iss":self.config.issuer,"aud":self.config.resource(),"sub":"owner","iat":now,"exp":now+900,"scope":SCOPES.join(" "),"client_id":"test-agent"})
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
        service
            .call(self.identity(), name.into(), args, Uuid::new_v4())
            .await
    }
    async fn account(&self, service: &Service, name: &str) -> Uuid {
        let reply=self.call(service,"account_create",json!({"name":name,"description":"service contract","environment":"paper","broker":"ibkr","broker_account_id":"DU_TEST","idempotency_key":format!("create-{name}")})).await;
        assert!(reply.error.is_none(), "{reply:?}");
        reply.data.unwrap()["id"].as_str().unwrap().parse().unwrap()
    }
}
struct UncertainBroker(Arc<AtomicUsize>);
impl Broker for UncertainBroker {
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
            assert!(
                !["token", "password", "secret", "url", "path", "command"].contains(&key.as_str())
            );
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
            _ => json!("test-value"),
        },
        "integer" => schema.get("minimum").cloned().unwrap_or(json!(0)),
        "boolean" => json!(false),
        "array" => json!([schema_sample(&schema["items"])]),
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
        let base = tool_arguments(&tool);
        validate(&tool, &base).unwrap_or_else(|e| panic!("{}: {e:?}", tool.name));
        for required in tool.input_schema["required"].as_array().unwrap() {
            let mut args = base.clone();
            args.as_object_mut()
                .unwrap()
                .remove(required.as_str().unwrap());
            assert!(
                validate(&tool, &args).is_err(),
                "{} missing {required}",
                tool.name
            );
            checks += 1;
        }
        for (name, schema) in tool.input_schema["properties"].as_object().unwrap() {
            let mut args = base.clone();
            args[name] = schema_sample(schema);
            assert!(validate(&tool, &args).is_ok(), "{} valid {name}", tool.name);
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
                _ => unreachable!(),
            }
            for value in invalid {
                args[name] = value;
                assert!(
                    validate(&tool, &args).is_err(),
                    "{} accepted invalid {name}: {}",
                    tool.name,
                    args[name]
                );
                checks += 1;
            }
        }
        let mut unknown = base;
        unknown["unexpected_argument"] = json!(true);
        assert!(validate(&tool, &unknown).is_err());
        for value in [json!(null), json!([]), json!(1), json!("arguments")] {
            assert!(validate(&tool, &value).is_err());
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
                    tool_arguments(&tool),
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

#[tokio::test]
async fn manual_tools_record_actual_prices_and_conserve_long_short_cash() {
    for (side, stop, target, fill) in [("long", "90", "120", "99"), ("short", "110", "80", "101")] {
        let fixture = Fixture::new();
        let (service, worker) = fixture.start();
        let account = fixture.account(&service, side).await;
        let setup = [
            (
                "capital_deposit",
                json!({"account_id":account,"amount":"10000","idempotency_key":"setup-deposit"}),
            ),
            (
                "risk_rule_create",
                json!({"account_id":account,"kind":"per_trade","percentage":"0.1","priority":"error","description":"precise cap","idempotency_key":"setup-policy"}),
            ),
        ];
        for (tool, args) in setup {
            let reply = fixture.call(&service, tool, args).await;
            assert!(reply.error.is_none(), "{tool}: {reply:?}");
        }
        let instrument=fixture.call(&service,"instrument_create",json!({"symbol":"RELIABLE","category":"stock","broker":"ibkr","idempotency_key":"setup-instrument"})).await;
        assert!(instrument.error.is_none(), "{instrument:?}");
        let instrument = instrument.data.unwrap()["id"].clone();
        let trade=fixture.call(&service,"trade_prepare",json!({"account_id":account,"instrument_id":instrument,"side":side,"quantity":"1","entry":"100","stop":stop,"target":target,"idempotency_key":"prepare-trade"})).await;
        assert!(trade.error.is_none(), "{trade:?}");
        let id = trade.data.unwrap()["id"].clone();
        let preview = fixture
            .call(
                &service,
                "risk_preview",
                json!({"account_id":account,"entry":"100","stop":stop}),
            )
            .await;
        assert!(preview.error.is_none(), "{preview:?}");
        for (tool, args) in [
            (
                "trade_fund",
                json!({"account_id":account,"trade_id":id,"idempotency_key":"fund-trade-once"}),
            ),
            (
                "trade_record_fill",
                json!({"account_id":account,"trade_id":id,"price":fill,"fee":"0.5","idempotency_key":"record-entry-once"}),
            ),
            (
                "trade_record_target",
                json!({"account_id":account,"trade_id":id,"price":target,"fee":"0.25","idempotency_key":"record-exit-once"}),
            ),
        ] {
            let first = fixture.call(&service, tool, args.clone()).await;
            assert!(first.error.is_none(), "{tool}: {first:?}");
            let again = fixture.call(&service, tool, args).await;
            assert_eq!(first.operation_id, again.operation_id);
            assert_eq!(first.data, again.data);
        }
        let state = fixture
            .call(
                &service,
                "trade_get",
                json!({"account_id":account,"trade_id":id}),
            )
            .await
            .data
            .unwrap();
        assert_eq!(state["entry"]["average_filled_price"], fill);
        assert_eq!(state["entry"]["filled_quantity"], "1");
        let balances = fixture
            .call(&service, "balances_get", json!({"account_id":account}))
            .await
            .data
            .unwrap();
        assert_eq!(
            balances["items"][0]["total_available"]
                .as_str()
                .unwrap()
                .parse::<Decimal>()
                .unwrap(),
            "10020.25".parse::<Decimal>().unwrap()
        );
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
        // All report variants are routed; empty/insufficient analysis remains explicit.
        for kind in [
            "summary",
            "performance",
            "drawdown",
            "risk",
            "concentration",
            "metrics",
            "attribution",
            "benchmark",
            "timeline",
            "bias",
            "wash_sales",
            "tax",
        ] {
            let reply = fixture
                .call(
                    &service,
                    "report_get",
                    json!({"account_id":account,"kind":kind}),
                )
                .await;
            if let Some(error) = reply.error {
                assert_eq!(error.code, "domain_rejected", "report {kind}: {error:?}");
            } else {
                assert!(reply.data.is_some(), "report {kind} has no payload");
            }
        }
        worker.shutdown().await.unwrap();
    }
}
