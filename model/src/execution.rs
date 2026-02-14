use chrono::NaiveDateTime;
use chrono::Utc;
use rust_decimal::Decimal;
use std::str::FromStr;
use uuid::Uuid;

/// Where we learned about an execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionSource {
    /// Real-time `trade_updates` websocket.
    TradeUpdates,
    /// REST `/v2/account/activities` reconciliation.
    AccountActivities,
}

impl std::fmt::Display for ExecutionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionSource::TradeUpdates => write!(f, "trade_updates"),
            ExecutionSource::AccountActivities => write!(f, "account_activities"),
        }
    }
}

/// Error returned when parsing an invalid execution source.
#[derive(Debug, PartialEq)]
pub struct ExecutionSourceParseError;

impl FromStr for ExecutionSource {
    type Err = ExecutionSourceParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "trade_updates" => Ok(ExecutionSource::TradeUpdates),
            "account_activities" => Ok(ExecutionSource::AccountActivities),
            _ => Err(ExecutionSourceParseError),
        }
    }
}

/// Trade execution side as used by Alpaca payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionSide {
    /// Buy.
    Buy,
    /// Sell.
    Sell,
    /// Short sell.
    SellShort,
}

impl std::fmt::Display for ExecutionSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionSide::Buy => write!(f, "buy"),
            ExecutionSide::Sell => write!(f, "sell"),
            ExecutionSide::SellShort => write!(f, "sell_short"),
        }
    }
}

/// Error returned when parsing an invalid execution side.
#[derive(Debug, PartialEq)]
pub struct ExecutionSideParseError;

impl FromStr for ExecutionSide {
    type Err = ExecutionSideParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "buy" => Ok(ExecutionSide::Buy),
            "sell" => Ok(ExecutionSide::Sell),
            "sell_short" => Ok(ExecutionSide::SellShort),
            _ => Err(ExecutionSideParseError),
        }
    }
}

/// An immutable broker execution (fill).
///
/// This is the primitive event that order/trade accounting should be derived from.
#[derive(Clone, Debug, PartialEq)]
pub struct Execution {
    /// Unique identifier for this execution row.
    pub id: Uuid,

    // Entity timestamps
    /// When the execution row was created locally.
    pub created_at: NaiveDateTime,
    /// When the execution row was last updated locally.
    pub updated_at: NaiveDateTime,
    /// When the execution row was soft-deleted locally (if ever).
    pub deleted_at: Option<NaiveDateTime>,

    // Identity / ownership
    /// Broker identifier (e.g., `alpaca`).
    pub broker: String,
    /// Where this execution was sourced from.
    pub source: ExecutionSource,
    /// Account this execution belongs to.
    pub account_id: Uuid,

    /// Optional linkage to a trade we know about.
    pub trade_id: Option<Uuid>,
    /// Optional linkage to a local order we know about (via `client_order_id`).
    pub order_id: Option<Uuid>,

    /// Broker execution identifier (must be unique per broker+account).
    pub broker_execution_id: String,
    /// Broker order identifier (if available).
    pub broker_order_id: Option<Uuid>,

    /// Executed symbol (as reported by broker).
    pub symbol: String,
    /// Broker-reported side for this execution.
    pub side: ExecutionSide,

    /// Filled quantity. Decimal to support fractional fills (e.g., crypto).
    pub qty: Decimal,
    /// Executed price.
    pub price: Decimal,
    /// When the execution happened.
    pub executed_at: NaiveDateTime,

    /// Optional raw payload for audit/debugging.
    pub raw_json: Option<String>,
}

impl Execution {
    /// Construct a new execution.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        broker: String,
        source: ExecutionSource,
        account_id: Uuid,
        broker_execution_id: String,
        broker_order_id: Option<Uuid>,
        symbol: String,
        side: ExecutionSide,
        qty: Decimal,
        price: Decimal,
        executed_at: NaiveDateTime,
    ) -> Self {
        let now = Utc::now().naive_utc();
        Self {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            broker,
            source,
            account_id,
            trade_id: None,
            order_id: None,
            broker_execution_id,
            broker_order_id,
            symbol,
            side,
            qty,
            price,
            executed_at,
            raw_json: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_source_parse_roundtrip() {
        let src: ExecutionSource = "trade_updates".parse().unwrap();
        assert_eq!(src, ExecutionSource::TradeUpdates);
        assert_eq!(src.to_string(), "trade_updates");
    }

    #[test]
    fn test_execution_side_parse_roundtrip() {
        let side: ExecutionSide = "sell_short".parse().unwrap();
        assert_eq!(side, ExecutionSide::SellShort);
        assert_eq!(side.to_string(), "sell_short");
    }
}
