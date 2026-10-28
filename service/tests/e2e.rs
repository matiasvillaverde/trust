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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn encrypted_process_lifecycle_restart_backup_restore_and_wrong_key_fail_closed() {
    use std::os::unix::fs::PermissionsExt;
    let gateway = Gateway::start().await;
    let mut f = Fixture::new(10000, 32);
    f.enable_encryption();
    let mut process = f.start(&gateway).await;
    let (account, trade) = f
        .seed(&process, "long", "10", "encrypted-lifecycle-marker")
        .await;
    f.ok(
        &process,
        "trade_fund",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"encrypted-fund"}),
    )
    .await;
    let submit =
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"encrypted-submit"});
    let original = f.call(&process, "trade_submit", submit.clone(), true).await;
    assert!(original["error"].is_null());
    gateway.state.fills("10", "10", false);
    let reconciled = f
        .ok(
            &process,
            "trade_reconcile",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"encrypted-fill"}),
        )
        .await;
    assert_eq!(reconciled["status"], "ClosedTarget");
    let balances = f
        .ok(&process, "balances_get", json!({"account_id":account}))
        .await;
    let backup = f
        .ok(
            &process,
            "backup_export",
            json!({"idempotency_key":"encrypted-backup"}),
        )
        .await;
    let backup_id = backup["backup_id"].as_str().unwrap();
    let data = f.dir.path().join("data");
    for entry in std::fs::read_dir(&data).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_file() {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3"));
        assert!(!bytes
            .windows(b"encrypted-lifecycle-marker".len())
            .any(|window| window == b"encrypted-lifecycle-marker"));
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let backup_path = data.join("backups").join(format!("{backup_id}.db"));
    assert!(!std::fs::read(&backup_path)
        .unwrap()
        .starts_with(b"SQLite format 3"));
    let count: i64 = f
        .journal()
        .query_row("SELECT COUNT(*) FROM audit", [], |row| row.get(0))
        .unwrap();
    assert!(count > 10);
    process.stop().await;
    let mut restarted = f.start(&gateway).await;
    let replay = f.call(&restarted, "trade_submit", submit, true).await;
    assert_eq!(replay["operation_id"], original["operation_id"]);
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 1);
    assert_eq!(
        f.ok(&restarted, "balances_get", json!({"account_id":account}))
            .await,
        balances
    );
    f.ok(&restarted, "audit_verify", json!({})).await;
    restarted.stop().await;

    let mut restore = Fixture::new(10000, 32);
    restore.enable_encryption();
    std::fs::copy(
        f.dir.path().join("secrets/backup-key.hex"),
        restore.dir.path().join("secrets/backup-key.hex"),
    )
    .unwrap();
    let backup_directory = restore.dir.path().join("data/backups");
    trust_security::private_directory(&backup_directory).unwrap();
    std::fs::copy(
        &backup_path,
        backup_directory.join(format!("{backup_id}.db")),
    )
    .unwrap();
    let mut restored = restore.start(&gateway).await;
    restore
        .ok(
            &restored,
            "backup_restore",
            json!({"backup_id":backup_id,"idempotency_key":"encrypted-restore"}),
        )
        .await;
    assert_eq!(
        restore
            .ok(&restored, "balances_get", json!({"account_id":account}))
            .await,
        balances
    );
    let duplicate = restore
        .call(
            &restored,
            "backup_restore",
            json!({"backup_id":backup_id,"idempotency_key":"second-restore"}),
            true,
        )
        .await;
    assert!(!duplicate["error"].is_null());
    restored.stop().await;
    // Rotate only the file in this disposable fixture: production must copy/rekey
    // storage first. A mismatch must stop startup, never create a fresh ledger.
    let replacement = f.dir.path().join("secrets/replacement-key.hex");
    trust_security::DatabaseKey::generate()
        .unwrap()
        .write_new(&replacement)
        .unwrap();
    std::fs::rename(&replacement, f.dir.path().join("secrets/financial-key.hex")).unwrap();
    let mut failed = f.spawn(&gateway);
    assert!(!failed.wait().await.success());
    for name in ["financial", "journal", "backup"] {
        let secret =
            trust_security::read_secret(&f.dir.path().join(format!("secrets/{name}-key.hex")), 66)
                .unwrap();
        let key = std::str::from_utf8(&secret).unwrap();
        assert!(!process.log_text().contains(key));
        assert!(!restarted.log_text().contains(key));
        assert!(!failed.log_text().contains(key));
    }
    assert!(!failed.log_text().contains("service_ready"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn failed_advisory_and_market_reads_do_not_create_unknown_trading_outcomes() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(5000, 32);
    let mut p = f.start(&gateway).await;
    let (account, trade) = f.seed(&p, "long", "10", "read-failure").await;
    for tool in ["advisor_regime", "market_snapshot"] {
        let reply = f
            .call(
                &p,
                tool,
                json!({"account_id":account,"symbol":"TEST"}),
                true,
            )
            .await;
        assert_eq!(reply["error"]["code"], "domain_rejected", "{reply}");
        assert!(reply["operation_id"].is_null());
        let ready = f
            .ok(&p, "account_readiness", json!({"account_id":account}))
            .await;
        assert_eq!(ready["halted"], false, "{ready}");
        assert_eq!(ready["unresolved_operation_count"], 0);
    }
    f.ok(
        &p,
        "trade_fund",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"fund-after-read-failure"}),
    )
    .await;
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
    p.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn whole_account_reservations_and_halt_survive_restart_and_preserve_cancellation() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(5000, 32);
    let mut p = f.start(&gateway).await;
    let (account, trade) = f.seed(&p, "long", "10", "shared").await;
    let instrument = f
        .ok(
            &p,
            "trade_get",
            json!({"account_id":account,"trade_id":trade}),
        )
        .await["trading_vehicle"]["id"]
        .clone();
    let other=f.ok(&p,"account_create",json!({"name":"second profile","description":"same external account","broker":"ibkr","environment":"paper","broker_account_id":"DU_TEST","idempotency_key":"second-profile"})).await["id"].clone();
    f.ok(
        &p,
        "capital_deposit",
        json!({"account_id":other,"amount":"10000","idempotency_key":"second-cash"}),
    )
    .await;
    for kind in ["per_trade", "per_month"] {
        f.ok(&p,"risk_rule_create",json!({"account_id":other,"kind":kind,"percentage":"5","priority":"error","description":"test","idempotency_key":format!("second-{kind}")})).await;
    }
    f.ok(&p,"account_safety_policy_set",json!({"account_id":account,"currency":"USD","max_gross_notional":"1500","max_order_notional":"1000","max_daily_loss":"500","max_drawdown":"1000","max_snapshot_age_seconds":120,"controls":support::risk::controls(),"idempotency_key":"shared-cap"})).await;
    let second=f.ok(&p,"trade_prepare",json!({"account_id":other,"instrument_id":instrument,"side":"long","quantity":"10","entry":"100","stop":"95","target":"110","idempotency_key":"second-trade"})).await["id"].clone();
    f.ok(
        &p,
        "trade_fund",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"shared-fund"}),
    )
    .await;
    let rejected = f
        .call(
            &p,
            "trade_fund",
            json!({"account_id":other,"trade_id":second,"idempotency_key":"second-fund"}),
            true,
        )
        .await;
    assert_eq!(rejected["error"]["code"], "account_exposure_exceeded");
    f.ok(
        &p,
        "account_halt",
        json!({"account_id":account,"reason":"Test durable pause","idempotency_key":"shared-halt"}),
    )
    .await;
    p.stop().await;
    let mut p = f.start(&gateway).await;
    let readiness = f
        .ok(&p, "account_readiness", json!({"account_id":other}))
        .await;
    assert_eq!(readiness["halted"], true);
    let denied = f
        .call_as(
            &p,
            "agent",
            "account_resume",
            json!({"account_id":other,"reason":"Agent attempt","idempotency_key":"agent-resume"}),
            true,
        )
        .await
        .unwrap();
    assert_eq!(denied["error"]["code"], "forbidden");
    f.ok(
        &p,
        "trade_cancel",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"cancel-under-halt"}),
    )
    .await;
    f.ok(&p,"account_resume",json!({"account_id":other,"reason":"Fresh evidence verified","idempotency_key":"owner-resume"})).await;
    f.ok(
        &p,
        "trade_fund",
        json!({"account_id":other,"trade_id":second,"idempotency_key":"new-intent-after-recovery"}),
    )
    .await;
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
    p.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn external_positions_unrealized_losses_and_snapshot_failures_block_new_exposure() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(5000, 32);
    let mut p = f.start(&gateway).await;
    let (account, trade) = f.seed(&p, "long", "10", "broker-risk").await;
    f.ok(&p,"account_safety_policy_set",json!({"account_id":account,"currency":"USD","max_gross_notional":"2500","max_order_notional":"1000","max_daily_loss":"500","max_drawdown":"1000","max_snapshot_age_seconds":120,"controls":support::risk::controls(),"idempotency_key":"external-cap"})).await;
    *gateway.state.positions.lock().unwrap() = Some(
        json!([{"acctId":"DU_TEST","conid":"external","contractDesc":"MANUAL","currency":"USD","assetClass":"STK","position":"-20","mktValue":"-2000"}]),
    );
    let rejected = f
        .call(
            &p,
            "trade_fund",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"external-fund"}),
            true,
        )
        .await;
    assert_eq!(rejected["error"]["code"], "instrument_risk_policy_missing");
    *gateway.state.positions.lock().unwrap() = None;
    f.ok(&p,"account_resume",json!({"account_id":account,"reason":"External exposure resolved and broker verified","idempotency_key":"external-recovery"})).await;
    f.ok(
        &p,
        "trade_fund",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"within-exposure"}),
    )
    .await;
    *gateway.state.ledger.lock().unwrap() = Some(
        json!({"BASE":{"acctcode":"DU_TEST","netliquidationvalue":"9500","cashbalance":"10000","realizedpnl":"0","unrealizedpnl":"-500","timestamp":chrono::Utc::now().timestamp()}}),
    );
    let loss = f
        .call(
            &p,
            "account_refresh",
            json!({"account_id":account,"idempotency_key":"loss-observation"}),
            true,
        )
        .await;
    assert_eq!(loss["error"]["code"], "daily_equity_loss_exceeded");
    let blocked = f
        .call(
            &p,
            "trade_submit",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"loss-submit"}),
            true,
        )
        .await;
    assert!(!blocked["error"].is_null());
    *gateway.state.ledger.lock().unwrap() = None;
    p.stop().await;
    let mut p = f.start(&gateway).await;
    let readiness = f
        .ok(&p, "account_readiness", json!({"account_id":account}))
        .await;
    assert_eq!(readiness["halted"], true);
    f.ok(&p,"account_resume",json!({"account_id":account,"reason":"Restored equity and complete snapshot","idempotency_key":"loss-recovery"})).await;
    gateway.state.fail_snapshot.store(true, Ordering::SeqCst);
    let failed = f
        .call(
            &p,
            "trade_submit",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"unavailable-submit"}),
            true,
        )
        .await;
    assert_eq!(failed["error"]["code"], "broker_snapshot_unavailable");
    gateway.state.fail_snapshot.store(false, Ordering::SeqCst);
    assert_eq!(
        f.ok(&p, "account_readiness", json!({"account_id":account}))
            .await["halted"],
        true
    );
    f.ok(
        &p,
        "trade_cancel",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"protect-during-outage"}),
    )
    .await;
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
    p.stop().await;
}

// Offline, real-process benchmark: JWT verification, HTTP/MCP, queueing, risk
// calculation and FULL-synchronous durable read auditing all stay enabled.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Release performance gate; run via make service-perf"]
async fn real_process_risk_preview_latency_budget() {
    use std::time::Instant;
    for mcp in [false, true] {
        let gateway = Gateway::start().await;
        let mut f = Fixture::new(5000, 32);
        f.enable_encryption();
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
                json!({"encrypted_storage":true,"transport":if mcp{"mcp"}else{"http"},"concurrency":concurrency,"samples":samples.len(),"elapsed_us":elapsed.as_micros(),"p50_us":percentile(50).as_micros(),"p95_us":percentile(95).as_micros(),"p99_us":percentile(99).as_micros(),"max_us":samples.last().unwrap().as_micros()})
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn portfolio_heat_is_atomic_across_agents_and_profiles_and_preview_never_reserves() {
    use rust_decimal::Decimal;
    let gateway = Gateway::start().await;
    let mut f = Fixture::new(5000, 32);
    f.add_agent("agent2");
    let mut p = f.start(&gateway).await;
    let (account, seed) = f.seed(&p, "long", "10", "heat").await;
    let instrument = f
        .ok(
            &p,
            "trade_get",
            json!({"account_id":account,"trade_id":seed}),
        )
        .await["trading_vehicle"]["id"]
        .clone();
    let other=f.ok(&p,"account_create",json!({"name":"second-profile","description":"same broker","environment":"paper","broker":"ibkr","broker_account_id":"DU_TEST","idempotency_key":"heat-other"})).await["id"].clone();
    f.ok(
        &p,
        "capital_deposit",
        json!({"account_id":other,"amount":"10000","idempotency_key":"heat-deposit"}),
    )
    .await;
    for kind in ["per_trade", "per_month"] {
        f.ok(&p,"risk_rule_create",json!({"account_id":other,"kind":kind,"percentage":"5","priority":"error","description":"test","idempotency_key":format!("heat-{kind}")})).await;
    }
    let mut controls = support::risk::controls();
    controls["max_portfolio_heat_pct"] = json!("0.6");
    controls["max_trade_heat_pct"] = json!("0.6");
    f.ok(&p,"account_safety_policy_set",json!({"account_id":account,"currency":"USD","max_gross_notional":"100000","max_order_notional":"10000","max_daily_loss":"500","max_drawdown":"1000","max_snapshot_age_seconds":120,"controls":controls,"idempotency_key":"heat-cap"})).await;
    f.ok(&p,"agent_mandate_set",json!({"account_id":other,"subject":"agent2","instrument_ids":[instrument],"max_order_notional":"10000","max_total_notional":"100000","max_open_trades":100,"allow_short":true,"expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339(),"risk_budget":support::risk::budget(),"idempotency_key":"heat-agent2"})).await;
    let proposal = json!({"account_id":account,"instrument_id":instrument,"side":"long","quantity":"10","entry":"100","stop":"95","target":"110"});
    let before: i64 = f
        .journal()
        .query_row("SELECT COUNT(*) FROM account_safety_history", [], |row| {
            row.get(0)
        })
        .unwrap();
    for _ in 0..2 {
        let r = f
            .call_as(&p, "agent", "risk_check", proposal.clone(), true)
            .await
            .unwrap();
        assert!(r["error"].is_null(), "{r}");
        assert_eq!(r["data"]["requested_admissible"], true);
        assert_eq!(r["data"]["reserves_capacity"], false);
        assert_eq!(
            r["data"]["admission"]["portfolio"]["instrument_notional"],
            json!({})
        );
    }
    let after: i64 = f
        .journal()
        .query_row("SELECT COUNT(*) FROM account_safety_history", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(before, after);
    let mut args = proposal.clone();
    args["idempotency_key"] = json!("heat-trade1");
    let one = f
        .call_as(&p, "agent", "trade_prepare", args.clone(), true)
        .await
        .unwrap();
    assert!(one["error"].is_null(), "{one}");
    args["account_id"] = other.clone();
    args["idempotency_key"] = json!("heat-trade2");
    let two = f
        .call_as(&p, "agent2", "trade_prepare", args, true)
        .await
        .unwrap();
    assert!(two["error"].is_null(), "{two}");
    let (a,b)=tokio::join!(f.call_as(&p,"agent","trade_fund",json!({"account_id":account,"trade_id":one["data"]["id"],"idempotency_key":"heat-fund1"}),true),f.call_as(&p,"agent2","trade_fund",json!({"account_id":other,"trade_id":two["data"]["id"],"idempotency_key":"heat-fund2"}),true));
    let results = [a.unwrap(), b.unwrap()];
    assert_eq!(
        results.iter().filter(|r| r["error"].is_null()).count(),
        1,
        "{results:?}"
    );
    assert_eq!(
        results.iter().find(|r| !r["error"].is_null()).unwrap()["error"]["code"],
        "portfolio_heat_exceeded"
    );
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
    p.stop().await;
    let mut p = f.start(&gateway).await;
    let r = f
        .ok(&p, "account_readiness", json!({"account_id":other}))
        .await;
    assert_eq!(
        Decimal::from_str_exact(r["portfolio_risk"]["planned_loss"].as_str().unwrap()).unwrap(),
        Decimal::from(50)
    );
    let r = f
        .call_as(&p, "agent", "risk_check", proposal, true)
        .await
        .unwrap();
    assert!(r["error"].is_null(), "{r}");
    let q =
        Decimal::from_str_exact(r["data"]["max_admissible_quantity"].as_str().unwrap()).unwrap();
    assert!(q <= Decimal::from(2) && q > Decimal::new(199999, 5), "{r}");
    assert_eq!(r["data"]["blockers"], json!(["portfolio_heat_exceeded"]));
    p.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn admission_rechecks_quotes_margin_and_calendar_before_broker_submission() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(5000, 32);
    let mut p = f.start(&gateway).await;
    let (account, trade) = f.seed(&p, "long", "10", "quote-risk").await;
    f.ok(
        &p,
        "trade_fund",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"quote-fund"}),
    )
    .await;
    for (label, mut quote) in [
        ("delayed", support::risk::quote()),
        ("stale", support::risk::quote()),
        ("spread", support::risk::quote()),
        ("identity", support::risk::quote()),
    ] {
        match label {
            "delayed" => quote[0]["6509"] = json!("DpB"),
            "stale" => {
                quote[0]["_updated"] =
                    json!((chrono::Utc::now() - chrono::Duration::seconds(60)).timestamp_millis())
            }
            "spread" => quote[0]["86"] = json!("110"),
            _ => quote[0]["conid"] = json!("9999"),
        }
        *gateway.state.quote.lock().unwrap() = Some(quote);
        let r=f.call(&p,"trade_submit",json!({"account_id":account,"trade_id":trade,"idempotency_key":format!("reject-{label}")}),true).await;
        assert!(!r["error"].is_null(), "{label}: {r}");
        assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
    }
    *gateway.state.quote.lock().unwrap() = None;
    let mut summary = support::risk::summary();
    summary.as_object_mut().unwrap().remove("excessliquidity-s");
    *gateway.state.summary.lock().unwrap() = Some(summary);
    let r = f
        .call(
            &p,
            "trade_submit",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"reject-margin"}),
            true,
        )
        .await;
    assert_eq!(r["error"]["code"], "margin_evidence_missing");
    *gateway.state.summary.lock().unwrap() = None;
    let r = f
        .ok(&p, "account_readiness", json!({"account_id":account}))
        .await;
    assert_eq!(r["halted"], true);
    f.ok(&p,"account_resume",json!({"account_id":account,"reason":"Fresh complete margin evidence verified","idempotency_key":"margin-resume"})).await;
    let mut controls = support::risk::controls();
    controls["instruments"]["1234"]["blackouts"] = json!([{"starts_at":(chrono::Utc::now()-chrono::Duration::minutes(1)).to_rfc3339(),"ends_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339()}]);
    f.ok(&p,"account_safety_policy_set",json!({"account_id":account,"currency":"USD","max_gross_notional":"100000","max_order_notional":"10000","max_daily_loss":"500","max_drawdown":"1000","max_snapshot_age_seconds":120,"controls":controls,"idempotency_key":"blackout-policy"})).await;
    let r = f
        .call(
            &p,
            "trade_submit",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"reject-blackout"}),
            true,
        )
        .await;
    assert_eq!(r["error"]["code"], "entry_window_blocked");
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
    f.ok(
        &p,
        "trade_cancel",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"cancel-reservation"}),
    )
    .await;
    p.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn observed_agent_losses_and_fees_block_reentry_after_restart_without_policy_reset() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(5000, 32);
    let mut p = f.start(&gateway).await;
    let (account, template) = f.seed(&p, "long", "10", "agent-loss").await;
    let instrument = f
        .ok(
            &p,
            "trade_get",
            json!({"account_id":account,"trade_id":template}),
        )
        .await["trading_vehicle"]["id"]
        .clone();
    let mut budget = support::risk::budget();
    budget["max_daily_loss"] = json!("50");
    let mandate = json!({"account_id":account,"subject":"agent","instrument_ids":[instrument],"max_order_notional":"10000","max_total_notional":"100000","max_open_trades":100,"allow_short":true,"expires_at":(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339(),"risk_budget":budget,"idempotency_key":"agent-loss-mandate"});
    f.ok(&p, "agent_mandate_set", mandate.clone()).await;
    let draft = json!({"account_id":account,"instrument_id":instrument,"side":"long","quantity":"10","entry":"100","stop":"95","target":"110","idempotency_key":"agent-loss-draft"});
    let r = f
        .call_as(&p, "agent", "trade_prepare", draft.clone(), true)
        .await
        .unwrap();
    assert!(r["error"].is_null(), "{r}");
    let trade = r["data"]["id"].clone();
    for name in ["trade_fund", "trade_submit"] {
        let r=f.call_as(&p,"agent",name,json!({"account_id":account,"trade_id":trade,"idempotency_key":format!("agent-loss-{name}")}),true).await.unwrap();
        assert!(r["error"].is_null(), "{r}");
    }
    gateway.state.fills("10", "10", true);
    let r = f
        .call_as(
            &p,
            "agent",
            "trade_reconcile",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"agent-loss-close"}),
            true,
        )
        .await
        .unwrap();
    assert!(r["error"].is_null(), "{r}");
    f.ok(
        &p,
        "account_refresh",
        json!({"account_id":account,"idempotency_key":"agent-loss-observe"}),
    )
    .await;
    let r = f
        .call_as(
            &p,
            "agent",
            "account_readiness",
            json!({"account_id":account}),
            true,
        )
        .await
        .unwrap();
    assert!(
        r["data"]["blockers"]
            .as_array()
            .unwrap()
            .contains(&json!("agent_daily_loss_exceeded")),
        "{r}"
    );
    p.stop().await;
    let mut p = f.start(&gateway).await;
    let mut same = mandate;
    same["idempotency_key"] = json!("agent-loss-policy-again");
    f.ok(&p, "agent_mandate_set", same).await;
    let mut another = draft;
    another["quantity"] = json!("1");
    another["idempotency_key"] = json!("agent-loss-reentry");
    let r = f
        .call_as(&p, "agent", "trade_prepare", another, true)
        .await
        .unwrap();
    assert!(r["error"].is_null(), "{r}");
    let r=f.call_as(&p,"agent","trade_fund",json!({"account_id":account,"trade_id":r["data"]["id"],"idempotency_key":"agent-loss-denied"}),true).await.unwrap();
    assert_eq!(r["error"]["code"], "agent_daily_loss_exceeded");
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 1);
    p.stop().await;
}
