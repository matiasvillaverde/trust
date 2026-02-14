use crate::{Account, BarTimeframe, MarketBar, Order, Status, Trade};
use chrono::NaiveDateTime;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::error::Error;
use std::time::Duration;
use uuid::Uuid;

/// Log entry for broker operations
#[derive(Debug)]
pub struct BrokerLog {
    /// Unique identifier for the log entry
    pub id: Uuid,

    // Entity timestamps
    /// Timestamp when the log was created
    pub created_at: NaiveDateTime,
    /// Timestamp when the log was last updated
    pub updated_at: NaiveDateTime,
    /// Optional timestamp when the log was deleted
    pub deleted_at: Option<NaiveDateTime>,

    // Entity fields
    /// ID of the trade associated with this log
    pub trade_id: Uuid,
    /// Log message content
    pub log: String,
}

impl Default for BrokerLog {
    fn default() -> Self {
        let now = chrono::Utc::now().naive_utc();
        Self {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
            trade_id: Uuid::new_v4(),
            log: String::new(),
        }
    }
}

/// Container for order IDs associated with a trade
#[derive(Debug)]
pub struct OrderIds {
    /// ID of the stop loss order
    pub stop: Uuid,
    /// ID of the entry order
    pub entry: Uuid,
    /// ID of the target/take profit order
    pub target: Uuid,
}

/// Options controlling broker watch behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchOptions {
    /// Periodic REST reconciliation interval to heal missed websocket events.
    pub reconcile_every: Duration,
    /// Optional overall timeout for a watch session.
    pub timeout: Option<Duration>,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            reconcile_every: Duration::from_secs(20),
            timeout: None,
        }
    }
}

/// Control flow signal returned by watch callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchControl {
    /// Keep watching.
    Continue,
    /// Stop the watch session gracefully.
    Stop,
}

/// A broker-emitted watch event containing domain-order updates plus the raw payload.
#[derive(Debug, Clone, PartialEq)]
pub struct WatchEvent {
    /// Broker/source identifier, e.g. `alpaca`.
    pub broker_source: String,
    /// Stream identifier, e.g. `trade_updates` or `market_data`.
    pub broker_stream: String,
    /// Updated domain orders (subset) affected by this broker event.
    pub updated_orders: Vec<Order>,
    /// Optional human-readable broker message.
    pub message: Option<String>,
    /// Broker-native event type (e.g. `fill`, `partial_fill`).
    pub broker_event_type: String,
    /// Broker-native order id when available.
    pub broker_order_id: Option<Uuid>,
    /// Optional market price observed during this watch session.
    ///
    /// This is typically populated from a market-data websocket stream and
    /// should be treated as informational (not authoritative for fills).
    pub market_price: Option<Decimal>,
    /// Optional market timestamp associated with `market_price`.
    pub market_timestamp: Option<DateTime<Utc>>,
    /// Optional market symbol associated with `market_price`.
    pub market_symbol: Option<String>,
    /// Raw JSON payload for audit/replay.
    pub payload_json: String,
}

/// Trait for implementing broker integrations
pub trait Broker {
    /// Submit a new trade to the broker
    fn submit_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>>;

    /// Synchronize trade status with the broker
    fn sync_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>>;

    /// Manually Close a trade
    /// The target will be cancelled and a new target will be created
    /// with the market price. The goal is to close the trade as soon as possible.
    /// The return value is the new target order.
    fn close_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>>;

    /// Cancel a trade that has been submitted
    /// The order should not be filled
    fn cancel_trade(&self, trade: &Trade, account: &Account) -> Result<(), Box<dyn Error>>;

    /// Modify the stop loss price of an existing trade
    fn modify_stop(
        &self,
        trade: &Trade,
        account: &Account,
        new_stop_price: Decimal,
    ) -> Result<Uuid, Box<dyn Error>>;

    /// Modify the target price of an existing trade
    fn modify_target(
        &self,
        trade: &Trade,
        account: &Account,
        new_price: Decimal,
    ) -> Result<Uuid, Box<dyn Error>>;

    /// Retrieve market bars for a symbol from the broker's market data API (if supported).
    ///
    /// Implementations may return an error if market data isn't available for the broker/account.
    fn get_bars(
        &self,
        _symbol: &str,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
        _timeframe: BarTimeframe,
        _account: &Account,
    ) -> Result<Vec<MarketBar>, Box<dyn Error>> {
        Err("Market data not supported by this broker".into())
    }

    /// Watch a trade in near real-time, emitting order updates as broker events occur.
    ///
    /// Implementations should be resilient to websocket disconnects and should
    /// periodically reconcile via REST (per `options.reconcile_every`) to heal gaps.
    ///
    /// Default implementation returns an error so non-streaming brokers do not break.
    fn watch_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
        _options: WatchOptions,
        _on_event: &mut dyn FnMut(WatchEvent) -> Result<WatchControl, Box<dyn Error>>,
    ) -> Result<(), Box<dyn Error>> {
        Err("watch_trade not supported by this broker".into())
    }
}
