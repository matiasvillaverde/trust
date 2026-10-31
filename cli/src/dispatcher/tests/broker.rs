#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use chrono::TimeZone;
use chrono::Utc;
use model::Broker;
use model::BrokerKind;
use model::BrokerLog;
use model::MarketBar;
use model::MarketDataChannel;
use model::MarketDataStreamEvent;
use model::MarketQuote;
use model::MarketTradeTick;
use model::OrderIds;
use model::Status;
use model::Trade;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;

#[derive(Clone)]
pub(super) struct StubBroker {
    pub(super) bars: Vec<MarketBar>,
    pub(super) quote: Option<MarketQuote>,
    pub(super) trade: Option<MarketTradeTick>,
    pub(super) events: Vec<MarketDataStreamEvent>,
    pub(super) fail_bars: bool,
    pub(super) fail_stream: bool,
    pub(super) submit_succeeds: bool,
    pub(super) sync_fills_entry: bool,
    pub(super) sync_returns_empty: bool,
}

impl StubBroker {
    pub(super) fn quote_trade_fixture() -> Self {
        Self {
            bars: vec![MarketBar {
                time: Utc
                    .with_ymd_and_hms(2026, 2, 24, 10, 0, 0)
                    .single()
                    .expect("valid fixture timestamp"),
                open: dec!(198),
                high: dec!(205),
                low: dec!(197),
                close: dec!(201),
                volume: 12_345,
            }],
            quote: Some(MarketQuote {
                symbol: "AAPL".to_string(),
                as_of: Utc
                    .with_ymd_and_hms(2026, 2, 24, 10, 0, 0)
                    .single()
                    .expect("valid fixture timestamp"),
                bid_price: dec!(200.95),
                bid_size: 100,
                ask_price: dec!(201.05),
                ask_size: 120,
            }),
            trade: Some(MarketTradeTick {
                symbol: "AAPL".to_string(),
                as_of: Utc
                    .with_ymd_and_hms(2026, 2, 24, 10, 0, 0)
                    .single()
                    .expect("valid fixture timestamp"),
                price: dec!(201),
                size: 50,
            }),
            events: vec![
                MarketDataStreamEvent {
                    channel: MarketDataChannel::Quotes,
                    symbol: "AAPL".to_string(),
                    as_of: Utc
                        .with_ymd_and_hms(2026, 2, 24, 10, 0, 1)
                        .single()
                        .expect("valid fixture timestamp"),
                    price: dec!(201),
                    size: 20,
                },
                MarketDataStreamEvent {
                    channel: MarketDataChannel::Trades,
                    symbol: "AAPL".to_string(),
                    as_of: Utc
                        .with_ymd_and_hms(2026, 2, 24, 10, 0, 2)
                        .single()
                        .expect("valid fixture timestamp"),
                    price: dec!(201.1),
                    size: 10,
                },
            ],
            fail_bars: false,
            fail_stream: false,
            submit_succeeds: false,
            sync_fills_entry: false,
            sync_returns_empty: false,
        }
    }

    pub(super) fn submitted_fill_fixture() -> Self {
        let mut broker = Self::quote_trade_fixture();
        broker.submit_succeeds = true;
        broker.sync_fills_entry = true;
        broker
    }

    pub(super) fn submitted_empty_sync_fixture() -> Self {
        let mut broker = Self::submitted_fill_fixture();
        broker.sync_returns_empty = true;
        broker
    }
}

impl Broker for StubBroker {
    fn kind(&self) -> BrokerKind {
        BrokerKind::Alpaca
    }

    fn submit_trade(
        &self,
        trade: &Trade,
        _account: &model::Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        if !self.submit_succeeds {
            return Err("not implemented in test stub".into());
        }
        Ok((
            BrokerLog {
                trade_id: trade.id,
                log: "submitted by test stub".to_string(),
                ..BrokerLog::default()
            },
            OrderIds {
                stop: format!("stop-{}", trade.id),
                entry: format!("entry-{}", trade.id),
                target: format!("target-{}", trade.id),
            },
        ))
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        _account: &model::Account,
    ) -> Result<(Status, Vec<model::Order>, BrokerLog), Box<dyn Error>> {
        if !self.sync_fills_entry {
            return Err("not implemented in test stub".into());
        }

        if self.sync_returns_empty {
            return Ok((
                Status::Submitted,
                vec![],
                BrokerLog {
                    trade_id: trade.id,
                    log: "empty sync by test stub".to_string(),
                    ..BrokerLog::default()
                },
            ));
        }

        let mut entry = trade.entry.clone();
        entry.status = model::OrderStatus::Filled;
        entry.filled_quantity = entry.quantity;
        entry.average_filled_price = Some(entry.unit_price);
        entry.filled_at = Some(Utc::now().naive_utc());
        Ok((
            Status::Filled,
            vec![entry],
            BrokerLog {
                trade_id: trade.id,
                log: "synced by test stub".to_string(),
                ..BrokerLog::default()
            },
        ))
    }

    fn close_trade(
        &self,
        _trade: &Trade,
        _account: &model::Account,
    ) -> Result<(model::Order, BrokerLog), Box<dyn Error>> {
        Err("not implemented in test stub".into())
    }

    fn cancel_trade(
        &self,
        _trade: &Trade,
        _account: &model::Account,
    ) -> Result<(), Box<dyn Error>> {
        Err("not implemented in test stub".into())
    }

    fn modify_stop(
        &self,
        _trade: &Trade,
        _account: &model::Account,
        _new_stop_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        Err("not implemented in test stub".into())
    }

    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &model::Account,
        _new_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        Err("not implemented in test stub".into())
    }

    fn get_bars(
        &self,
        _symbol: &str,
        _start: chrono::DateTime<Utc>,
        _end: chrono::DateTime<Utc>,
        _timeframe: model::BarTimeframe,
        _account: &model::Account,
    ) -> Result<Vec<MarketBar>, Box<dyn Error>> {
        if self.fail_bars {
            return Err("bars unavailable".into());
        }
        Ok(self.bars.clone())
    }

    fn get_latest_quote(
        &self,
        _symbol: &str,
        _account: &model::Account,
    ) -> Result<MarketQuote, Box<dyn Error>> {
        self.quote.clone().ok_or_else(|| "quote unavailable".into())
    }

    fn get_latest_trade(
        &self,
        _symbol: &str,
        _account: &model::Account,
    ) -> Result<MarketTradeTick, Box<dyn Error>> {
        self.trade.clone().ok_or_else(|| "trade unavailable".into())
    }

    fn stream_market_data(
        &self,
        _symbols: &[String],
        _channels: &[MarketDataChannel],
        _max_events: usize,
        _timeout_seconds: u64,
        _account: &model::Account,
    ) -> Result<Vec<MarketDataStreamEvent>, Box<dyn Error>> {
        if self.fail_stream {
            return Err("stream unavailable".into());
        }
        Ok(self.events.clone())
    }
}
