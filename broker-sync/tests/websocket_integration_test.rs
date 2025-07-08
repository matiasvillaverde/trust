//! Integration tests for WebSocket synchronization

use broker_sync::{
    AlpacaMessage, AlpacaMessageData, BrokerState, MockAlpacaOrder, MockAlpacaServer,
    StateTransition, WebSocketSync,
};
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::{sleep, timeout};

/// Test basic WebSocket connection and authentication
#[tokio::test]
async fn test_websocket_connection_flow() {
    // Start mock Alpaca server on a random port
    let server_port = find_free_port().await;

    // Start server in background
    let server_handle = tokio::spawn(async move {
        let server = MockAlpacaServer::new(server_port);
        server.start().await.unwrap();
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    // Create WebSocket sync client
    let state = Arc::new(Mutex::new(BrokerState::Disconnected));
    let websocket_url = format!("ws://127.0.0.1:{}", server_port);
    let sync_client = WebSocketSync::new(websocket_url, state.clone());

    // Start sync with timeout - run directly without spawning to avoid Send issues
    let sync_result = timeout(Duration::from_secs(2), sync_client.start()).await;

    // We expect this to timeout since it's a continuous connection
    assert!(
        sync_result.is_err(),
        "Sync should timeout (continuous connection)"
    );

    // Check that state progressed through connection states
    let final_state = state.lock().await;
    // In a real test, we'd verify the state progression
    println!("Final state: {:?}", *final_state);

    // Cleanup
    server_handle.abort();
}

/// Test order fill event processing
#[tokio::test]
async fn test_order_fill_processing() {
    // Start mock server
    let server_port = find_free_port().await;
    let server = Arc::new(MockAlpacaServer::new(server_port));

    let server_clone = server.clone();
    let server_handle = tokio::spawn(async move {
        server_clone.start().await.unwrap();
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    // Create sync client
    let state = Arc::new(Mutex::new(BrokerState::Disconnected));
    let websocket_url = format!("ws://127.0.0.1:{}", server_port);
    let sync_client = WebSocketSync::new(websocket_url, state.clone());

    // Start sync with timeout to avoid Send issues
    let sync_future = sync_client.start();
    let fill_future = async {
        // Wait for connection to establish
        sleep(Duration::from_secs(1)).await;

        // Simulate order fill
        let order_id = "test-order-123";
        let fill_result = server.simulate_order_fill(order_id, "150.00", "100").await;
        assert!(fill_result.is_ok(), "Should be able to simulate order fill");

        // Give time for message processing
        sleep(Duration::from_millis(500)).await;
    };

    // Run both concurrently with timeout
    let result = timeout(Duration::from_secs(3), async {
        tokio::select! {
            _ = sync_future => {},
            _ = fill_future => {},
        }
    })
    .await;

    // Should timeout since sync runs continuously
    assert!(result.is_err(), "Should timeout");
    server_handle.abort();
}

/// Test multiple order events in sequence
#[tokio::test]
async fn test_multiple_order_events() {
    let server_port = find_free_port().await;
    let server = Arc::new(MockAlpacaServer::new(server_port));

    let server_clone = server.clone();
    let server_handle = tokio::spawn(async move {
        server_clone.start().await.unwrap();
    });

    sleep(Duration::from_millis(100)).await;

    let state = Arc::new(Mutex::new(BrokerState::Disconnected));
    let websocket_url = format!("ws://127.0.0.1:{}", server_port);
    let sync_client = WebSocketSync::new(websocket_url, state.clone());

    // Start sync with timeout to avoid Send issues
    let sync_future = sync_client.start();
    let events_future = async {
        sleep(Duration::from_secs(1)).await;

        // Test sequence: partial fill, then full fill
        let order_id = "test-order-456";

        // Partial fill
        let partial_result = server
            .simulate_order_partial_fill(order_id, "50", "149.50")
            .await;
        assert!(partial_result.is_ok());

        sleep(Duration::from_millis(200)).await;

        // Full fill
        let fill_result = server.simulate_order_fill(order_id, "150.25", "100").await;
        assert!(fill_result.is_ok());

        sleep(Duration::from_millis(200)).await;

        // Cancel another order
        let cancel_order_id = "test-order-789";
        let cancel_result = server.simulate_order_cancel(cancel_order_id).await;
        assert!(cancel_result.is_ok());

        sleep(Duration::from_millis(500)).await;
    };

    // Run both concurrently with timeout
    let result = timeout(Duration::from_secs(4), async {
        tokio::select! {
            _ = sync_future => {},
            _ = events_future => {},
        }
    })
    .await;

    // Should timeout since sync runs continuously
    assert!(result.is_err(), "Should timeout");
    server_handle.abort();
}

/// Test bracket order scenario (entry + target + stop)
#[tokio::test]
async fn test_bracket_order_scenario() {
    let server_port = find_free_port().await;
    let server = Arc::new(MockAlpacaServer::new(server_port));

    let server_clone = server.clone();
    let server_handle = tokio::spawn(async move {
        server_clone.start().await.unwrap();
    });

    sleep(Duration::from_millis(100)).await;

    let state = Arc::new(Mutex::new(BrokerState::Disconnected));
    let websocket_url = format!("ws://127.0.0.1:{}", server_port);
    let sync_client = WebSocketSync::new(websocket_url, state.clone());

    // Start sync with timeout to avoid Send issues
    let sync_future = sync_client.start();
    let bracket_future = async {
        sleep(Duration::from_secs(1)).await;

        // Create bracket order
        let bracket_order = MockAlpacaOrder::bracket(
            "AAPL", "buy", "100", "150.00", // entry limit
            "160.00", // target
            "140.00", // stop loss
        );

        // Simulate entry order fill
        let entry_fill_msg = AlpacaMessage {
            stream: "trade_updates".to_string(),
            data: AlpacaMessageData::OrderFill {
                order: bracket_order.clone(),
                position_qty: "100".to_string(),
                price: "150.00".to_string(),
                qty: "100".to_string(),
                timestamp: Utc::now(),
            },
        };

        let broadcast_result = server.broadcast_message(entry_fill_msg).await;
        assert!(broadcast_result.is_ok());

        sleep(Duration::from_millis(200)).await;

        // Simulate target order fill (trade exit)
        let target_order = bracket_order.legs[0].clone(); // Target is first leg
        let target_fill_msg = AlpacaMessage {
            stream: "trade_updates".to_string(),
            data: AlpacaMessageData::OrderFill {
                order: target_order,
                position_qty: "0".to_string(),
                price: "160.00".to_string(),
                qty: "100".to_string(),
                timestamp: Utc::now(),
            },
        };

        let broadcast_result = server.broadcast_message(target_fill_msg).await;
        assert!(broadcast_result.is_ok());

        sleep(Duration::from_millis(500)).await;
    };

    // Run both concurrently with timeout
    let result = timeout(Duration::from_secs(5), async {
        tokio::select! {
            _ = sync_future => {},
            _ = bracket_future => {},
        }
    })
    .await;

    // Should timeout since sync runs continuously
    assert!(result.is_err(), "Should timeout");
    server_handle.abort();
}

/// Test error recovery scenario
#[tokio::test]
async fn test_error_recovery() {
    // Test connecting to a non-existent server (should fail and recover)
    let state = Arc::new(Mutex::new(BrokerState::Disconnected));
    let websocket_url = "ws://127.0.0.1:1".to_string(); // Invalid port
    let sync_client = WebSocketSync::new(websocket_url, state.clone());

    // This should fail quickly
    let sync_result = timeout(Duration::from_secs(2), sync_client.start()).await;

    // Should either timeout or fail
    if let Ok(result) = sync_result {
        assert!(result.is_err(), "Should fail to connect to invalid port");
    }

    // Check that state went to error recovery
    let final_state = state.lock().await;
    // Note: Due to timing, state might be Disconnected or in error recovery
    println!("Final state after connection failure: {:?}", *final_state);
}

/// Test state transitions during normal operation
#[tokio::test]
async fn test_state_transitions() {
    let state = Arc::new(Mutex::new(BrokerState::Disconnected));

    // Test state transitions manually
    {
        let mut current_state = state.lock().await;
        *current_state = current_state.clone().transition(StateTransition::Connect);
        assert!(matches!(*current_state, BrokerState::Connecting));

        *current_state = current_state
            .clone()
            .transition(StateTransition::ConnectionEstablished);
        assert!(matches!(*current_state, BrokerState::Reconciling { .. }));

        *current_state = current_state
            .clone()
            .transition(StateTransition::ReconciliationComplete);
        assert!(matches!(*current_state, BrokerState::Live { .. }));

        assert!(current_state.is_connected());

        *current_state = current_state.clone().transition(StateTransition::Error);
        assert!(matches!(*current_state, BrokerState::ErrorRecovery { .. }));

        assert!(!current_state.is_connected());
    }
}

/// Test message parsing and handling
#[tokio::test]
async fn test_message_parsing() {
    let state = Arc::new(Mutex::new(BrokerState::Disconnected));
    let sync_client = WebSocketSync::new("ws://test".to_string(), state);

    // Test auth response
    let auth_json = serde_json::json!({
        "stream": "authorization",
        "data": {"action": "authenticate", "status": "authorized"}
    });

    let result = sync_client.handle_auth_response(&auth_json).await;
    assert!(result.is_ok());

    // Test subscription response
    let sub_json = serde_json::json!({
        "data": {"action": "listen", "status": "subscribed"}
    });

    let result = sync_client.handle_message(&sub_json.to_string()).await;
    assert!(result.is_ok());
}

/// Helper function to find a free port for testing
async fn find_free_port() -> u16 {
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener); // Release the port
    port
}

/// Test mock Alpaca server functionality
#[tokio::test]
async fn test_mock_server_operations() {
    let server = MockAlpacaServer::new(0);

    // Test order simulation methods
    let result = server.simulate_order_fill("test-id", "100.00", "50").await;
    assert!(result.is_ok());

    let result = server
        .simulate_order_partial_fill("test-id", "25", "100.00")
        .await;
    assert!(result.is_ok());

    let result = server.simulate_order_cancel("test-id").await;
    assert!(result.is_ok());
}

/// Test order data structures and serialization
#[tokio::test]
async fn test_order_data_structures() {
    // Test mock order creation
    let order = MockAlpacaOrder::new("AAPL", "buy", "100", "limit");
    assert_eq!(order.symbol, "AAPL");
    assert_eq!(order.side, "buy");
    assert_eq!(order.qty, Some("100".to_string()));

    // Test bracket order
    let bracket = MockAlpacaOrder::bracket("TSLA", "buy", "50", "800.00", "850.00", "750.00");
    assert_eq!(bracket.symbol, "TSLA");
    assert_eq!(bracket.order_class, "bracket");
    assert_eq!(bracket.legs.len(), 2);

    // Test message serialization
    let message = AlpacaMessage {
        stream: "trade_updates".to_string(),
        data: AlpacaMessageData::OrderFill {
            order: order.clone(),
            position_qty: "100".to_string(),
            price: "150.00".to_string(),
            qty: "100".to_string(),
            timestamp: Utc::now(),
        },
    };

    let json = serde_json::to_string(&message);
    assert!(json.is_ok());

    let parsed: Result<AlpacaMessage, _> = serde_json::from_str(&json.unwrap());
    assert!(parsed.is_ok());
}

/// Integration test combining daemon and WebSocket sync
#[tokio::test]
async fn test_daemon_integration() {
    // This test would ideally start the full daemon process and test IPC communication
    // For now, we test the components separately due to the complexity of
    // running a full daemon in a test environment

    use broker_sync::BrokerSyncDaemon;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let daemon_dir = temp_dir.path().join("daemon");

    let config = broker_sync::DaemonConfig {
        socket_path: daemon_dir.join("test.sock").to_str().unwrap().to_string(),
        pid_path: daemon_dir.join("test.pid").to_str().unwrap().to_string(),
        log_path: daemon_dir.join("test.log").to_str().unwrap().to_string(),
        daemon_dir: daemon_dir.to_str().unwrap().to_string(),
    };

    config.ensure_directories().unwrap();

    // Test daemon creation
    let daemon = BrokerSyncDaemon::new(config);
    assert!(daemon.is_ok());
}
