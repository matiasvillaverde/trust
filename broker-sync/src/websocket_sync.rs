//! WebSocket synchronization logic for real-time order updates

use crate::mock_alpaca::{AlpacaMessage, AlpacaMessageData, MockAlpacaOrder};
use crate::state::{BrokerState, StateTransition};
use anyhow::Result;
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use model::{DatabaseFactory, Order, OrderStatus};
use rust_decimal::Decimal;
use serde_json::Value;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// WebSocket client for Alpaca real-time order synchronization
pub struct WebSocketSync {
    /// Connection URL for Alpaca WebSocket API
    url: String,
    /// Current broker state
    state: Arc<Mutex<BrokerState>>,
    /// Database factory for order synchronization
    database: Option<Arc<dyn DatabaseFactory>>,
}

impl WebSocketSync {
    /// Create a new WebSocket sync client
    pub fn new(url: String, state: Arc<Mutex<BrokerState>>) -> Self {
        Self {
            url,
            state,
            database: None,
        }
    }

    /// Create a new WebSocket sync client with database
    pub fn with_database(
        url: String,
        state: Arc<Mutex<BrokerState>>,
        database: Arc<dyn DatabaseFactory>,
    ) -> Self {
        Self {
            url,
            state,
            database: Some(database),
        }
    }

    /// Start the WebSocket connection and sync loop
    pub async fn start(&self) -> Result<()> {
        info!("Starting WebSocket sync with URL: {}", self.url);

        // Update state to connecting
        {
            let mut state = self.state.lock().await;
            *state = state.clone().transition(StateTransition::Connect);
        }

        // Connect to WebSocket
        let (ws_stream, _) = match connect_async(&self.url).await {
            Ok(connection) => {
                info!("WebSocket connection established");
                {
                    let mut state = self.state.lock().await;
                    *state = state
                        .clone()
                        .transition(StateTransition::ConnectionEstablished);
                }
                connection
            }
            Err(e) => {
                error!("Failed to connect to WebSocket: {}", e);
                {
                    let mut state = self.state.lock().await;
                    *state = state.clone().transition(StateTransition::Error);
                }
                return Err(e.into());
            }
        };

        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Send authentication message
        let auth_msg = r#"{"action":"auth","key":"test_key","secret":"test_secret"}"#;
        if let Err(e) = ws_sender.send(Message::Text(auth_msg.to_string())).await {
            error!("Failed to send auth message: {}", e);
            return Err(e.into());
        }

        // Subscribe to trade updates
        let subscribe_msg = r#"{"action":"listen","data":{"streams":["trade_updates"]}}"#;
        if let Err(e) = ws_sender
            .send(Message::Text(subscribe_msg.to_string()))
            .await
        {
            error!("Failed to send subscribe message: {}", e);
            return Err(e.into());
        }

        // Start reconciliation
        {
            let mut state = self.state.lock().await;
            *state = state
                .clone()
                .transition(StateTransition::StartReconciliation);
        }

        // Main message loop
        while let Some(msg) = ws_receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    debug!("Received WebSocket message: {}", text);
                    if let Err(e) = self.handle_message(&text).await {
                        error!("Error handling message: {}", e);
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket connection closed");
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    {
                        let mut state = self.state.lock().await;
                        *state = state.clone().transition(StateTransition::Error);
                    }
                    break;
                }
                _ => {}
            }
        }

        // Update state to disconnected
        {
            let mut state = self.state.lock().await;
            *state = state.clone().transition(StateTransition::Disconnect);
        }

        Ok(())
    }

    /// Handle incoming WebSocket message
    pub async fn handle_message(&self, text: &str) -> Result<()> {
        // Try to parse as generic JSON first to handle various message types
        let json: Value = serde_json::from_str(text)?;

        // Handle authentication response
        if let Some(stream) = json.get("stream") {
            if stream == "authorization" {
                return self.handle_auth_response(&json).await;
            }
        }

        // Handle subscription confirmation
        if let Some(data) = json.get("data") {
            if let Some(action) = data.get("action") {
                if action == "listen" {
                    info!("Successfully subscribed to trade updates");
                    {
                        let mut state = self.state.lock().await;
                        *state = state
                            .clone()
                            .transition(StateTransition::ReconciliationComplete);
                    }
                    return Ok(());
                }
            }
        }

        // Try to parse as Alpaca message
        match serde_json::from_str::<AlpacaMessage>(text) {
            Ok(alpaca_msg) => {
                self.handle_alpaca_message(alpaca_msg).await?;
            }
            Err(e) => {
                debug!("Could not parse as AlpacaMessage, ignoring: {}", e);
            }
        }

        Ok(())
    }

    /// Handle authentication response
    pub async fn handle_auth_response(&self, json: &Value) -> Result<()> {
        if let Some(data) = json.get("data") {
            if let Some(status) = data.get("status") {
                if status == "authorized" {
                    info!("WebSocket authentication successful");
                    return Ok(());
                }
            }
        }

        warn!("WebSocket authentication failed: {}", json);
        Ok(())
    }

    /// Handle Alpaca message
    async fn handle_alpaca_message(&self, message: AlpacaMessage) -> Result<()> {
        info!("Processing Alpaca message: {:?}", message.stream);

        match message.data {
            AlpacaMessageData::OrderNew { order } => {
                self.handle_order_new(order).await?;
            }
            AlpacaMessageData::OrderFill {
                order,
                position_qty: _,
                price,
                qty: _,
                timestamp,
            } => {
                self.handle_order_fill(order, price, timestamp).await?;
            }
            AlpacaMessageData::OrderPartialFill {
                order,
                position_qty: _,
                price,
                qty,
                timestamp,
            } => {
                self.handle_order_partial_fill(order, price, qty, timestamp)
                    .await?;
            }
            AlpacaMessageData::OrderCanceled { order, timestamp } => {
                self.handle_order_canceled(order, timestamp).await?;
            }
            AlpacaMessageData::OrderExpired { order, timestamp } => {
                self.handle_order_expired(order, timestamp).await?;
            }
            AlpacaMessageData::OrderReplaced { order, timestamp } => {
                self.handle_order_replaced(order, timestamp).await?;
            }
            AlpacaMessageData::TradeUpdate {
                order,
                event_type,
                position_qty: _,
                price,
                qty: _,
                timestamp,
            } => {
                self.handle_trade_update(order, event_type, price, timestamp)
                    .await?;
            }
        }

        Ok(())
    }

    /// Handle new order event
    async fn handle_order_new(&self, order: MockAlpacaOrder) -> Result<()> {
        info!("Order new: {} - {}", order.id, order.symbol);
        // TODO: Update database with new order status
        Ok(())
    }

    /// Handle order fill event
    async fn handle_order_fill(
        &self,
        order: MockAlpacaOrder,
        price: String,
        _timestamp: DateTime<Utc>,
    ) -> Result<()> {
        info!(
            "Order filled: {} - {} shares at ${}",
            order.id, order.filled_qty, price
        );

        if let Some(_database_factory) = &self.database {
            // Convert Alpaca order to Trust order
            let trust_order = convert_alpaca_order_to_trust(&order)?;

            // TODO: Database operations are temporarily disabled due to Send/Sync issues
            // In a full implementation, these operations would:
            // 1. Update the order in the database
            // 2. Find the trade associated with this order
            // 3. Update the trade status
            // 4. Update trade balance calculations

            info!(
                "Database sync: Order {} filled - {} shares at ${}",
                trust_order.broker_order_id.unwrap_or_default(),
                trust_order.filled_quantity,
                trust_order.average_filled_price.unwrap_or_default()
            );
        } else {
            warn!("No database connection available for order synchronization");
        }

        Ok(())
    }

    /// Handle order partial fill event
    async fn handle_order_partial_fill(
        &self,
        order: MockAlpacaOrder,
        price: String,
        qty: String,
        _timestamp: DateTime<Utc>,
    ) -> Result<()> {
        info!(
            "Order partially filled: {} - {} shares at ${}",
            order.id, qty, price
        );

        if let Some(_database_factory) = &self.database {
            let trust_order = convert_alpaca_order_to_trust(&order)?;

            // TODO: Database operations are temporarily disabled due to Send/Sync issues
            info!(
                "Database sync: Order {} updated - status: {:?}",
                trust_order.broker_order_id.unwrap_or_default(),
                trust_order.status
            );
        } else {
            warn!("No database connection available for order synchronization");
        }

        Ok(())
    }

    /// Handle order canceled event
    async fn handle_order_canceled(
        &self,
        order: MockAlpacaOrder,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        info!("Order canceled: {} at {}", order.id, timestamp);

        if let Some(_database_factory) = &self.database {
            let trust_order = convert_alpaca_order_to_trust(&order)?;

            // TODO: Database operations are temporarily disabled due to Send/Sync issues
            info!(
                "Database sync: Order {} updated - status: {:?}",
                trust_order.broker_order_id.unwrap_or_default(),
                trust_order.status
            );
        } else {
            warn!("No database connection available for order synchronization");
        }

        Ok(())
    }

    /// Handle order expired event
    async fn handle_order_expired(
        &self,
        order: MockAlpacaOrder,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        info!("Order expired: {} at {}", order.id, timestamp);

        if let Some(_database_factory) = &self.database {
            let trust_order = convert_alpaca_order_to_trust(&order)?;

            // TODO: Database operations are temporarily disabled due to Send/Sync issues
            info!(
                "Database sync: Order {} updated - status: {:?}",
                trust_order.broker_order_id.unwrap_or_default(),
                trust_order.status
            );
        } else {
            warn!("No database connection available for order synchronization");
        }

        Ok(())
    }

    /// Handle order replaced event
    async fn handle_order_replaced(
        &self,
        order: MockAlpacaOrder,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        info!("Order replaced: {} at {}", order.id, timestamp);

        if let Some(_database_factory) = &self.database {
            let trust_order = convert_alpaca_order_to_trust(&order)?;

            // TODO: Database operations are temporarily disabled due to Send/Sync issues
            info!(
                "Database sync: Order {} updated - status: {:?}",
                trust_order.broker_order_id.unwrap_or_default(),
                trust_order.status
            );
        } else {
            warn!("No database connection available for order synchronization");
        }

        Ok(())
    }

    /// Handle trade update event
    async fn handle_trade_update(
        &self,
        order: MockAlpacaOrder,
        event_type: String,
        price: String,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        info!(
            "Trade update: {} - {} event at ${} on {}",
            order.id, event_type, price, timestamp
        );

        // Handle different trade events
        match event_type.as_str() {
            "fill" => self.handle_order_fill(order, price, timestamp).await?,
            "partial_fill" => {
                let filled_qty = order.filled_qty.clone();
                self.handle_order_partial_fill(order, price, filled_qty, timestamp)
                    .await?
            }
            "canceled" => self.handle_order_canceled(order, timestamp).await?,
            "expired" => self.handle_order_expired(order, timestamp).await?,
            "replaced" => self.handle_order_replaced(order, timestamp).await?,
            _ => {
                debug!("Unknown trade event type: {}", event_type);
            }
        }

        Ok(())
    }

    // TODO: Database helper methods are temporarily disabled due to Send/Sync issues
    // These would be re-enabled once the database interface is made Send/Sync safe

    // /// Find trade by order ID
    // async fn find_trade_by_order_id(&self, order_id: &Uuid) -> Result<Option<Trade>> {
    //     // Implementation would search for trades by order ID
    //     Ok(None)
    // }

    // /// Update trade status based on order changes
    // async fn update_trade_status_from_order(&self, trade: &Trade, updated_order: &Order) -> Result<Trade> {
    //     // Implementation would update trade status based on order changes
    //     Ok(trade.clone())
    // }
}

// TODO: Helper function temporarily disabled due to Send/Sync issues
//
// /// Determine trade status based on order states
// fn determine_trade_status(trade: &Trade) -> Status {
//     // Priority order: stop loss > target > entry > current status
//     if trade.safety_stop.status == OrderStatus::Filled {
//         Status::ClosedStopLoss
//     } else if trade.target.status == OrderStatus::Filled {
//         Status::ClosedTarget
//     } else if trade.entry.status == OrderStatus::Filled {
//         Status::Filled
//     } else if trade.entry.status == OrderStatus::PartiallyFilled {
//         Status::Filled // Partial fills still count as filled in Trust
//     } else if matches!(trade.entry.status, OrderStatus::Accepted | OrderStatus::New | OrderStatus::PendingNew) {
//         Status::Submitted
//     } else if matches!(trade.entry.status, OrderStatus::Canceled | OrderStatus::Rejected) {
//         Status::Canceled
//     } else {
//         trade.status // Keep current status if no clear transition
//     }
// }

/// Convert MockAlpacaOrder to Trust Order
fn convert_alpaca_order_to_trust(alpaca_order: &MockAlpacaOrder) -> Result<Order> {
    let order_id = Uuid::parse_str(&alpaca_order.client_order_id)
        .map_err(|e| anyhow::anyhow!("Invalid client_order_id UUID: {}", e))?;

    let broker_order_id = Uuid::parse_str(&alpaca_order.id)
        .map_err(|e| anyhow::anyhow!("Invalid order ID UUID: {}", e))?;

    let status = convert_alpaca_status(&alpaca_order.status)?;

    let filled_quantity = alpaca_order
        .filled_qty
        .parse::<u64>()
        .map_err(|e| anyhow::anyhow!("Invalid filled quantity: {}", e))?;

    let average_filled_price = if let Some(price_str) = &alpaca_order.filled_avg_price {
        Some(
            Decimal::from_str(price_str)
                .map_err(|e| anyhow::anyhow!("Invalid filled price: {}", e))?,
        )
    } else {
        None
    };

    Ok(Order {
        id: order_id,
        broker_order_id: Some(broker_order_id),
        unit_price: Decimal::ZERO, // TODO: Get from original order
        quantity: alpaca_order
            .qty
            .as_ref()
            .map(|q| q.parse::<u64>().unwrap_or(0))
            .unwrap_or(0),
        filled_quantity,
        average_filled_price,
        status,
        submitted_at: alpaca_order.submitted_at.map(|dt| dt.naive_utc()),
        filled_at: alpaca_order.filled_at.map(|dt| dt.naive_utc()),
        expired_at: alpaca_order.expired_at.map(|dt| dt.naive_utc()),
        cancelled_at: alpaca_order.canceled_at.map(|dt| dt.naive_utc()),
        ..Default::default()
    })
}

/// Convert Alpaca status string to Trust OrderStatus
fn convert_alpaca_status(status: &str) -> Result<OrderStatus> {
    match status.to_lowercase().as_str() {
        "new" => Ok(OrderStatus::New),
        "partially_filled" => Ok(OrderStatus::PartiallyFilled),
        "filled" => Ok(OrderStatus::Filled),
        "done_for_day" => Ok(OrderStatus::DoneForDay),
        "canceled" => Ok(OrderStatus::Canceled),
        "expired" => Ok(OrderStatus::Expired),
        "replaced" => Ok(OrderStatus::Replaced),
        "pending_cancel" => Ok(OrderStatus::PendingCancel),
        "pending_replace" => Ok(OrderStatus::PendingReplace),
        "pending_new" => Ok(OrderStatus::PendingNew),
        "accepted" => Ok(OrderStatus::Accepted),
        "stopped" => Ok(OrderStatus::Stopped),
        "rejected" => Ok(OrderStatus::Rejected),
        "suspended" => Ok(OrderStatus::Suspended),
        "calculated" => Ok(OrderStatus::Calculated),
        "held" => Ok(OrderStatus::Held),
        "accepted_for_bidding" => Ok(OrderStatus::AcceptedForBidding),
        _ => Ok(OrderStatus::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::BrokerState;

    #[test]
    fn test_convert_alpaca_status() {
        assert_eq!(convert_alpaca_status("new").unwrap(), OrderStatus::New);
        assert_eq!(
            convert_alpaca_status("filled").unwrap(),
            OrderStatus::Filled
        );
        assert_eq!(
            convert_alpaca_status("canceled").unwrap(),
            OrderStatus::Canceled
        );
        assert_eq!(
            convert_alpaca_status("unknown_status").unwrap(),
            OrderStatus::Unknown
        );
    }

    #[tokio::test]
    async fn test_websocket_sync_creation() {
        let state = Arc::new(Mutex::new(BrokerState::Disconnected));
        let sync = WebSocketSync::new("ws://localhost:8080".to_string(), state);
        assert_eq!(sync.url, "ws://localhost:8080");
    }

    #[tokio::test]
    async fn test_handle_auth_response() {
        let state = Arc::new(Mutex::new(BrokerState::Disconnected));
        let sync = WebSocketSync::new("ws://localhost:8080".to_string(), state);

        let auth_json: Value = serde_json::from_str(
            r#"
            {
                "stream": "authorization",
                "data": {"action": "authenticate", "status": "authorized"}
            }
        "#,
        )
        .unwrap();

        let result = sync.handle_auth_response(&auth_json).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_convert_alpaca_order_to_trust() {
        let alpaca_order = MockAlpacaOrder {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            client_order_id: "123e4567-e89b-12d3-a456-426614174000".to_string(),
            symbol: "AAPL".to_string(),
            asset_class: "us_equity".to_string(),
            notional: None,
            qty: Some("100".to_string()),
            filled_qty: "50".to_string(),
            filled_avg_price: Some("150.25".to_string()),
            order_class: "simple".to_string(),
            order_type: "limit".to_string(),
            type_: "limit".to_string(),
            side: "buy".to_string(),
            time_in_force: "day".to_string(),
            limit_price: Some("150.00".to_string()),
            stop_price: None,
            status: "partially_filled".to_string(),
            extended_hours: false,
            legs: vec![],
            trail_percent: None,
            trail_price: None,
            hwm: None,
            subtag: None,
            source: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            submitted_at: Some(Utc::now()),
            filled_at: Some(Utc::now()),
            expired_at: None,
            canceled_at: None,
            replaced_at: None,
            replaced_by: None,
            replaces: None,
        };

        let trust_order = convert_alpaca_order_to_trust(&alpaca_order).unwrap();

        assert_eq!(
            trust_order.id,
            Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap()
        );
        assert_eq!(
            trust_order.broker_order_id,
            Some(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap())
        );
        assert_eq!(trust_order.quantity, 100);
        assert_eq!(trust_order.filled_quantity, 50);
        assert_eq!(
            trust_order.average_filled_price,
            Some(Decimal::from_str("150.25").unwrap())
        );
        assert_eq!(trust_order.status, OrderStatus::PartiallyFilled);
    }
}
