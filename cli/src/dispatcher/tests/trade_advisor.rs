#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::dispatcher::output::ReportOutputFormat;
use crate::dispatcher::trade_advisor::TradeAdvisorCheck;
use chrono::Days;
use chrono::Utc;
use serde_json::json;

#[test]
fn test_trade_events_add_persists_and_list_json_path_runs() {
    let mut dispatcher = test_dispatcher();
    let (_account, trade) = seed_new_trade(&mut dispatcher);
    let event_date = Utc::now()
        .date_naive()
        .checked_add_days(Days::new(14))
        .expect("event date");
    let trade_id = trade.id.to_string();
    let date = event_date.to_string();
    let add_matches = trade_event_add_matches(&[
        "--trade-id",
        &trade_id,
        "--type",
        "Earnings",
        "--date",
        &date,
        "--severity",
        "High",
        "--notes",
        "post-close earnings",
    ]);

    crate::dispatcher::trade_advisor::trade_events_add(&mut dispatcher.trust, &add_matches)
        .expect("event add should succeed");

    let events = dispatcher
        .trust
        .trade_events_for_trade(trade.id)
        .expect("events should be persisted");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, model::TradeEventType::Earnings);
    assert_eq!(events[0].severity, model::TradeEventSeverity::High);
    assert_eq!(events[0].notes.as_deref(), Some("post-close earnings"));

    let list_matches = trade_event_list_matches(&["--trade-id", &trade_id, "--format", "json"]);
    crate::dispatcher::trade_advisor::trade_events_list(
        &mut dispatcher.trust,
        &list_matches,
        ReportOutputFormat::Json,
    )
    .expect("event list JSON should succeed");
}

#[test]
fn test_trade_advisor_json_contract_and_verdict_logic_for_high_catalyst() {
    let mut dispatcher = test_dispatcher();
    let (_account, trade) = seed_new_trade(&mut dispatcher);
    let today = Utc::now().date_naive();
    let event_date = today.checked_add_days(Days::new(2)).expect("event date");
    let _event = dispatcher
        .trust
        .create_trade_event(
            trade.id,
            model::TradeEventType::Earnings,
            event_date,
            model::TradeEventSeverity::High,
            None,
        )
        .expect("seed event");
    let events = dispatcher
        .trust
        .trade_events_for_trade(trade.id)
        .expect("read events");

    let catalyst = crate::dispatcher::trade_advisor::trade_advisor_catalyst_check(
        &events, today, 30, false, None,
    );
    let correlation = TradeAdvisorCheck {
        status: "OK",
        payload: json!({
            "max_corr": "0.12",
            "corr_with": "MSFT",
            "cluster": null,
            "heat_adjusted_pct": "1.5",
            "status": "OK",
        }),
    };
    let regime = TradeAdvisorCheck {
        status: "OK",
        payload: json!({
            "vol": "Normal",
            "trend": "Trending",
            "breadth": null,
            "composite": "Normal",
            "breakouts_permitted": true,
            "status": "OK",
        }),
    };
    let verdict = crate::dispatcher::trade_advisor::trade_advisor_verdict(&[
        &catalyst,
        &correlation,
        &regime,
    ]);
    let payload = json!({
        "trade_id": trade.id.to_string(),
        "verdict": verdict,
        "catalyst": catalyst.payload,
        "correlation": correlation.payload,
        "regime": regime.payload,
    });

    assert_eq!(payload["verdict"], "NO_TRADE");
    assert_eq!(payload["catalyst"]["status"], "BLOCKED");
    assert_eq!(payload["catalyst"]["events"][0]["type"], "Earnings");
    assert_eq!(payload["catalyst"]["events"][0]["severity"], "High");
    assert_eq!(payload["correlation"]["status"], "OK");
    assert_eq!(payload["regime"]["composite"], "Normal");
}
