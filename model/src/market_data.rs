use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

/// Supported bar timeframes for market data retrieval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarTimeframe {
    /// One-minute bars.
    OneMinute,
    /// One-hour bars.
    OneHour,
    /// One-day bars.
    OneDay,
}

/// OHLCV bar for a symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketBar {
    /// Bar timestamp (start of the bar).
    pub time: DateTime<Utc>,
    /// Open price.
    pub open: Decimal,
    /// High price.
    pub high: Decimal,
    /// Low price.
    pub low: Decimal,
    /// Close price.
    pub close: Decimal,
    /// Volume.
    pub volume: u64,
}

