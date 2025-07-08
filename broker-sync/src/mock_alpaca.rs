//! Mock Alpaca WebSocket server for testing real-time order synchronization

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info, warn};
use uuid::Uuid;

/// Mock Alpaca WebSocket server for testing
pub struct MockAlpacaServer {
    port: u16,
    clients: Arc<Mutex<HashMap<String, broadcast::Sender<AlpacaMessage>>>>,
    message_sender: broadcast::Sender<AlpacaMessage>,
}

/// WebSocket message types that Alpaca sends
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlpacaMessage {
    pub stream: String,
    pub data: AlpacaMessageData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlpacaMessageData {
    #[serde(rename = "new")]
    OrderNew { order: MockAlpacaOrder },
    #[serde(rename = "fill")]
    OrderFill {
        order: MockAlpacaOrder,
        position_qty: String,
        price: String,
        qty: String,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "partial_fill")]
    OrderPartialFill {
        order: MockAlpacaOrder,
        position_qty: String,
        price: String,
        qty: String,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "canceled")]
    OrderCanceled {
        order: MockAlpacaOrder,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "expired")]
    OrderExpired {
        order: MockAlpacaOrder,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "replaced")]
    OrderReplaced {
        order: MockAlpacaOrder,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "trade_update")]
    TradeUpdate {
        order: MockAlpacaOrder,
        event_type: String,
        position_qty: String,
        price: String,
        qty: String,
        timestamp: DateTime<Utc>,
    },
}

/// Mock Alpaca order structure matching the real API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MockAlpacaOrder {
    pub id: String,
    pub client_order_id: String,
    pub symbol: String,
    pub asset_class: String,
    pub notional: Option<String>,
    pub qty: Option<String>,
    pub filled_qty: String,
    pub filled_avg_price: Option<String>,
    pub order_class: String,
    pub order_type: String,
    pub type_: String,
    pub side: String,
    pub time_in_force: String,
    pub limit_price: Option<String>,
    pub stop_price: Option<String>,
    pub status: String,
    pub extended_hours: bool,
    pub legs: Vec<MockAlpacaOrder>,
    pub trail_percent: Option<String>,
    pub trail_price: Option<String>,
    pub hwm: Option<String>,
    pub subtag: Option<String>,
    pub source: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub filled_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub replaced_at: Option<DateTime<Utc>>,
    pub replaced_by: Option<String>,
    pub replaces: Option<String>,
}

impl MockAlpacaOrder {
    /// Create a new mock order
    pub fn new(symbol: &str, side: &str, qty: &str, order_type: &str) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            client_order_id: Uuid::new_v4().to_string(),
            symbol: symbol.to_string(),
            asset_class: "us_equity".to_string(),
            notional: None,
            qty: Some(qty.to_string()),
            filled_qty: "0".to_string(),
            filled_avg_price: None,
            order_class: "simple".to_string(),
            order_type: order_type.to_string(),
            type_: order_type.to_string(),
            side: side.to_string(),
            time_in_force: "day".to_string(),
            limit_price: None,
            stop_price: None,
            status: "new".to_string(),
            extended_hours: false,
            legs: vec![],
            trail_percent: None,
            trail_price: None,
            hwm: None,
            subtag: None,
            source: None,
            created_at: now,
            updated_at: now,
            submitted_at: Some(now),
            filled_at: None,
            expired_at: None,
            canceled_at: None,
            replaced_at: None,
            replaced_by: None,
            replaces: None,
        }
    }

    /// Create a bracket order with target and stop loss
    pub fn bracket(
        symbol: &str,
        side: &str,
        qty: &str,
        limit_price: &str,
        take_profit: &str,
        stop_loss: &str,
    ) -> Self {
        let now = Utc::now();
        let entry_id = Uuid::new_v4().to_string();
        let target_id = Uuid::new_v4().to_string();
        let stop_id = Uuid::new_v4().to_string();

        let target_side = if side == "buy" { "sell" } else { "buy" };
        let stop_side = target_side;

        Self {
            id: entry_id,
            client_order_id: Uuid::new_v4().to_string(),
            symbol: symbol.to_string(),
            asset_class: "us_equity".to_string(),
            notional: None,
            qty: Some(qty.to_string()),
            filled_qty: "0".to_string(),
            filled_avg_price: None,
            order_class: "bracket".to_string(),
            order_type: "limit".to_string(),
            type_: "limit".to_string(),
            side: side.to_string(),
            time_in_force: "day".to_string(),
            limit_price: Some(limit_price.to_string()),
            stop_price: None,
            status: "new".to_string(),
            extended_hours: false,
            legs: vec![
                // Target order
                MockAlpacaOrder {
                    id: target_id,
                    client_order_id: Uuid::new_v4().to_string(),
                    symbol: symbol.to_string(),
                    asset_class: "us_equity".to_string(),
                    notional: None,
                    qty: Some(qty.to_string()),
                    filled_qty: "0".to_string(),
                    filled_avg_price: None,
                    order_class: "simple".to_string(),
                    order_type: "limit".to_string(),
                    type_: "limit".to_string(),
                    side: target_side.to_string(),
                    time_in_force: "day".to_string(),
                    limit_price: Some(take_profit.to_string()),
                    stop_price: None,
                    status: "held".to_string(),
                    extended_hours: false,
                    legs: vec![],
                    trail_percent: None,
                    trail_price: None,
                    hwm: None,
                    subtag: None,
                    source: None,
                    created_at: now,
                    updated_at: now,
                    submitted_at: Some(now),
                    filled_at: None,
                    expired_at: None,
                    canceled_at: None,
                    replaced_at: None,
                    replaced_by: None,
                    replaces: None,
                },
                // Stop loss order
                MockAlpacaOrder {
                    id: stop_id,
                    client_order_id: Uuid::new_v4().to_string(),
                    symbol: symbol.to_string(),
                    asset_class: "us_equity".to_string(),
                    notional: None,
                    qty: Some(qty.to_string()),
                    filled_qty: "0".to_string(),
                    filled_avg_price: None,
                    order_class: "simple".to_string(),
                    order_type: "stop".to_string(),
                    type_: "stop".to_string(),
                    side: stop_side.to_string(),
                    time_in_force: "day".to_string(),
                    limit_price: None,
                    stop_price: Some(stop_loss.to_string()),
                    status: "held".to_string(),
                    extended_hours: false,
                    legs: vec![],
                    trail_percent: None,
                    trail_price: None,
                    hwm: None,
                    subtag: None,
                    source: None,
                    created_at: now,
                    updated_at: now,
                    submitted_at: Some(now),
                    filled_at: None,
                    expired_at: None,
                    canceled_at: None,
                    replaced_at: None,
                    replaced_by: None,
                    replaces: None,
                },
            ],
            trail_percent: None,
            trail_price: None,
            hwm: None,
            subtag: None,
            source: None,
            created_at: now,
            updated_at: now,
            submitted_at: Some(now),
            filled_at: None,
            expired_at: None,
            canceled_at: None,
            replaced_at: None,
            replaced_by: None,
            replaces: None,
        }
    }

    /// Mark order as filled
    pub fn fill(&mut self, price: &str) {
        let now = Utc::now();
        self.status = "filled".to_string();
        self.filled_qty = self.qty.clone().unwrap_or("0".to_string());
        self.filled_avg_price = Some(price.to_string());
        self.filled_at = Some(now);
        self.updated_at = now;
    }

    /// Mark order as partially filled
    pub fn partial_fill(&mut self, filled_qty: &str, price: &str) {
        let now = Utc::now();
        self.status = "partially_filled".to_string();
        self.filled_qty = filled_qty.to_string();
        self.filled_avg_price = Some(price.to_string());
        self.filled_at = Some(now);
        self.updated_at = now;
    }

    /// Mark order as canceled
    pub fn cancel(&mut self) {
        let now = Utc::now();
        self.status = "canceled".to_string();
        self.canceled_at = Some(now);
        self.updated_at = now;
    }
}

impl MockAlpacaServer {
    /// Create a new mock Alpaca server
    pub fn new(port: u16) -> Self {
        let (message_sender, _) = broadcast::channel(100);
        Self {
            port,
            clients: Arc::new(Mutex::new(HashMap::new())),
            message_sender,
        }
    }

    /// Start the mock server
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port)).await?;
        info!("Mock Alpaca server listening on port {}", self.port);

        loop {
            let (stream, addr) = listener.accept().await?;
            info!("New connection from {}", addr);

            let clients = self.clients.clone();
            let message_sender = self.message_sender.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, clients, message_sender).await {
                    error!("Error handling connection: {}", e);
                }
            });
        }
    }

    /// Send a message to all connected clients
    pub async fn broadcast_message(
        &self,
        message: AlpacaMessage,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let clients = self.clients.lock().await;
        for sender in clients.values() {
            if let Err(e) = sender.send(message.clone()) {
                warn!("Failed to send message to client: {}", e);
            }
        }
        Ok(())
    }

    /// Simulate order fill
    pub async fn simulate_order_fill(
        &self,
        order_id: &str,
        price: &str,
        qty: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut order = MockAlpacaOrder::new("AAPL", "buy", qty, "limit");
        order.id = order_id.to_string();
        order.fill(price);

        let message = AlpacaMessage {
            stream: "trade_updates".to_string(),
            data: AlpacaMessageData::OrderFill {
                order,
                position_qty: qty.to_string(),
                price: price.to_string(),
                qty: qty.to_string(),
                timestamp: Utc::now(),
            },
        };

        self.broadcast_message(message).await
    }

    /// Simulate order partial fill
    pub async fn simulate_order_partial_fill(
        &self,
        order_id: &str,
        filled_qty: &str,
        price: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut order = MockAlpacaOrder::new("AAPL", "buy", "100", "limit");
        order.id = order_id.to_string();
        order.partial_fill(filled_qty, price);

        let message = AlpacaMessage {
            stream: "trade_updates".to_string(),
            data: AlpacaMessageData::OrderPartialFill {
                order,
                position_qty: filled_qty.to_string(),
                price: price.to_string(),
                qty: filled_qty.to_string(),
                timestamp: Utc::now(),
            },
        };

        self.broadcast_message(message).await
    }

    /// Simulate order cancellation
    pub async fn simulate_order_cancel(
        &self,
        order_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut order = MockAlpacaOrder::new("AAPL", "buy", "100", "limit");
        order.id = order_id.to_string();
        order.cancel();

        let message = AlpacaMessage {
            stream: "trade_updates".to_string(),
            data: AlpacaMessageData::OrderCanceled {
                order,
                timestamp: Utc::now(),
            },
        };

        self.broadcast_message(message).await
    }

    /// Get server port
    pub fn port(&self) -> u16 {
        self.port
    }
}

async fn handle_connection(
    stream: TcpStream,
    clients: Arc<Mutex<HashMap<String, broadcast::Sender<AlpacaMessage>>>>,
    _message_sender: broadcast::Sender<AlpacaMessage>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ws_stream = accept_async(stream).await?;
    let client_id = Uuid::new_v4().to_string();

    info!("WebSocket connection established for client: {}", client_id);

    let (tx, _rx) = broadcast::channel(100);
    clients.lock().await.insert(client_id.clone(), tx.clone());

    let (ws_sender, mut ws_receiver) = ws_stream.split();
    let ws_sender = Arc::new(Mutex::new(ws_sender));
    let mut broadcast_receiver = tx.subscribe();

    // Handle incoming messages from client
    let client_id_clone = client_id.clone();
    let ws_sender_clone = ws_sender.clone();
    let client_handler = tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    info!("Received from client {}: {}", client_id_clone, text);

                    // Handle subscription messages
                    if text.contains("auth") {
                        // Send authentication success
                        let auth_response = r#"{"stream":"authorization","data":{"action":"authenticate","status":"authorized"}}"#;
                        let mut sender = ws_sender_clone.lock().await;
                        if let Err(e) = sender.send(Message::Text(auth_response.to_string())).await
                        {
                            error!("Failed to send auth response: {}", e);
                            break;
                        }
                    } else if text.contains("trade_updates") {
                        // Send subscription success
                        let sub_response = r#"{"stream":"trade_updates","data":{"action":"listen","status":"subscribed"}}"#;
                        let mut sender = ws_sender_clone.lock().await;
                        if let Err(e) = sender.send(Message::Text(sub_response.to_string())).await {
                            error!("Failed to send subscription response: {}", e);
                            break;
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("Client {} disconnected", client_id_clone);
                    break;
                }
                Err(e) => {
                    error!("WebSocket error for client {}: {}", client_id_clone, e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Handle outgoing messages to client
    let message_handler = tokio::spawn(async move {
        while let Ok(message) = broadcast_receiver.recv().await {
            let json = serde_json::to_string(&message).unwrap_or_else(|e| {
                error!("Failed to serialize message: {}", e);
                "{}".to_string()
            });

            let mut sender = ws_sender.lock().await;
            if let Err(e) = sender.send(Message::Text(json)).await {
                error!("Failed to send message to client: {}", e);
                break;
            }
        }
    });

    // Wait for either handler to complete
    tokio::select! {
        _ = client_handler => {},
        _ = message_handler => {},
    }

    // Clean up
    clients.lock().await.remove(&client_id);
    info!("Client {} cleaned up", client_id);

    Ok(())
}

// Split the stream import for different tokio-tungstenite versions
use futures_util::{SinkExt, StreamExt};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_alpaca_order_creation() {
        let order = MockAlpacaOrder::new("AAPL", "buy", "100", "limit");
        assert_eq!(order.symbol, "AAPL");
        assert_eq!(order.side, "buy");
        assert_eq!(order.qty, Some("100".to_string()));
        assert_eq!(order.status, "new");
    }

    #[test]
    fn test_mock_alpaca_order_fill() {
        let mut order = MockAlpacaOrder::new("AAPL", "buy", "100", "limit");
        order.fill("150.00");

        assert_eq!(order.status, "filled");
        assert_eq!(order.filled_qty, "100");
        assert_eq!(order.filled_avg_price, Some("150.00".to_string()));
        assert!(order.filled_at.is_some());
    }

    #[test]
    fn test_mock_alpaca_order_partial_fill() {
        let mut order = MockAlpacaOrder::new("AAPL", "buy", "100", "limit");
        order.partial_fill("50", "150.00");

        assert_eq!(order.status, "partially_filled");
        assert_eq!(order.filled_qty, "50");
        assert_eq!(order.filled_avg_price, Some("150.00".to_string()));
        assert!(order.filled_at.is_some());
    }

    #[test]
    fn test_mock_alpaca_order_cancel() {
        let mut order = MockAlpacaOrder::new("AAPL", "buy", "100", "limit");
        order.cancel();

        assert_eq!(order.status, "canceled");
        assert!(order.canceled_at.is_some());
    }

    #[test]
    fn test_bracket_order_creation() {
        let order = MockAlpacaOrder::bracket("AAPL", "buy", "100", "150.00", "160.00", "140.00");

        assert_eq!(order.symbol, "AAPL");
        assert_eq!(order.order_class, "bracket");
        assert_eq!(order.legs.len(), 2);

        // Check target order
        let target = &order.legs[0];
        assert_eq!(target.side, "sell");
        assert_eq!(target.limit_price, Some("160.00".to_string()));
        assert_eq!(target.status, "held");

        // Check stop loss order
        let stop = &order.legs[1];
        assert_eq!(stop.side, "sell");
        assert_eq!(stop.stop_price, Some("140.00".to_string()));
        assert_eq!(stop.status, "held");
    }

    #[tokio::test]
    async fn test_server_creation() {
        let server = MockAlpacaServer::new(0);
        assert_eq!(server.port(), 0);
    }

    #[tokio::test]
    async fn test_broadcast_message() {
        let server = MockAlpacaServer::new(0);
        let order = MockAlpacaOrder::new("AAPL", "buy", "100", "limit");
        let message = AlpacaMessage {
            stream: "trade_updates".to_string(),
            data: AlpacaMessageData::OrderNew { order },
        };

        // Should not fail even with no clients
        let result = server.broadcast_message(message).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_simulate_order_fill() {
        let server = MockAlpacaServer::new(0);
        let result = server
            .simulate_order_fill("test-order-id", "150.00", "100")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_simulate_order_partial_fill() {
        let server = MockAlpacaServer::new(0);
        let result = server
            .simulate_order_partial_fill("test-order-id", "50", "150.00")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_simulate_order_cancel() {
        let server = MockAlpacaServer::new(0);
        let result = server.simulate_order_cancel("test-order-id").await;
        assert!(result.is_ok());
    }
}
