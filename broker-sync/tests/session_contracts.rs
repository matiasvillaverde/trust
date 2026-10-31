//! Assert observable session and wire behavior instead of derived traits.
use broker_sync::{BrokerCommand, BrokerEvent, TradeSessionHandle, TradeSessionTracker};
use serde_json::{json, Value};
use std::{sync::mpsc::RecvTimeoutError, time::Duration};
use uuid::Uuid;

fn send(handle: &TradeSessionHandle, value: Value) {
    handle.send(serde_json::from_value(value).unwrap()).unwrap();
}

fn receive(handle: &TradeSessionHandle) -> Value {
    serde_json::to_value(handle.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap()
}

#[test]
fn serialized_session_workflow_replaces_duplicates_removes_only_the_requested_trade_and_shuts_down()
{
    let handle = TradeSessionTracker::spawn();
    let accounts = [Uuid::new_v4(), Uuid::new_v4()];
    let trades = [Uuid::new_v4(), Uuid::new_v4()];
    for (account, trade) in [
        (accounts[0], trades[0]),
        (accounts[1], trades[1]),
        (accounts[1], trades[0]),
    ] {
        send(
            &handle,
            json!({"type":"StartTradeSession","account_id":account,"trade_id":trade}),
        );
        assert_eq!(
            receive(&handle),
            json!({"type":"TradeSessionStarted","account_id":account,"trade_id":trade})
        );
    }
    send(&handle, json!({"type":"ListTradeSessions"}));
    let snapshot = receive(&handle);
    assert_eq!(snapshot["type"], "TradeSessionSnapshot");
    let sessions = snapshot["sessions"].as_array().unwrap();
    assert_eq!(
        sessions.len(),
        2,
        "starting the same trade replaces its session"
    );
    for trade in trades {
        let session = sessions
            .iter()
            .find(|s| s["trade_id"] == trade.to_string())
            .unwrap();
        assert_eq!(session["account_id"], accounts[1].to_string());
        assert!(session["started_at_ms"].as_i64().unwrap() > 0);
        assert!(
            session["last_activity_at_ms"].as_i64().unwrap()
                >= session["started_at_ms"].as_i64().unwrap()
        );
    }
    for trade in [trades[0], trades[0], Uuid::new_v4()] {
        send(&handle, json!({"type":"StopTradeSession","trade_id":trade}));
        assert_eq!(
            receive(&handle),
            json!({"type":"TradeSessionStopped","trade_id":trade,"reason":"stopped"})
        );
    }
    send(&handle, json!({"type":"ListTradeSessions"}));
    let remaining = receive(&handle);
    assert_eq!(remaining["sessions"].as_array().unwrap().len(), 1);
    assert_eq!(remaining["sessions"][0]["trade_id"], trades[1].to_string());
    handle.send(BrokerCommand::Shutdown).unwrap();
    assert_eq!(
        handle.recv_timeout(Duration::from_secs(2)),
        Err(RecvTimeoutError::Disconnected)
    );
    assert!(handle.send(BrokerCommand::GetStatus).is_err());
}

#[test]
fn connected_events_remove_credentials_from_wire_and_debug_output() {
    let account = Uuid::new_v4();
    for (raw, expected) in [
        (
            "wss://api.example.com/stream?token=synthetic-secret&key=synthetic-secret",
            "wss://api.example.com/stream",
        ),
        (
            "wss://owner:synthetic-secret@api.example.com/stream#synthetic-secret",
            "wss://****@api.example.com/stream",
        ),
        ("not a valid url synthetic-secret", "wss://[redacted]"),
    ] {
        let event = BrokerEvent::connected(account, raw);
        assert_eq!(
            serde_json::to_value(&event).unwrap(),
            json!({"type":"Connected","account_id":account,"websocket_url":expected})
        );
        assert!(!format!("{event:?}").contains("synthetic-secret"));
    }
}
