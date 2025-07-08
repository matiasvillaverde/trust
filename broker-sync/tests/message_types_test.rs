//! Tests for actor message types (commands and events)
//! Following TDD - tests written first

use broker_sync::{BrokerCommand, BrokerEvent, OrderUpdate, ReconciliationStatus};
use rust_decimal_macros::dec;
use serde_json;
use std::time::Duration;
use uuid::Uuid;

#[test]
fn test_broker_command_variants_exist() {
    // Verify all command variants exist
    let _start_sync = BrokerCommand::StartSync {
        account_id: Uuid::new_v4(),
    };

    let _stop_sync = BrokerCommand::StopSync {
        account_id: Uuid::new_v4(),
    };

    let _manual_reconcile = BrokerCommand::ManualReconcile {
        account_id: Some(Uuid::new_v4()),
        force: false,
    };

    let _get_status = BrokerCommand::GetStatus;

    let _shutdown = BrokerCommand::Shutdown;
}

#[test]
fn test_broker_command_implements_debug() {
    let cmd = BrokerCommand::GetStatus;
    let debug_str = format!("{:?}", cmd);
    assert!(debug_str.contains("GetStatus"));
}

#[test]
fn test_broker_command_implements_clone() {
    let cmd = BrokerCommand::GetStatus;
    let cloned = cmd.clone();
    assert!(matches!(cloned, BrokerCommand::GetStatus));
}

#[test]
fn test_broker_event_variants_exist() {
    // Verify all event variants exist
    let _connected = BrokerEvent::Connected {
        account_id: Uuid::new_v4(),
        websocket_url: "wss://test.com".to_string(),
    };

    let _disconnected = BrokerEvent::Disconnected {
        account_id: Uuid::new_v4(),
        reason: "Connection lost".to_string(),
    };

    let order_update = OrderUpdate {
        trade_id: Uuid::new_v4(),
        order_type: "stop_loss".to_string(),
        old_status: "New".to_string(),
        new_status: "Filled".to_string(),
        filled_qty: Some(100),
        filled_price: Some(dec!(50.25)),
        message: None,
    };

    let _order_updated = BrokerEvent::OrderUpdated {
        account_id: Uuid::new_v4(),
        update: order_update,
    };

    let _reconciliation_complete = BrokerEvent::ReconciliationComplete {
        account_id: Uuid::new_v4(),
        status: ReconciliationStatus {
            orders_checked: 45,
            orders_updated: 3,
            errors: Vec::new(),
            duration: Duration::from_secs(2),
        },
    };

    let _error = BrokerEvent::Error {
        account_id: Some(Uuid::new_v4()),
        error: "API rate limit exceeded".to_string(),
        recoverable: true,
    };
}

#[test]
fn test_broker_event_implements_debug() {
    let event = BrokerEvent::GetStatus;
    let debug_str = format!("{:?}", event);
    assert!(debug_str.len() > 0);
}

#[test]
fn test_broker_event_implements_clone() {
    let event = BrokerEvent::Connected {
        account_id: Uuid::new_v4(),
        websocket_url: "wss://test.com".to_string(),
    };
    let cloned = event.clone();

    if let BrokerEvent::Connected { websocket_url, .. } = cloned {
        assert_eq!(websocket_url, "wss://test.com");
    } else {
        panic!("Expected Connected event");
    }
}

#[test]
fn test_broker_command_serialization() {
    let cmd = BrokerCommand::ManualReconcile {
        account_id: Some(Uuid::new_v4()),
        force: true,
    };

    // Should serialize to JSON
    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("ManualReconcile"));
    assert!(json.contains("force"));

    // Should deserialize back
    let deserialized: BrokerCommand = serde_json::from_str(&json).unwrap();

    if let BrokerCommand::ManualReconcile { force, .. } = deserialized {
        assert!(force);
    } else {
        panic!("Expected ManualReconcile command");
    }
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
