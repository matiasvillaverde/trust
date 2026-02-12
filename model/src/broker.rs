use crate::{Account, BarTimeframe, MarketBar, Order, Status, Trade};
use chrono::NaiveDateTime;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::error::Error;
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
}
