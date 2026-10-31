//! Product reports must agree with cash actually recorded through authenticated tools.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
#[allow(dead_code)]
mod support;

use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::{collections::BTreeMap, str::FromStr, sync::atomic::Ordering};
use support::{Fixture, Gateway, Process};

fn money(value: &Value) -> Decimal {
    Decimal::from_str(value.as_str().expect("money must remain a decimal string")).unwrap()
}

async fn report(f: &Fixture, p: &Process, account: &Value, kind: &str) -> Value {
    let mut result = f
        .ok(p, "report_get", json!({"account_id":account,"kind":kind}))
        .await;
    // Concentration groups are an unordered set; timestamps and timeline order
    // remain untouched so replay/restart checks still detect financial changes.
    if kind == "summary" {
        result["concentration"]
            .as_array_mut()
            .unwrap()
            .sort_by_key(|group| group["name"].as_str().unwrap().to_owned());
    }
    result
}

async fn prepare(
    f: &Fixture,
    p: &Process,
    account: &Value,
    instrument: &Value,
    side: &str,
    key: &str,
) -> Value {
    f.ok(p,"trade_prepare",json!({"account_id":account,"instrument_id":instrument,"side":side,"quantity":"1","entry":"100","stop":if side=="long"{"95"}else{"105"},"target":if side=="long"{"110"}else{"90"},"sector":"Technology","asset_class":"Stocks","idempotency_key":format!("product-{key}")})).await["id"].clone()
}

async fn record_close(
    f: &Fixture,
    p: &Process,
    account: &Value,
    trade: &Value,
    outcome: (&str, &str, &str, &str),
    key: &str,
) -> (Value, Value) {
    let (entry, exit, fee, tool) = outcome;
    f.ok(
        p,
        "trade_fund",
        json!({"account_id":account,"trade_id":trade,"idempotency_key":format!("{key}-fund")}),
    )
    .await;
    f.ok(p,"trade_record_fill",json!({"account_id":account,"trade_id":trade,"price":entry,"fee":fee,"idempotency_key":format!("{key}-fill")})).await;
    let args = json!({"account_id":account,"trade_id":trade,"price":exit,"fee":fee,"idempotency_key":format!("{key}-close")});
    let reply = f.call(p, tool, args.clone(), true).await;
    assert!(reply["error"].is_null(), "{reply}");
    (args, reply)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reports_reconcile_realized_fees_open_exposure_and_account_scope_after_restart() {
    for side in ["long", "short"] {
        let gateway = Gateway::start().await;
        let f = Fixture::new(10000, 32);
        let mut p = f.start(&gateway).await;
        let (account, winner) = f.seed(&p, side, "1", side).await;
        let instrument = f
            .ok(
                &p,
                "trade_get",
                json!({"account_id":account,"trade_id":winner}),
            )
            .await["trading_vehicle"]["id"]
            .clone();
        let (replay_args, original) = record_close(
            &f,
            &p,
            &account,
            &winner,
            (
                if side == "long" { "99" } else { "101" },
                if side == "long" { "110" } else { "90" },
                "0.50",
                "trade_record_target",
            ),
            "winner",
        )
        .await;
        let loser = prepare(&f, &p, &account, &instrument, side, "loser").await;
        record_close(
            &f,
            &p,
            &account,
            &loser,
            (
                "100",
                if side == "long" { "95" } else { "105" },
                "0.50",
                "trade_record_stop",
            ),
            "loser",
        )
        .await;
        let flat = prepare(&f, &p, &account, &instrument, side, "flat").await;
        record_close(
            &f,
            &p,
            &account,
            &flat,
            ("100", "100", "0", "trade_record_target"),
            "flat",
        )
        .await;

        // An unrelated account must never contribute cash or trade outcomes.
        let other = f.ok(&p,"account_create",json!({"name":"unrelated","description":"report isolation","environment":"paper","broker":"ibkr","broker_account_id":"DU_OTHER","idempotency_key":"other-account"})).await["id"].clone();
        f.ok(
            &p,
            "capital_deposit",
            json!({"account_id":other,"amount":"777.77","idempotency_key":"other-cash"}),
        )
        .await;
        let open = prepare(&f, &p, &account, &instrument, side, "open").await;
        f.ok(
            &p,
            "trade_fund",
            json!({"account_id":account,"trade_id":open,"idempotency_key":"open-fund"}),
        )
        .await;

        // Independent arithmetic: +11 - 1 fees, -5 - 1 fees, and 0 = +4.
        let performance = report(&f, &p, &account, "performance").await;
        assert_eq!(performance["total_trades"], 3);
        assert_eq!(performance["winning_trades"], 1);
        assert_eq!(performance["losing_trades"], 1);
        assert_eq!(money(&performance["average_win"]), Decimal::from(10));
        assert_eq!(money(&performance["average_loss"]), Decimal::from(-6));
        let metrics = report(&f, &p, &account, "metrics").await;
        assert_eq!(money(&metrics["net_profit"]), Decimal::from(4));
        let summary = report(&f, &p, &account, "summary").await;
        assert_eq!(
            money(&summary["balances"][0]["total_balance"]),
            Decimal::from(10004)
        );
        assert_eq!(summary["performance"], performance);
        let risk = report(&f, &p, &account, "risk").await;
        assert_eq!(risk["positions"].as_array().unwrap().len(), 1);
        assert_eq!(risk["positions"][0]["trade_id"], open);
        assert_eq!(summary["risk"], risk["positions"]);
        let attribution = report(&f, &p, &account, "attribution").await;
        assert_eq!(attribution["groups"].as_array().unwrap().len(), 1);
        assert_eq!(attribution["groups"][0]["trades"], 3);
        assert_eq!(
            money(&attribution["groups"][0]["realized_pnl"]),
            Decimal::from(4)
        );
        for (by, classified) in [("sector", "Technology"), ("asset_class", "Stocks")] {
            let grouped = f
                .ok(
                    &p,
                    "report_get",
                    json!({"account_id":account,"kind":"attribution","by":by}),
                )
                .await;
            let groups = grouped["groups"].as_array().unwrap();
            assert_eq!(groups.len(), 2);
            for (key, count, pnl) in [("unknown", 1, 10), (classified, 2, -6)] {
                let group = groups.iter().find(|group| group["key"] == key).unwrap();
                assert_eq!(group["trades"], count);
                assert_eq!(money(&group["realized_pnl"]), Decimal::from(pnl));
            }
        }
        let other_report = report(&f, &p, &other, "summary").await;
        assert_eq!(other_report["performance"]["total_trades"], 0);
        assert_eq!(
            money(&other_report["balances"][0]["total_available"]),
            Decimal::new(77777, 2)
        );

        let past = f.ok(&p,"report_get",json!({"account_id":account,"kind":"performance","from":"2000-01-01T00:00:00Z","to":"2000-01-02T00:00:00Z"})).await;
        assert_eq!(past["total_trades"], 0);
        // The legacy timeline subtracts funding reservations before fills:
        // 10,000 + 4 realized - 100 reserved long (105 short). An empty period
        // must carry that opening cash forward instead of manufacturing zero.
        let from = chrono::Utc::now();
        let to = from + chrono::Duration::seconds(1);
        let window = f
            .ok(
                &p,
                "report_get",
                json!({"account_id":account,"kind":"timeline","from":from,"to":to}),
            )
            .await;
        assert_eq!(window["points"].as_array().unwrap().len(), 1);
        assert_eq!(window["points"][0]["timestamp"], json!(from.naive_utc()));
        assert_eq!(
            money(&window["points"][0]["balance"]),
            Decimal::from(if side == "long" { 9904 } else { 9899 })
        );
        let mut before = BTreeMap::new();
        for kind in [
            "summary",
            "performance",
            "drawdown",
            "risk",
            "metrics",
            "attribution",
            "timeline",
            "tax",
        ] {
            before.insert(kind, report(&f, &p, &account, kind).await);
        }
        p.stop().await;
        let mut restarted = f.start(&gateway).await;
        let replay = f
            .call(&restarted, "trade_record_target", replay_args, true)
            .await;
        assert_eq!(replay["operation_id"], original["operation_id"]);
        assert_eq!(replay["data"], original["data"]);
        for (kind, expected) in before {
            assert_eq!(
                report(&f, &restarted, &account, kind).await,
                expected,
                "{kind}"
            );
        }
        f.ok(&restarted, "audit_verify", json!({})).await;
        assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
        restarted.stop().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reports_reject_missing_accounts_and_mixed_currencies_instead_of_plausible_totals() {
    let gateway = Gateway::start().await;
    let f = Fixture::new(10000, 32);
    let mut p = f.start(&gateway).await;
    let account = f.ok(&p,"account_create",json!({"name":"multi-currency","description":"report bounds","environment":"paper","broker":"ibkr","broker_account_id":"DU_TEST","idempotency_key":"product-account"})).await["id"].clone();
    let empty = report(&f, &p, &account, "summary").await;
    assert_eq!(empty["performance"]["total_trades"], 0);
    assert!(empty["risk"].as_array().unwrap().is_empty());
    for currency in ["usd", "eur"] {
        f.ok(&p,"capital_deposit",json!({"account_id":account,"amount":"100","currency":currency,"idempotency_key":format!("deposit-{currency}")})).await;
    }
    for kind in [
        "summary",
        "performance",
        "drawdown",
        "risk",
        "concentration",
        "metrics",
        "attribution",
        "timeline",
        "tax",
    ] {
        let reply = f
            .call(
                &p,
                "report_get",
                json!({"account_id":account,"kind":kind}),
                true,
            )
            .await;
        assert_eq!(reply["error"]["code"], "domain_rejected", "{kind}: {reply}");
        assert!(reply["data"].is_null());
        let missing = f
            .call(
                &p,
                "report_get",
                json!({"account_id":uuid::Uuid::new_v4(),"kind":kind}),
                true,
            )
            .await;
        assert_eq!(missing["error"]["code"], "not_found", "{kind}: {missing}");
        assert!(missing["data"].is_null());
    }
    let balances = f
        .ok(&p, "balances_get", json!({"account_id":account}))
        .await;
    assert_eq!(balances["items"].as_array().unwrap().len(), 2);
    assert!(balances["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|b| money(&b["total_balance"]) == Decimal::from(100)));
    assert_eq!(gateway.state.submissions.load(Ordering::SeqCst), 0);
    p.stop().await;
}
