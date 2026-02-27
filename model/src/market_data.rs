use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

/// Supported bar timeframes for market data retrieval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
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

/// Best-effort market snapshot for a symbol.
///
/// This is intentionally minimal and broker-agnostic. It is derived from the
/// latest available market bar when a broker does not expose richer typed
/// snapshot endpoints.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketSnapshot {
    /// Symbol associated with this snapshot.
    pub symbol: String,
    /// Timestamp of the latest known market point.
    pub as_of: DateTime<Utc>,
    /// Last trade/close-like price.
    pub last_price: Decimal,
    /// Latest known volume from the same bar.
    pub volume: u64,
    /// Latest known open price from the same bar.
    pub open: Decimal,
    /// Latest known high price from the same bar.
    pub high: Decimal,
    /// Latest known low price from the same bar.
    pub low: Decimal,
}

/// A best bid/ask quote for a symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketQuote {
    /// Symbol associated with this quote.
    pub symbol: String,
    /// Quote timestamp.
    pub as_of: DateTime<Utc>,
    /// Best bid price.
    pub bid_price: Decimal,
    /// Best bid size.
    pub bid_size: u64,
    /// Best ask price.
    pub ask_price: Decimal,
    /// Best ask size.
    pub ask_size: u64,
}

/// A market trade tick for a symbol.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketTradeTick {
    /// Symbol associated with this trade.
    pub symbol: String,
    /// Trade timestamp.
    pub as_of: DateTime<Utc>,
    /// Last trade price.
    pub price: Decimal,
    /// Last trade size.
    pub size: u64,
}

/// Origin used to compose a [`MarketSnapshotV2`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketSnapshotSource {
    /// Snapshot was composed from quote + trade data.
    QuoteTrade,
    /// Snapshot was composed from market bars.
    BarsFallback,
}

/// Richer market snapshot for LLM-friendly consumption.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketSnapshotV2 {
    /// Symbol associated with this snapshot.
    pub symbol: String,
    /// Snapshot timestamp.
    pub as_of: DateTime<Utc>,
    /// Last trade/close-like price.
    pub last_price: Decimal,
    /// Last known volume.
    pub volume: u64,
    /// Latest known open price.
    pub open: Decimal,
    /// Latest known high price.
    pub high: Decimal,
    /// Latest known low price.
    pub low: Decimal,
    /// Optional best-quote enrichment.
    pub quote: Option<MarketQuote>,
    /// Optional last-trade enrichment.
    pub trade: Option<MarketTradeTick>,
    /// Composition origin.
    pub source: MarketSnapshotSource,
}

/// Streaming channel emitted by a broker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketDataChannel {
    /// Aggregate bars.
    Bars,
    /// Quotes.
    Quotes,
    /// Trades.
    Trades,
}

/// Single normalized market-data streaming event.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketDataStreamEvent {
    /// Emitted channel.
    pub channel: MarketDataChannel,
    /// Symbol associated with this event.
    pub symbol: String,
    /// Event timestamp.
    pub as_of: DateTime<Utc>,
    /// Price (bar close / quote mid / trade price).
    pub price: Decimal,
    /// Size/volume associated with the event.
    pub size: u64,
}
