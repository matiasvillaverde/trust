//! Message types for the BrokerSync actor

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use uuid::Uuid;

/// Commands that can be sent to the broker actor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BrokerCommand {
    /// Start synchronization for an account
    StartSync { account_id: Uuid },
    /// Stop synchronization for an account
    StopSync { account_id: Uuid },
    /// Trigger manual reconciliation
    ManualReconcile {
        account_id: Option<Uuid>,
        force: bool,
    },
    /// Get current status
    GetStatus,
    /// Shutdown the actor
    Shutdown,
}

/// Events emitted by the broker actor
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum BrokerEvent {
    /// WebSocket connected
    Connected {
        account_id: Uuid,
        /// Sanitized URL (sensitive parts redacted)
        websocket_url: String,
    },
    /// WebSocket disconnected
    Disconnected { account_id: Uuid, reason: String },
    /// Order update received
    OrderUpdated {
        account_id: Uuid,
        update: OrderUpdate,
    },
    /// Reconciliation completed
    ReconciliationComplete {
        account_id: Uuid,
        status: ReconciliationStatus,
    },
    /// Error occurred
    Error {
        account_id: Option<Uuid>,
        error: String,
        recoverable: bool,
    },
    /// Status response (for testing compatibility)
    GetStatus,
}

/// Order update details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderUpdate {
    pub trade_id: Uuid,
    pub order_type: String,
    pub old_status: String,
    pub new_status: String,
    pub filled_qty: Option<u32>,
    pub filled_price: Option<Decimal>,
    pub message: Option<String>,
}

/// Reconciliation status details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReconciliationStatus {
    pub orders_checked: u32,
    pub orders_updated: u32,
    pub errors: Vec<String>,
    #[serde(with = "serde_duration")]
    pub duration: Duration,
}

impl BrokerEvent {
    /// Create a Connected event with sanitized URL
    pub fn connected(account_id: Uuid, raw_url: &str) -> Self {
        BrokerEvent::Connected {
            account_id,
            websocket_url: sanitize_url(raw_url),
        }
    }
}

/// Sanitize WebSocket URL to remove sensitive information
fn sanitize_url(url: &str) -> String {
    if let Ok(mut parsed) = url.parse::<url::Url>() {
        // Remove query parameters that might contain tokens
        parsed.set_query(None);

        // Remove password from URL if present
        let _ = parsed.set_password(None);

        // If username exists, replace with "****"
        if parsed.username() != "" {
            let _ = parsed.set_username("****");
        }

        parsed.to_string()
    } else {
        // If parsing fails, return a generic placeholder
        "wss://[redacted]".to_string()
    }
}

/// Custom serialization for Duration
mod serde_duration {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let millis = u64::deserialize(deserializer)?;
        Ok(Duration::from_millis(millis))
    }
}
