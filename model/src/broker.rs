use crate::{
    Account, BarTimeframe, BrokerKind, Execution, FeeActivity, MarketBar, MarketDataChannel,
    MarketDataStreamEvent, MarketQuote, MarketTradeTick, Order, Status, Trade,
    TradingVehicleCategory,
};
use chrono::NaiveDateTime;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::error::Error;
use uuid::Uuid;

/// Broker-level errors that callers can inspect instead of parsing strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokerError {
    /// The selected broker does not support the requested asset category.
    UnsupportedAssetClass {
        /// Broker that rejected the asset category.
        broker: BrokerKind,
        /// Asset category that is not supported by the broker operation.
        category: TradingVehicleCategory,
        /// Human-readable remediation message.
        message: String,
    },
}

impl BrokerError {
    /// Creates a typed unsupported-asset-class error.
    pub fn unsupported_asset_class(
        broker: BrokerKind,
        category: TradingVehicleCategory,
        message: impl Into<String>,
    ) -> Self {
        Self::UnsupportedAssetClass {
            broker,
            category,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedAssetClass {
                broker,
                category,
                message,
            } => write!(f, "{broker} does not support {category}: {message}"),
        }
    }
}

impl Error for BrokerError {}

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
    pub stop: String,
    /// ID of the entry order
    pub entry: String,
    /// ID of the target/take profit order
    pub target: String,
}

/// Trait for implementing broker integrations
pub trait Broker {
    /// Stable broker identity used for runtime dispatch.
    fn kind(&self) -> BrokerKind;

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
    ) -> Result<String, Box<dyn Error>>;

    /// Modify the target price of an existing trade
    fn modify_target(
        &self,
        trade: &Trade,
        account: &Account,
        new_price: Decimal,
    ) -> Result<String, Box<dyn Error>>;

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

    /// Retrieve latest quote for a symbol from the broker's market data API.
    fn get_latest_quote(
        &self,
        _symbol: &str,
        _account: &Account,
    ) -> Result<MarketQuote, Box<dyn Error>> {
        Err("Latest quote not supported by this broker".into())
    }

    /// Retrieve latest trade tick for a symbol from the broker's market data API.
    fn get_latest_trade(
        &self,
        _symbol: &str,
        _account: &Account,
    ) -> Result<MarketTradeTick, Box<dyn Error>> {
        Err("Latest trade not supported by this broker".into())
    }

    /// Retrieve a finite batch of realtime market-data events.
    fn stream_market_data(
        &self,
        _symbols: &[String],
        _channels: &[MarketDataChannel],
        _max_events: usize,
        _timeout_seconds: u64,
        _account: &Account,
    ) -> Result<Vec<MarketDataStreamEvent>, Box<dyn Error>> {
        Err("Realtime market data streaming not supported by this broker".into())
    }

    /// Fetch broker executions (fills) for a trade since an optional timestamp.
    ///
    /// Default implementation returns an empty list. Brokers that support an execution feed
    /// (REST and/or websocket) should override this to enable execution-level accounting.
    fn fetch_executions(
        &self,
        _trade: &Trade,
        _account: &Account,
        _after: Option<DateTime<Utc>>,
    ) -> Result<Vec<Execution>, Box<dyn Error>> {
        Ok(vec![])
    }

    /// Fetch non-fill fee activities relevant to trading costs (`FEE`, `PTC`, etc).
    ///
    /// Default implementation returns an empty list. Implementations may provide a richer
    /// reconciliation source for execution accounting.
    fn fetch_fee_activities(
        &self,
        _trade: &Trade,
        _account: &Account,
        _after: Option<DateTime<Utc>>,
    ) -> Result<Vec<FeeActivity>, Box<dyn Error>> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct UnsupportedMarketDataBroker;

    impl Broker for UnsupportedMarketDataBroker {
        fn kind(&self) -> BrokerKind {
            BrokerKind::Alpaca
        }

        fn submit_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
            Err("submit not implemented".into())
        }

        fn sync_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
            Err("sync not implemented".into())
        }

        fn close_trade(
            &self,
            _trade: &Trade,
            _account: &Account,
        ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
            Err("close not implemented".into())
        }

        fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
            Err("cancel not implemented".into())
        }

        fn modify_stop(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_stop_price: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Err("modify stop not implemented".into())
        }

        fn modify_target(
            &self,
            _trade: &Trade,
            _account: &Account,
            _new_price: Decimal,
        ) -> Result<String, Box<dyn Error>> {
            Err("modify target not implemented".into())
        }
    }

    #[test]
    fn default_market_data_methods_return_explicit_unsupported_errors() {
        let broker = UnsupportedMarketDataBroker;
        let account = Account::default();
        let trade = Trade::default();

        assert_eq!(broker.kind(), BrokerKind::Alpaca);

        assert_eq!(
            broker
                .submit_trade(&trade, &account)
                .unwrap_err()
                .to_string(),
            "submit not implemented"
        );
        assert_eq!(
            broker.sync_trade(&trade, &account).unwrap_err().to_string(),
            "sync not implemented"
        );
        assert_eq!(
            broker
                .close_trade(&trade, &account)
                .unwrap_err()
                .to_string(),
            "close not implemented"
        );
        assert_eq!(
            broker
                .cancel_trade(&trade, &account)
                .unwrap_err()
                .to_string(),
            "cancel not implemented"
        );
        assert_eq!(
            broker
                .modify_stop(&trade, &account, Decimal::ONE)
                .unwrap_err()
                .to_string(),
            "modify stop not implemented"
        );
        assert_eq!(
            broker
                .modify_target(&trade, &account, Decimal::ONE)
                .unwrap_err()
                .to_string(),
            "modify target not implemented"
        );

        let quote_error = broker.get_latest_quote("AAPL", &account).unwrap_err();
        assert_eq!(
            quote_error.to_string(),
            "Latest quote not supported by this broker"
        );

        let trade_error = broker.get_latest_trade("AAPL", &account).unwrap_err();
        assert_eq!(
            trade_error.to_string(),
            "Latest trade not supported by this broker"
        );

        let events_error = broker
            .stream_market_data(
                &[String::from("AAPL")],
                &[MarketDataChannel::Quotes],
                1,
                1,
                &account,
            )
            .unwrap_err();
        assert_eq!(
            events_error.to_string(),
            "Realtime market data streaming not supported by this broker"
        );
    }
}
