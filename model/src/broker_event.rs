use chrono::NaiveDateTime;
use uuid::Uuid;

/// Immutable record of a broker-originated event (e.g. Alpaca `trade_updates`).
///
/// This is intended for auditability and replay/debugging. Payload JSON is stored
/// verbatim; do not mutate or normalize it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokerEvent {
    /// Unique identifier.
    pub id: Uuid,
    /// Creation timestamp (UTC, naive).
    pub created_at: NaiveDateTime,
    /// Last update timestamp (UTC, naive).
    pub updated_at: NaiveDateTime,
    /// Soft-delete timestamp when applicable.
    pub deleted_at: Option<NaiveDateTime>,

    /// Owning account.
    pub account_id: Uuid,
    /// Associated trade.
    pub trade_id: Uuid,

    /// Broker/source identifier, e.g. `alpaca`.
    pub source: String,
    /// Stream identifier, e.g. `trade_updates`.
    pub stream: String,
    /// Event type, e.g. `fill`, `partial_fill`, `canceled`.
    pub event_type: String,

    /// Broker-native order id when available.
    pub broker_order_id: Option<Uuid>,

    /// Raw JSON payload as received/serialized.
    pub payload_json: String,
}
