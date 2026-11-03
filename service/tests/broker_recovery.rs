#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    dead_code
)]
mod support;
use serde_json::{json, Value};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use support::{Fixture, Gateway, Process};

async fn wait_for_balance(f: &Fixture, p: &Process, account: &Value, expected: &str) {
    let started = Instant::now();
    loop {
        let balances = f.ok(p, "balances_get", json!({"account_id":account})).await;
        if balances
            .to_string()
            .contains(&format!("\"total_available\":\"{expected}\""))
        {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "expected {expected}, got {balances}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn background_recovers_closed_trade_activity_and_late_fees_after_restart_without_resuming() {
    let gateway = Gateway::start().await;
    let mut f = Fixture::new(45000, 32);
    let mut p = f.start(&gateway).await;
    let (account, trade) = f.seed(&p, "long", "10", "recovery").await;
    f.ok(
        &p,
        "trade_fund",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"recovery-fund"}),
    )
    .await;
    f.ok(
        &p,
        "trade_submit",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"recovery-submit"}),
    )
    .await;
    gateway.state.fills("10", "10", false);
    gateway.state.fail_activities.store(true, Ordering::SeqCst);
    let failed = f
        .call(
            &p,
            "trade_reconcile",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"incomplete"}),
            true,
        )
        .await;
    assert_eq!(failed["error"]["code"], "reconciliation_incomplete");
    let closed = f
        .ok(
            &p,
            "trade_get",
            json!({"account_id":account,"trade_id":trade}),
        )
        .await;
    assert_eq!(closed["status"], "ClosedTarget");
    wait_for_balance(&f, &p, &account, "10100").await;
    let resume = f.call(&p, "account_resume", json!({"account_id":account,"reason":"Current positions alone are insufficient","idempotency_key":"premature-resume"}), true).await;
    assert_eq!(resume["error"]["code"], "recovery_evidence_incomplete");
    p.stop().await;

    let timestamp = chrono::Utc::now().format("%Y%m%d-%H:%M:%S").to_string();
    let activities: Vec<Value> = gateway
        .state
        .orders
        .lock()
        .unwrap()
        .iter()
        .filter(|row| row["status"] == "Filled")
        .map(|row| {
            json!({"execution_id":format!("native-{}",row["orderId"]),"order_ref":row["order_ref"],
            "symbol":"TEST","side":row["side"],"size":"10","price":row["price"],
            "commission":"0","trade_time":timestamp})
        })
        .collect();
    assert_eq!(activities.len(), 2);
    *gateway.state.activities.lock().unwrap() = activities;
    gateway.state.fail_activities.store(false, Ordering::SeqCst);
    f.update_config("reconcile_interval_seconds", json!(1));
    let mut p = f.start(&gateway).await;
    let started = Instant::now();
    loop {
        let rows = f
            .ok(
                &p,
                "executions_list",
                json!({"account_id":account,"trade_id":trade}),
            )
            .await;
        if rows.to_string().contains("native-") {
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(60));
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    // The trade already reconciled successfully as closed with zero commissions.
    // New commissions still arrive through background work, without a manual sync.
    for row in gateway.state.activities.lock().unwrap().iter_mut() {
        row["commission"] = json!(if row["side"] == "BUY" { "-1" } else { "-0.5" });
    }
    wait_for_balance(&f, &p, &account, "10098.5").await;
    tokio::time::sleep(Duration::from_secs(12)).await;
    wait_for_balance(&f, &p, &account, "10098.5").await;
    let ready = f
        .ok(&p, "account_readiness", json!({"account_id":account}))
        .await;
    assert_eq!(
        ready["halted"], true,
        "successful recovery cannot resume new risk"
    );
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.state.modifications.load(Ordering::SeqCst), 0);
    f.ok(&p, "audit_verify", json!({})).await;
    p.stop().await;
}
