//! Agent discovery and broker preflight through the shipped authenticated service.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
#[allow(dead_code)]
mod support;
use serde_json::{json, Value};
use std::{
    process::{Command, Stdio},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use support::{Fixture, Gateway};

fn money(value: &Value) -> rust_decimal::Decimal {
    rust_decimal::Decimal::from_str_exact(value.as_str().expect("exact decimal string")).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn discovery_resolves_exact_instruments_and_previews_without_committing_cash_or_orders() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(10000, 32);
    let mut p = f.start(&gateway).await;
    let (account, trade) = f.seed(&p, "long", "1", "discovery").await;
    let original = f
        .ok(
            &p,
            "trade_get",
            json!({"account_id":account,"trade_id":trade}),
        )
        .await;
    let instrument = original["trading_vehicle"]["id"].clone();
    let balances = f
        .ok(&p, "balances_get", json!({"account_id":account}))
        .await;
    let other = f.ok(&p, "account_create", json!({"name":"other","description":"inaccessible to agent","environment":"paper","broker":"ibkr","broker_account_id":"DU_OTHER","idempotency_key":"other-discovery-account"})).await["id"].clone();
    let alpaca = f.ok(&p,"instrument_create",json!({"symbol":"TEST","category":"stock","broker":"alpaca","idempotency_key":"same-symbol-other-broker"})).await["id"].clone();
    f.ok(&p,"instrument_create",json!({"symbol":"BOND","category":"bond","broker":"ibkr","idempotency_key":"missing-bond-terms"})).await;
    assert_eq!(
        f.ok(&p, "instruments_stats", json!({})).await,
        json!({"total":3,"bonds_missing_terms":1})
    );
    for (filter, expected) in [
        (
            json!({"symbol":"test","broker":"IBKR","category":"stock"}),
            vec![instrument.clone()],
        ),
        (json!({"symbol":"TEST","broker":"alpaca"}), vec![alpaca]),
        (json!({"symbol":"TEST","category":"bond"}), vec![]),
    ] {
        let items = f.ok(&p, "instruments_list", filter).await;
        assert_eq!(
            items["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v["id"].clone())
                .collect::<Vec<_>>(),
            expected
        );
    }
    // Repeated resolution must preserve identity rather than creating duplicates.
    for key in [
        "resolve-existing-contract",
        "resolve-existing-contract-again",
    ] {
        let resolved = f.ok(&p,"instrument_resolve",json!({"account_id":account,"symbol":"TEST","category":"stock","idempotency_key":key})).await;
        assert_eq!(resolved["id"], instrument);
        assert_eq!(resolved["broker_asset_id"], "1234");
    }
    let missing = f.call(&p,"instrument_resolve",json!({"account_id":account,"symbol":"OTHER","category":"stock","idempotency_key":"no-fuzzy-symbol-match"}),true).await;
    // Resolution is a persistent mutation. The service conservatively journals
    // domain failures as uncertain; callers must inspect the original intent.
    assert_eq!(missing["error"]["code"], "outcome_unknown");
    assert!(missing["operation_id"].is_string());
    let operation = f
        .ok(&p, "operation_get", json!({"key":"no-fuzzy-symbol-match"}))
        .await;
    assert_eq!(operation["state"], "outcome_unknown");
    assert_eq!(f.ok(&p, "instruments_stats", json!({})).await["total"], 3);
    let preview = f
        .ok(
            &p,
            "broker_preview",
            json!({"account_id":account,"trade_id":trade}),
        )
        .await;
    assert_eq!(
        money(&preview["commission"]["amount"]),
        rust_decimal::Decimal::ONE
    );
    assert_eq!(preview["commission"]["currency"], "USD");
    assert_eq!(
        money(&preview["maintenance_margin_change"]["amount"]),
        rust_decimal::Decimal::from(250)
    );
    assert_eq!(preview["maintenance_margin_change"]["currency"], "USD");
    let mut quote = support::risk::quote();
    let observed = chrono::Utc::now().timestamp_millis();
    quote[0]["_updated"] = json!(observed);
    quote[0]["31"] = json!("100.00");
    quote[0]["7059"] = json!("25");
    *gateway.state.quote.lock().unwrap() = Some(quote);
    let quote = f
        .ok(
            &p,
            "market_quote",
            json!({"account_id":account,"symbol":"TEST"}),
        )
        .await;
    assert_eq!(quote["symbol"], "TEST");
    assert_eq!(
        money(&quote["bid_price"]),
        rust_decimal::Decimal::new(9999, 2)
    );
    assert_eq!(
        money(&quote["ask_price"]),
        rust_decimal::Decimal::new(10001, 2)
    );
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(quote["as_of"].as_str().unwrap())
            .unwrap()
            .timestamp_millis(),
        observed
    );
    let tick = f
        .ok(
            &p,
            "market_trade",
            json!({"account_id":account,"symbol":"TEST"}),
        )
        .await;
    assert_eq!(money(&tick["price"]), rust_decimal::Decimal::from(100));
    assert_eq!(tick["size"], 25);
    assert_eq!(
        f.ok(&p, "balances_get", json!({"account_id":account}))
            .await,
        balances
    );
    let after = f
        .ok(
            &p,
            "trade_get",
            json!({"account_id":account,"trade_id":trade}),
        )
        .await;
    assert_eq!(after["status"], original["status"]);
    assert_eq!(after["balance"], original["balance"]);
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
    assert_eq!(gateway.state.modifications.load(Ordering::SeqCst), 0);
    p.stop().await;

    // Discovery must respect the operator grant after restart, even when the
    // signed agent token contains broader scopes than the grant allows.
    let path = f.dir.path().join("config.json");
    let mut config: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    config["grants"]["agent"]["all_accounts"] = json!(false);
    config["grants"]["agent"]["accounts"] = json!([account]);
    std::fs::write(&path, config.to_string()).unwrap();
    let mut p = f.start(&gateway).await;
    let accounts = f
        .call_as(&p, "agent", "accounts_list", json!({}), true)
        .await
        .unwrap();
    assert!(accounts["error"].is_null(), "{accounts}");
    assert_eq!(accounts["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(accounts["data"]["items"][0]["id"], account);
    let denied = f
        .call_as(
            &p,
            "agent",
            "account_get",
            json!({"account_id":other}),
            true,
        )
        .await
        .unwrap();
    assert_eq!(denied["error"]["code"], "forbidden");
    assert!(denied["data"].is_null());
    let policy = f
        .call_as(&p, "agent", "policy_get", json!({}), true)
        .await
        .unwrap();
    assert!(policy["error"].is_null(), "{policy}");
    assert_eq!(policy["data"]["role"], "agent");
    let scopes = policy["data"]["scopes"].as_array().unwrap();
    assert!(scopes.contains(&json!("trust:read")));
    assert!(!scopes.contains(&json!("trust:admin")) && !scopes.contains(&json!("trust:policy")));
    p.stop().await;
}

#[tokio::test]
async fn invalid_operator_configuration_stops_before_readiness_or_financial_storage() {
    let f = Fixture::new(10000, 32);
    let valid: Value =
        serde_json::from_slice(&std::fs::read(f.dir.path().join("config.json")).unwrap()).unwrap();
    for (case, expected) in [
        ("missing", "Cannot read private operator configuration"),
        ("malformed", "Invalid service configuration"),
        ("public", "public_url must be an absolute URL"),
        ("issuer", "issuer must be an absolute URL"),
        ("remote_http", "HTTPS"),
    ] {
        let path = f.dir.path().join(format!("{case}.json"));
        let mut config = valid.clone();
        match case {
            "missing" => {}
            "malformed" => std::fs::write(&path, b"{synthetic-private-marker}").unwrap(),
            _ => {
                let (field, value) = match case {
                    "public" => ("public_url", "synthetic-private-marker"),
                    "issuer" => ("issuer", "synthetic-private-marker"),
                    _ => ("public_url", "http://192.0.2.1:8000"),
                };
                config[field] = json!(value);
                std::fs::write(&path, config.to_string()).unwrap();
            }
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_trust-service"))
            .arg(&path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let started = Instant::now();
        while child.try_wait().unwrap().is_none() {
            if started.elapsed() > Duration::from_secs(10) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("invalid configuration must exit promptly: {case}");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success(), "{case}");
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(log.contains(expected), "{case}: {log}");
        assert!(
            !log.contains("service_ready") && !log.contains("synthetic-private-marker"),
            "{case}: {log}"
        );
        assert!(!f.dir.path().join("financial.db").exists());
        assert!(!f.dir.path().join("control.db").exists());
    }
}
