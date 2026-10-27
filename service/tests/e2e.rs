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

// Offline, real-process benchmark: JWT verification, HTTP/MCP, queueing, risk
// calculation and FULL-synchronous durable read auditing all stay enabled.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Release performance gate; run via make service-perf"]
async fn real_process_risk_preview_latency_budget() {
    use std::time::Instant;
    for mcp in [false, true] {
        let gateway = Gateway::start().await;
        let f = Fixture::new(5000, 32);
        let mut p = f.start(&gateway).await;
        let (account, _) = f.seed(&p, "long", "10", "performance").await;
        // Sign outside the timed region, as a real agent reuses its access token.
        let token = f.token("agent");
        let args = json!({"account_id":account,"entry":"100","stop":"95"});
        let request = if mcp {
            f.client.post(format!("{}/mcp", p.url)).json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"risk_preview","arguments":args}}))
        } else {
            f.client.post(format!("{}/v1/tools/risk_preview", p.url)).json(&args)
        }.header("host", "localhost:8000")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-11-25")
            .bearer_auth(token);
        let measure = move |request: reqwest::RequestBuilder| async move {
            let start = Instant::now();
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), 200);
            let value: Value = response.json().await.unwrap();
            let reply = if mcp {
                &value["result"]["structuredContent"]
            } else {
                &value
            };
            assert!(reply["error"].is_null(), "{reply}");
            assert!(reply["data"].is_object(), "{reply}");
            start.elapsed()
        };
        for _ in 0..6 {
            measure(request.try_clone().unwrap()).await;
        }
        for concurrency in [1, 4] {
            let mut samples = Vec::new();
            let started = Instant::now();
            for _ in 0..(if concurrency == 1 { 32 } else { 16 }) {
                let mut pending = tokio::task::JoinSet::new();
                for _ in 0..concurrency {
                    pending.spawn(measure(request.try_clone().unwrap()));
                }
                while let Some(sample) = pending.join_next().await {
                    samples.push(sample.unwrap());
                }
            }
            let elapsed = started.elapsed();
            samples.sort_unstable();
            let percentile = |percent: usize| samples[(samples.len() * percent).div_ceil(100) - 1];
            println!(
                "service_perf {}",
                json!({"transport":if mcp{"mcp"}else{"http"},"concurrency":concurrency,"samples":samples.len(),"elapsed_us":elapsed.as_micros(),"p50_us":percentile(50).as_micros(),"p95_us":percentile(95).as_micros(),"p99_us":percentile(99).as_micros(),"max_us":samples.last().unwrap().as_micros()})
            );
            let budget = if concurrency == 1 { 50 } else { 200 };
            assert!(
                percentile(95) < Duration::from_millis(budget),
                "p95 exceeded {budget}ms at concurrency {concurrency}: {samples:?}"
            );
        }
        let verified = f.ok(&p, "audit_verify", json!({})).await;
        assert!(verified["events"].as_u64().unwrap() >= 102);
        assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
        let journal = f.journal();
        let audited: i64 = journal
            .query_row(
                "SELECT COUNT(*) FROM audit WHERE json_extract(event, '$.tool') = 'risk_preview'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            audited, 102,
            "every measured and warm-up read must be audited"
        );
        p.stop().await;
    }
}

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
        // Either concurrent transport can claim the intent. Only the winner
        // contacts the broker; the replay has its own request correlation ID.
        let submitting_request_id = submit["event"]["request_id"].clone();
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
        assert!(metrics.contains("trust_tool_duration_seconds_count{tool=\"trade_submit\"} 3\n"));
        assert!(metrics.contains("trust_stage_duration_seconds_count{stage=\"journal_finish\"}"));
        assert!(metrics.contains("trust_http_responses_total{route=\"mcp\",outcome=\"success\"}"));
        assert!(metrics.contains("trust_log_dropped_lines_total 0\n"));
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
        assert!(logs.contains("queue_wait_us"));
        let broker_logs: Vec<Value> = logs
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|event| event["fields"]["event"] == "broker_request")
            .collect();
        assert!(!broker_logs.is_empty());
        for event in &broker_logs {
            let fields = &event["fields"];
            assert!(fields["duration_us"].is_u64());
            assert!(!event.to_string().contains("DU_TEST"));
            assert!(fields["endpoint"]
                .as_str()
                .is_some_and(|endpoint| !endpoint.contains('/')));
        }
        assert!(
            broker_logs
                .iter()
                .any(|event| event["spans"].as_array().is_some_and(|spans| spans
                    .iter()
                    .any(|span| span["request_id"] == submitting_request_id))),
            "broker attempts must correlate to the originating request"
        );
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

#[tokio::test]
async fn agent_trades_within_owner_policy_and_can_only_tighten_stops() {
    for side in ["long", "short"] {
        let gateway = Gateway::start().await;
        let f = Fixture::new(5000, 8);
        let mut p = f.start(&gateway).await;
        let (account, template) = f.seed(&p, side, "10", side).await;
        let instrument = f
            .ok(
                &p,
                "trade_get",
                json!({"account_id":account,"trade_id":template}),
            )
            .await["trading_vehicle"]["id"]
            .clone();
        let sized = f
            .call_as(
                &p,
                "agent",
                "risk_preview",
                json!({"account_id":account,"entry":"100","stop":if side=="long"{"95"}else{"105"}}),
                true,
            )
            .await
            .unwrap();
        assert!(sized["error"].is_null(), "{sized}");
        let prepared = f.call_as(&p, "agent", "trade_prepare", json!({"account_id":account,"instrument_id":instrument,"side":side,"quantity":"10","entry":"100","stop":if side=="long"{"95"}else{"105"},"target":if side=="long"{"110"}else{"90"},"idempotency_key":"agent-prepare-own-trade"}), true).await.unwrap();
        assert!(prepared["error"].is_null(), "{prepared}");
        let trade = prepared["data"]["id"].clone();
        let discovery: Value = f
            .client
            .post(format!("{}/mcp", p.url))
            .header("host", "localhost:8000")
            .header("accept", "application/json, text/event-stream")
            .bearer_auth(f.token("agent"))
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let tools = discovery["result"]["tools"].as_array().unwrap();
        assert!(tools.iter().any(|tool| tool["name"] == "trade_submit"));
        for owner_tool in [
            "capital_deposit",
            "capital_withdraw",
            "risk_rule_create",
            "trade_record_fill",
            "backup_restore",
        ] {
            assert!(!tools.iter().any(|tool| tool["name"] == owner_tool));
        }
        for (name, extra) in [
            ("capital_deposit", json!({"amount":"1000"})),
            ("capital_withdraw", json!({"amount":"1"})),
            (
                "risk_rule_create",
                json!({"kind":"per_trade","percentage":"100","priority":"error","description":"agent escalation"}),
            ),
        ] {
            let mut args = extra;
            args["account_id"] = account.clone();
            args["idempotency_key"] = json!(format!("agent-denied-{name}"));
            let denied = f.call_as(&p, "agent", name, args, true).await.unwrap();
            assert_eq!(error(&denied), "forbidden");
            assert!(denied["operation_id"].is_null());
        }
        for name in ["trade_fund", "trade_submit"] {
            let reply = f.call_as(&p, "agent", name, json!({"account_id":account,"trade_id":trade,"idempotency_key":format!("agent-{name}")}), true).await.unwrap();
            assert!(reply["error"].is_null(), "{reply}");
        }
        gateway.state.fills("10", "0", false);
        let sync = f
            .call_as(
                &p,
                "agent",
                "trade_reconcile",
                json!({"account_id":account,"trade_id":trade,"idempotency_key":"agent-fill-sync"}),
                true,
            )
            .await
            .unwrap();
        assert!(sync["error"].is_null(), "{sync}");
        let (first, looser, tighter) = if side == "long" {
            ("102", "101", "103")
        } else {
            ("98", "99", "97")
        };
        let move_stop = |price: &str, key: &str| json!({"account_id":account,"trade_id":trade,"price":price,"idempotency_key":key});
        let args = move_stop(first, "agent-protect-profit");
        let protected = f
            .call_as(&p, "agent", "trade_modify_stop", args.clone(), true)
            .await
            .unwrap();
        assert!(protected["error"].is_null(), "{protected}");
        let replay = f
            .call_as(&p, "agent", "trade_modify_stop", args, true)
            .await
            .unwrap();
        assert_eq!(protected["operation_id"], replay["operation_id"]);
        assert_eq!(gateway.state.modifications.load(Ordering::SeqCst), 1);
        // A stale/weaker broker observation changes accounting evidence but
        // cannot reset the previously acknowledged amendment bound.
        for order in gateway.state.orders.lock().unwrap().iter_mut() {
            if order["leg"] == "trust-stop" {
                order["price"] = json!(if side == "long" { "95" } else { "105" });
            }
        }
        let observed = f.call_as(&p, "agent", "trade_reconcile", json!({"account_id":account,"trade_id":trade,"idempotency_key":"agent-weaker-snapshot"}), true).await.unwrap();
        assert!(observed["error"].is_null(), "{observed}");
        for subject in ["agent", "owner"] {
            for price in [looser, "0", "-1"] {
                let denied = f
                    .call_as(
                        &p,
                        subject,
                        "trade_modify_stop",
                        move_stop(price, &format!("denied-{subject}-{price}")),
                        true,
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    error(&denied),
                    if price == looser {
                        "stop_would_increase_risk"
                    } else {
                        "invalid_stop_price"
                    }
                );
                assert_eq!(gateway.state.modifications.load(Ordering::SeqCst), 1);
            }
        }
        let tightened = f
            .call_as(
                &p,
                "agent",
                "trade_modify_stop",
                move_stop(tighter, "agent-trail-profit"),
                true,
            )
            .await
            .unwrap();
        assert!(tightened["error"].is_null(), "{tightened}");
        assert_eq!(gateway.state.modifications.load(Ordering::SeqCst), 2);
        let closed = f.call_as(&p, "agent", "trade_close", json!({"account_id":account,"trade_id":trade,"idempotency_key":"agent-close-position"}), true).await.unwrap();
        assert!(closed["error"].is_null(), "{closed}");
        assert_eq!(gateway.state.modifications.load(Ordering::SeqCst), 3);
        assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 1);
        p.stop().await;
    }
}
