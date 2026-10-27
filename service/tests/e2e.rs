#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
mod support;
use serde_json::{json, Value};
use std::{sync::atomic::Ordering, time::Duration};
use support::{Fixture, Gateway, SENTINEL};

fn pending(
    f: &Fixture,
    p: &support::Process,
    tool: &str,
    args: Value,
) -> tokio::task::JoinHandle<Result<Value, reqwest::Error>> {
    let request = f
        .client
        .post(format!("{}/v1/tools/{tool}", p.url))
        .header("host", "localhost:8000")
        .bearer_auth(f.token("owner"))
        .json(&args);
    tokio::spawn(async move { request.send().await?.json().await })
}
fn error(reply: &Value) -> &str {
    reply["error"]["code"].as_str().unwrap()
}

#[tokio::test]
async fn real_process_mcp_ibkr_lifecycles_replay_audit_metrics_and_redaction() {
    for side in ["long", "short"] {
        let gateway = Gateway::start().await;
        let f = Fixture::new(5000, 8);
        let mut p = f.start(&gateway).await;
        assert_eq!(
            f.request(&p, "/metrics", None)
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        assert_eq!(
            f.request(&p, "/metrics", Some("reader"))
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        let (account, trade) = f.seed(&p, side, "10", side).await;
        f.ok(
            &p,
            "trade_fund",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"fund-test"}),
        )
        .await;
        let args = json!({"account_id":account,"trade_id":trade,"idempotency_key":"submit-test"});
        let (http, mcp) = tokio::join!(
            f.call(&p, "trade_submit", args.clone(), false),
            f.call(&p, "trade_submit", args.clone(), true)
        );
        assert!(http["error"].is_null(), "{http}");
        assert_eq!(http["operation_id"], mcp["operation_id"]);
        assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 1);
        gateway.state.fail_activities.store(true, Ordering::SeqCst);
        for (entry, exit, expected) in [
            ("4", "0", "partially_filled"),
            ("10", "0", "filled"),
            ("10", "10", "closed_target"),
        ] {
            gateway.state.fills(entry, exit, false);
            let data=f.ok(&p,"trade_reconcile",json!({"account_id":account,"trade_id":trade,"idempotency_key":format!("sync-{entry}-{exit}")})).await;
            // Public state spelling is defined by the domain enum serializer.
            let state = data["status"]
                .as_str()
                .unwrap_or_default()
                .to_lowercase()
                .replace('_', "");
            assert_eq!(state, expected.replace('_', ""), "{data}");
        }
        let balance = f
            .ok(&p, "balances_get", json!({"account_id":account}))
            .await;
        assert!(balance.to_string().contains("10100"), "{balance}");
        let recipient=f.ok(&p,"account_create",json!({"name":"recipient","account_type":"earnings","parent_account_id":account,"description":"E2E transfer","environment":"paper","broker":"ibkr","broker_account_id":"DU_TEST","idempotency_key":"recipient-account"})).await["id"].clone();
        f.ok(&p,"capital_transfer",json!({"account_id":account,"to_account_id":recipient,"amount":"1","reason":"E2E atomic transfer","idempotency_key":"transfer-once"})).await;
        let recipient_balance = f
            .ok(&p, "balances_get", json!({"account_id":recipient}))
            .await;
        assert!(
            recipient_balance
                .to_string()
                .contains("\"total_available\":\"1\""),
            "{recipient_balance}"
        );
        let invalid = f
            .call(&p, "capital_deposit", json!({"amount":SENTINEL}), false)
            .await;
        assert_eq!(error(&invalid), "invalid_argument");
        let conflict = f
            .call(
                &p,
                "trade_submit",
                json!({"account_id":account,"trade_id":trade,"idempotency_key":"fund-test"}),
                true,
            )
            .await;
        assert_eq!(error(&conflict), "idempotency_conflict");
        let audit = f.ok(&p, "audit_list", json!({"limit":200})).await;
        let events = audit["events"].as_array().unwrap();
        assert!(events.iter().any(|e| e["event"]["outcome"] == "replayed"));
        assert!(events
            .iter()
            .any(|e| e["event"]["error_code"] == "idempotency_conflict"));
        assert!(events
            .iter()
            .any(|e| e["event"]["request_id"] == invalid["request_id"]));
        let submit = events
            .iter()
            .find(|e| {
                e["event"]["operation_id"] == http["operation_id"]
                    && e["event"]["outcome"] == "completed"
            })
            .unwrap();
        assert_eq!(submit["event"]["version"], 2);
        assert_eq!(submit["event"]["account_ids"], json!([account]));
        assert!(submit["event"]["duration_ms"].is_u64());
        assert_eq!(submit["event_hash"].as_str().unwrap().len(), 64);
        f.ok(&p, "audit_verify", json!({})).await;
        let metrics = f
            .request(&p, "/metrics", Some("owner"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        assert!(metrics.contains("trust_worker_ready 1\n"));
        assert!(metrics.contains("trust_replays_total 1\n"));
        assert!(metrics.contains("trust_execution_milliseconds_bucket"));
        assert!(!metrics.contains(account.as_str().unwrap()));
        // Probes do not consume the ordinary global request budget.
        for _ in 0..610 {
            assert_eq!(
                f.request(&p, "/healthz", None)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                200
            );
        }
        assert_eq!(
            f.request(&p, "/readyz", None)
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        p.stop().await;
        let logs = p.log_text();
        assert!(!logs.contains(SENTINEL));
        assert!(!logs.contains(&f.token("owner")));
        assert!(logs.contains("execution_reconciliation_failed"));
        assert!(logs.contains("queue_wait_ms"));
        assert!(logs.contains(http["request_id"].as_str().unwrap()));
        let mut p = f.start(&gateway).await;
        let replay = f.call(&p, "trade_submit", args, true).await;
        assert_eq!(replay["operation_id"], http["operation_id"]);
        assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 1);
        p.stop().await;
    }
}

#[tokio::test]
async fn risk_rejection_is_definitive_and_leaves_cash_and_broker_untouched() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(5000, 8);
    let mut p = f.start(&gateway).await;
    let (account, trade) = f.seed(&p, "long", "30", "risk").await;
    let before = f
        .ok(&p, "balances_get", json!({"account_id":account}))
        .await;
    let reply = f
        .call(
            &p,
            "trade_fund",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"too-risky"}),
            true,
        )
        .await;
    assert!(
        matches!(
            error(&reply),
            "risk_per_trade_exceeded" | "level_quantity_exceeded"
        ),
        "{reply}"
    );
    let after = f
        .ok(&p, "balances_get", json!({"account_id":account}))
        .await;
    assert_eq!(before, after);
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
    let operation = f.ok(&p, "operation_get", json!({"key":"too-risky"})).await;
    assert_eq!(operation["state"], "failed");
    let audit = f.ok(&p, "audit_list", json!({})).await;
    assert!(audit["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["event"]["error_code"] == reply["error"]["code"]));
    p.stop().await;
}

#[tokio::test]
async fn crash_after_broker_acceptance_recovers_unknown_without_resubmission_and_detects_tamper() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(5000, 8);
    let mut p = f.start(&gateway).await;
    let (account, trade) = f.seed(&p, "long", "10", "crash").await;
    f.ok(
        &p,
        "trade_fund",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"fund-test"}),
    )
    .await;
    gateway.state.hold.store(true, Ordering::SeqCst);
    let args = json!({"account_id":account,"trade_id":trade,"idempotency_key":"crash-submit"});
    let call = pending(&f, &p, "trade_submit", args.clone());
    f.accepted(&gateway).await;
    p.child.kill().unwrap();
    assert!(!p.wait().await.success());
    assert!(call.await.unwrap().is_err());
    gateway.state.release.add_permits(1);
    let mut p = f.start(&gateway).await;
    let reply = f.call(&p, "trade_submit", args, true).await;
    assert_eq!(error(&reply), "outcome_unknown");
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 1);
    let audit = f.ok(&p, "audit_list", json!({})).await;
    let recovery = audit["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["event"]["outcome"] == "recovered_unknown")
        .unwrap();
    assert_eq!(recovery["event"]["operation_id"], reply["operation_id"]);
    assert_eq!(recovery["event"]["source"], "recovery");
    f.ok(&p, "audit_verify", json!({})).await;
    p.stop().await;
    f.journal()
        .execute_batch("DROP TRIGGER audit_no_update; UPDATE audit SET event='{}' WHERE sequence=1")
        .unwrap();
    let mut failed = f.spawn(&gateway);
    assert!(!failed.wait().await.success());
    assert!(failed.log_text().contains("audit_integrity_failed"));
}

#[tokio::test]
async fn queue_timeout_never_dispatches_expired_intent_and_audit_failure_stops_worker() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(400, 1);
    let mut p = f.start(&gateway).await;
    let (account, trade) = f.seed(&p, "long", "10", "queue").await;
    f.ok(
        &p,
        "trade_fund",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"fund-test"}),
    )
    .await;
    gateway.state.hold.store(true, Ordering::SeqCst);
    let submit = pending(
        &f,
        &p,
        "trade_submit",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"slow-submit"}),
    );
    f.accepted(&gateway).await;
    let deposit = json!({"account_id":account,"amount":"5","idempotency_key":"queued-deposit"});
    let queued = pending(&f, &p, "capital_deposit", deposit.clone());
    let start = std::time::Instant::now();
    loop {
        let metrics = f
            .request(&p, "/metrics", Some("owner"))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        if metrics.contains("trust_queue_depth 1\n") {
            break;
        }
        assert!(start.elapsed() < Duration::from_millis(300));
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let busy = f.call(&p, "service_status", json!({}), false).await;
    assert_eq!(error(&busy), "busy");
    assert_eq!(
        f.request(&p, "/readyz", None)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(error(&submit.await.unwrap().unwrap()), "outcome_unknown");
    assert_eq!(error(&queued.await.unwrap().unwrap()), "request_expired");
    gateway.state.release.add_permits(1);
    // Observe completion without dispatching new financial work into the full queue.
    let start = std::time::Instant::now();
    loop {
        let count:i64=f.journal().query_row("SELECT count(*) FROM operations WHERE request_key='slow-submit' AND state='completed'",[],|row|row.get(0)).unwrap();
        if count == 1 {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(3));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let count: i64 = f
        .journal()
        .query_row(
            "SELECT count(*) FROM operations WHERE request_key='queued-deposit'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 0,
        "expired request must never acquire durable intent"
    );
    f.ok(&p, "capital_deposit", deposit).await;
    let metrics = f
        .request(&p, "/metrics", Some("owner"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(metrics.contains("trust_queued_cancellations_total 1\n"));
    assert!(metrics.contains("trust_unknown_responses_total 1\n"));
    assert!(metrics.contains("trust_queue_rejections_total 1\n"));
    f.journal().execute_batch("CREATE TRIGGER fail_audit BEFORE INSERT ON audit BEGIN SELECT RAISE(ABORT,'simulated disk failure'); END;").unwrap();
    let reply = f
        .call(
            &p,
            "capital_deposit",
            json!({"account_id":account,"amount":"900","idempotency_key":"must-not-run"}),
            true,
        )
        .await;
    assert_eq!(error(&reply), "audit_unavailable");
    assert_eq!(
        f.request(&p, "/readyz", None)
            .send()
            .await
            .unwrap()
            .status(),
        503
    );
    let metrics = f
        .request(&p, "/metrics", Some("owner"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(metrics.contains("trust_worker_ready 0\n"));
    assert!(metrics.contains("trust_audit_failures_total 1\n"));
    assert_eq!(
        f.journal()
            .query_row(
                "SELECT count(*) FROM operations WHERE request_key='must-not-run'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    p.stop().await;
}

#[tokio::test]
async fn graceful_shutdown_finishes_active_operation_before_process_exit() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(5000, 8);
    let mut p = f.start(&gateway).await;
    let (account, trade) = f.seed(&p, "long", "10", "shutdown").await;
    f.ok(
        &p,
        "trade_fund",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"fund-test"}),
    )
    .await;
    gateway.state.hold.store(true, Ordering::SeqCst);
    let args = json!({"account_id":account,"trade_id":trade,"idempotency_key":"shutdown-submit"});
    let call = pending(&f, &p, "trade_submit", args.clone());
    f.accepted(&gateway).await;
    p.signal();
    let start = std::time::Instant::now();
    while !p.log_text().contains("shutdown_started") {
        assert!(start.elapsed() < Duration::from_secs(2));
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(p.child.try_wait().unwrap().is_none());
    gateway.state.release.add_permits(1);
    let reply = call.await.unwrap().unwrap();
    assert!(reply["error"].is_null(), "{reply}");
    assert!(p.wait().await.success());
    let mut p = f.start(&gateway).await;
    let replay = f.call(&p, "trade_submit", args, true).await;
    assert_eq!(reply["operation_id"], replay["operation_id"]);
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 1);
    p.stop().await;
}

#[tokio::test]
async fn mcp_submission_observes_policy_changes_after_funding() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(5000, 8);
    let mut p = f.start(&gateway).await;
    let (account, trade) = f.seed(&p, "long", "10", "policy-change").await;
    f.ok(
        &p,
        "trade_fund",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"policy-fund"}),
    )
    .await;
    let rules = f
        .ok(&p, "risk_rules_list", json!({"account_id":account}))
        .await;
    // Remove all rules through operator authority, then install a zero trade cap.
    let rules = rules
        .as_array()
        .or_else(|| rules["items"].as_array())
        .unwrap();
    for rule in rules {
        f.ok(&p, "risk_rule_remove", json!({"account_id":account,"rule_id":rule["id"],"idempotency_key":format!("remove-{}",rule["id"])})).await;
    }
    for (kind, cap) in [("per_trade", "0"), ("per_month", "5")] {
        f.ok(&p, "risk_rule_create", json!({"account_id":account,"kind":kind,"percentage":cap,"priority":"error","description":"operator pause","idempotency_key":format!("replace-{kind}")})).await;
    }
    let balances = f
        .ok(&p, "balances_get", json!({"account_id":account}))
        .await;
    let denied = f
        .call(
            &p,
            "trade_submit",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"paused-submit"}),
            true,
        )
        .await;
    assert_eq!(error(&denied), "risk_per_trade_exceeded");
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
    assert_eq!(
        f.ok(&p, "balances_get", json!({"account_id":account}))
            .await,
        balances
    );
    assert_eq!(
        f.ok(
            &p,
            "trade_get",
            json!({"account_id":account,"trade_id":trade})
        )
        .await["status"],
        "Funded"
    );
    p.stop().await;
}
