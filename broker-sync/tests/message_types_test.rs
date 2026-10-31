//! Tests for actor message types (commands and events)
//! Following TDD - tests written first

use broker_sync::{BrokerCommand, BrokerEvent, OrderUpdate, ReconciliationStatus};
use rust_decimal_macros::dec;
use std::time::Duration;
use uuid::Uuid;

#[test]
fn unsupported_sync_commands_are_rejected_instead_of_silently_ignored() {
    for command in ["StartSync", "StopSync", "ManualReconcile"] {
        let value =
            serde_json::json!({"type": command, "account_id": Uuid::new_v4(), "force": true});
        assert!(serde_json::from_value::<BrokerCommand>(value).is_err());
    }
}

#[test]
fn test_broker_command_serialization() {
    let cmd = BrokerCommand::StartTradeSession {
        account_id: Uuid::new_v4(),
        trade_id: Uuid::new_v4(),
    };
    let json = serde_json::to_value(&cmd).unwrap();
    assert_eq!(json["type"], "StartTradeSession");
    assert_eq!(serde_json::from_value::<BrokerCommand>(json).unwrap(), cmd);
}

#[test]
fn test_broker_event_serialization() {
    let event = BrokerEvent::Connected {
        account_id: Uuid::new_v4(),
        websocket_url: "wss://alpaca.markets/stream".to_string(),
    };

    // Should serialize to JSON
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("Connected"));
    assert!(json.contains("wss://alpaca.markets/stream"));

    // Should deserialize back
    let deserialized: BrokerEvent = serde_json::from_str(&json).unwrap();

    if let BrokerEvent::Connected { websocket_url, .. } = deserialized {
        assert_eq!(websocket_url, "wss://alpaca.markets/stream");
    } else {
        panic!("Expected Connected event");
    }
}

#[test]
fn test_order_update_serialization() {
    let update = OrderUpdate {
        trade_id: Uuid::new_v4(),
        order_type: "limit".to_string(),
        old_status: "New".to_string(),
        new_status: "PartiallyFilled".to_string(),
        filled_qty: Some(50),
        filled_price: Some(dec!(100.50)),
        message: Some("Partial fill executed".to_string()),
    };

    let json = serde_json::to_string(&update).unwrap();
    let deserialized: OrderUpdate = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.order_type, "limit");
    assert_eq!(deserialized.filled_qty, Some(50));
    assert_eq!(deserialized.filled_price, Some(dec!(100.50)));
}

#[test]
fn test_reconciliation_status_serialization() {
    let status = ReconciliationStatus {
        orders_checked: 100,
        orders_updated: 5,
        errors: vec!["Order not found".to_string(), "API timeout".to_string()],
        duration: Duration::from_millis(1500),
    };

    let json = serde_json::to_string(&status).unwrap();
    let deserialized: ReconciliationStatus = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.orders_checked, 100);
    assert_eq!(deserialized.orders_updated, 5);
    assert_eq!(deserialized.errors.len(), 2);
}
