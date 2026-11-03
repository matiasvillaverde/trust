#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    dead_code
)]
#[path = "statements/access.rs"]
mod access;
#[path = "statements/integrity.rs"]
mod integrity;
mod support;

use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use support::{Fixture, Gateway, Process};

// Synthetic original-format exports: independent expected cash, not generated
// from Trust's returned balance or from its reconciliation calculator.
fn report(closing: &str, trades: &str, positions: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<FlexQueryResponse queryName="statement-private-marker" type="AF"><FlexStatements count="1">
<FlexStatement accountId="DU_TEST" fromDate="20260317" toDate="20260317" whenGenerated="20260318;060000">
<CashReport><CashReportCurrency currency="USD" fromDate="20260317" toDate="20260317" startingCash="10000" endingCash="{closing}" endingSettledCash="10000" commissions="{}" depositWithdrawals="0"/></CashReport>
<Trades>{trades}</Trades><OpenPositions>{positions}</OpenPositions><CashTransactions/>
</FlexStatement></FlexStatements></FlexQueryResponse>"#,
        if trades.is_empty() { "0" } else { "-3" }
    )
}
fn filled_report() -> String {
    report(
        "10097",
        r#"
<Trade levelOfDetail="EXECUTION" assetCategory="STK" multiplier="1" transactionType="ExchTrade" ibExecID="statement-entry" conid="1234" symbol="TEST" currency="USD" buySell="BUY" quantity="10" tradePrice="100" dateTime="20260317;100000" ibCommissionCurrency="USD" ibCommission="-1" taxes="0" proceeds="-1000"/>
<Trade levelOfDetail="EXECUTION" assetCategory="STK" multiplier="1" transactionType="ExchTrade" ibExecID="statement-exit" conid="1234" symbol="TEST" currency="USD" buySell="SELL" quantity="-10" tradePrice="110" dateTime="20260317;150000" ibCommissionCurrency="USD" ibCommission="-2" taxes="0" proceeds="1100"/>
"#,
        "",
    )
}
fn import(account: &Value, xml: &str, key: &str) -> Value {
    json!({"account_id":account,"xml":xml,"timezone":"UTC","complete_report":true,"idempotency_key":key})
}
fn financial(f: &Fixture) -> rusqlite::Connection {
    let cfg: Value =
        serde_json::from_slice(&std::fs::read(f.dir.path().join("config.json")).unwrap()).unwrap();
    let db = rusqlite::Connection::open(cfg["database"].as_str().unwrap()).unwrap();
    if let Some(path) = cfg["storage_encryption"]["financial_key_file"].as_str() {
        let key = trust_security::DatabaseKey::load(std::path::Path::new(path)).unwrap();
        db.execute_batch(&format!("PRAGMA key = {};", key.sql_literal().as_str()))
            .unwrap();
    }
    db
}
fn past_capital(f: &Fixture) {
    // Only clock placement is adjusted. Capital was deposited by the real service.
    financial(f)
        .execute(
            "UPDATE transactions SET created_at='2026-03-16 00:00:00' WHERE category='deposit'",
            [],
        )
        .unwrap();
}
fn snapshot(f: &Fixture) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    let db = financial(f);
    [
        "accounts",
        "trades",
        "orders",
        "transactions",
        "accounts_balances",
        "executions",
        "fee_activities",
    ]
    .iter()
    .map(|table| {
        let mut query = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY id"))
            .unwrap();
        let columns = query.column_count();
        query
            .query_map([], |row| (0..columns).map(|i| row.get(i)).collect())
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    })
    .collect()
}
async fn setup() -> (Gateway, Fixture, Process, Value, Value) {
    let gateway = Gateway::start().await;
    let mut f = Fixture::new(10000, 32);
    f.enable_encryption();
    let p = f.start(&gateway).await;
    let (account, trade) = f.seed(&p, "long", "10", "statement").await;
    past_capital(&f);
    (gateway, f, p, account, trade)
}
async fn close_with_fills(
    f: &Fixture,
    p: &Process,
    g: &Gateway,
    account: &Value,
    trade: &Value,
    with_fees: bool,
) {
    f.ok(
        p,
        "trade_fund",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"fill-fund"}),
    )
    .await;
    f.ok(
        p,
        "trade_submit",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":"fill-submit"}),
    )
    .await;
    g.state.fills("10", "10", false);
    let rows=g.state.orders.lock().unwrap().iter().filter_map(|order| {
        let (id,price,side,clock,commission)=match order["leg"].as_str().unwrap() {
            "trust-entry"=>("statement-entry","100","BOT","20260317-10:00:00","1"),
            "trust-target"=>("statement-exit","110","SLD","20260317-15:00:00","2"),
            _=>return None,
        };
        Some(json!({"execution_id":id,"order_ref":order["order_ref"],"symbol":"TEST","side":side,"size":"10","price":price,"trade_time":clock,"commission":if with_fees {commission}else{"0"}}))
    }).collect();
    *g.state.activities.lock().unwrap() = rows;
    let closed = f
        .ok(
            p,
            "trade_reconcile",
            json!({"account_id":account,"trade_id":trade,"idempotency_key":"fill-reconcile"}),
        )
        .await;
    assert_eq!(closed["status"], "ClosedTarget");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn statement_mcp_matches_actual_fills_fees_cash_and_replays_after_encrypted_restart() {
    let (g, f, mut p, a, t) = setup().await;
    close_with_fills(&f, &p, &g, &a, &t, true).await;
    let before = snapshot(&f);
    let xml = filled_report();
    let args = import(&a, &xml, "statement-match");
    let reply = f
        .call(&p, "broker_statement_import", args.clone(), true)
        .await;
    assert!(reply["error"].is_null(), "{reply}");
    let data = &reply["data"];
    assert_eq!(
        data["reconciliation"]["result"]["status"], "matched",
        "{data}"
    );
    assert_eq!(data["reconciliation"]["result"]["execution_count"], 2);
    assert_eq!(
        data["reconciliation"]["result"]["cash"][0]["trust_closing"],
        "10097"
    );
    assert_eq!(
        data["reconciliation"]["result"]["cash"][0]["broker_closing"],
        "10097"
    );
    assert_eq!(data["source_authenticated"], false);
    assert_eq!(data["reconciliation"]["result"]["permits_trading"], false);
    assert!(data["original_xml"].is_null());
    assert_eq!(snapshot(&f), before);
    let id = data["source"]["statement_id"].clone();
    let source = f
        .ok(
            &p,
            "broker_statement_get",
            json!({"account_id":a,"statement_id":id,"include_source":true}),
        )
        .await;
    assert_eq!(source["original_xml"], xml);
    let list = f
        .ok(
            &p,
            "broker_statements_list",
            json!({"account_id":a,"limit":1}),
        )
        .await;
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["source"]["statement_id"], id);
    assert!(!list.to_string().contains("statement-private-marker"));
    f.ok(&p, "audit_verify", json!({})).await;
    p.stop().await;
    assert!(!p.log_text().contains("statement-private-marker"));
    let bytes = std::fs::read(f.dir.path().join("data/control.db")).unwrap();
    assert!(!bytes.starts_with(b"SQLite format 3"));
    assert!(!bytes.windows(24).any(|b| b == b"statement-private-marker"));
    let mut p = f.start(&g).await;
    let replay = f.call(&p, "broker_statement_import", args, false).await;
    assert_eq!(replay["operation_id"], reply["operation_id"]);
    assert_eq!(replay["data"], reply["data"]);
    let checked = f
        .ok(
            &p,
            "broker_statement_reconcile",
            json!({"account_id":a,"statement_id":id,"idempotency_key":"statement-recheck"}),
        )
        .await;
    assert_eq!(checked["reconciliation"]["result"]["status"], "matched");
    assert_ne!(
        checked["reconciliation"]["attempt_id"],
        data["reconciliation"]["attempt_id"]
    );
    assert_eq!(checked["source"], data["source"]);
    assert_eq!(g.state.submissions.load(Ordering::SeqCst), 1);
    assert_eq!(snapshot(&f), before);
    p.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unresolved_statement_halts_all_profiles_until_explicit_resolution_and_verified_resume() {
    let (g, f, mut p, a, t) = setup().await;
    f.ok(
        &p,
        "account_refresh",
        json!({"account_id":a,"idempotency_key":"initial-refresh"}),
    )
    .await;
    let ready = f.ok(&p, "account_readiness", json!({"account_id":a})).await;
    assert_eq!(ready["ready_for_new_risk"], true, "{ready}");
    let before = snapshot(&f);
    let unknown_position = r#"<OpenPosition levelOfDetail="SUMMARY" reportDate="20260317" assetCategory="STK" multiplier="1" conid="9999" currency="USD" position="1"/>"#;
    let bad = f
        .ok(
            &p,
            "broker_statement_import",
            import(
                &a,
                &report("10000", "", unknown_position),
                "manual-position",
            ),
        )
        .await;
    assert_eq!(bad["reconciliation"]["result"]["status"], "discrepancies");
    assert_eq!(
        bad["reconciliation"]["result"]["issues"][0]["code"],
        "position_mismatch"
    );
    assert_eq!(snapshot(&f), before);
    let id = bad["source"]["statement_id"].clone();
    let immediately = f.ok(&p, "account_readiness", json!({"account_id":a})).await;
    assert_eq!(immediately["halted"], true);
    assert_eq!(immediately["unreconciled_statement_count"], 1);
    let duplicate = f
        .ok(
            &p,
            "broker_statement_import",
            import(
                &a,
                &report("10000", "", unknown_position),
                "same-original-new-attempt",
            ),
        )
        .await;
    assert_eq!(duplicate["source"], bad["source"]);
    assert_ne!(
        duplicate["reconciliation"]["attempt_id"],
        bad["reconciliation"]["attempt_id"]
    );
    let listed = f
        .ok(&p, "broker_statements_list", json!({"account_id":a}))
        .await;
    assert_eq!(listed["total"], 1);
    p.stop().await;
    let mut p = f.start(&g).await;
    let readiness = f.ok(&p, "account_readiness", json!({"account_id":a})).await;
    assert!(readiness["blockers"]
        .as_array()
        .unwrap()
        .contains(&json!("statement_reconciliation_unresolved")));
    let blocked = f
        .call_as(
            &p,
            "agent",
            "trade_fund",
            json!({"account_id":a,"trade_id":t,"idempotency_key":"blocked-by-statement"}),
            true,
        )
        .await
        .unwrap();
    assert_eq!(blocked["error"]["code"], "account_halted", "{blocked}");
    assert_eq!(snapshot(&f), before);
    assert_eq!(g.state.submissions.load(Ordering::SeqCst), 0);
    let resume = f
        .call(
            &p,
            "account_resume",
            json!({"account_id":a,"reason":"review","idempotency_key":"premature-resume"}),
            true,
        )
        .await;
    assert!(resume["error"].is_object(), "{resume}");
    let clean = f
        .ok(
            &p,
            "broker_statement_import",
            import(&a, &report("10000", "", ""), "clean-different-export"),
        )
        .await;
    assert_eq!(clean["reconciliation"]["result"]["status"], "matched");
    let ready = f.ok(&p, "account_readiness", json!({"account_id":a})).await;
    assert_eq!(ready["unreconciled_statement_count"], 1);
    assert_eq!(ready["halted"], true);
    let mut wrong_period = import(
        &a,
        &report("10000", "", "").replace("20260317", "20260316"),
        "different-period-correction",
    );
    wrong_period["supersedes_statement_id"] = id.clone();
    let rejected = f
        .call(&p, "broker_statement_import", wrong_period, true)
        .await;
    assert_eq!(rejected["error"]["code"], "statement_supersession_mismatch");
    let first = f
        .ok(
            &p,
            "broker_statements_list",
            json!({"account_id":a,"limit":1}),
        )
        .await;
    assert_eq!(first["total"], 2);
    assert_eq!(first["next_offset"], 1);
    let second = f
        .ok(
            &p,
            "broker_statements_list",
            json!({"account_id":a,"limit":1,"offset":1}),
        )
        .await;
    assert!(second["next_offset"].is_null());
    assert_eq!(second["items"][0]["source"]["statement_id"], id);
    let resolved=f.ok(&p,"broker_statement_reconcile",json!({"account_id":a,"statement_id":clean["source"]["statement_id"],"supersedes_statement_id":id,"idempotency_key":"resolve-reviewed-export"})).await;
    assert_eq!(resolved["reconciliation"]["supersedes_statement_id"], id);
    let ready = f.ok(&p, "account_readiness", json!({"account_id":a})).await;
    assert_eq!(ready["unreconciled_statement_count"], 0);
    assert_eq!(ready["halted"], true);
    let resumed=f.ok(&p,"account_resume",json!({"account_id":a,"reason":"corrected export and verified broker recovery","idempotency_key":"verified-resume"})).await;
    assert_eq!(resumed["ready_for_new_risk"], true, "{resumed}");
    f.ok(
        &p,
        "trade_fund",
        json!({"account_id":a,"trade_id":t,"idempotency_key":"allowed-after-recovery"}),
    )
    .await;
    assert_eq!(g.state.submissions.load(Ordering::SeqCst), 0);
    p.stop().await;
}
