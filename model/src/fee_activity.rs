use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use uuid::Uuid;

/// Normalized broker fee/pass-through activity.
#[derive(Debug, Clone, PartialEq)]
pub struct FeeActivity {
    /// Broker identifier (e.g. `alpaca`).
    pub broker: String,
    /// Broker-native activity identifier used for dedupe.
    pub broker_activity_id: String,
    /// Related account.
    pub account_id: Uuid,
    /// Optional broker order id when provided by the broker.
    pub broker_order_id: Option<Uuid>,
    /// Optional symbol when provided by the broker.
    pub symbol: Option<String>,
    /// Raw activity type (`FEE`, `PTC`, etc).
    pub activity_type: String,
    /// Positive fee amount (cost).
    pub amount: Decimal,
    /// Activity timestamp.
    pub occurred_at: NaiveDateTime,
    /// Optional raw JSON payload.
    pub raw_json: Option<String>,
}
