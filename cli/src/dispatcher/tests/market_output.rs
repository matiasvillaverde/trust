#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use chrono::TimeZone;
use chrono::Utc;
use model::MarketBar;
use model::MarketQuote;
use model::MarketSnapshotSource;
use model::MarketSnapshotV2;
use model::MarketTradeTick;
use rust_decimal_macros::dec;

#[test]
fn test_market_snapshot_payload_includes_quote_trade_and_freshness() {
    let requested_at = Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 30).unwrap();
    let snapshot_as_of = Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 0).unwrap();
    let snapshot = MarketSnapshotV2 {
        symbol: "AAPL".to_string(),
        as_of: snapshot_as_of,
        last_price: dec!(201.12),
        volume: 123,
        open: dec!(200),
        high: dec!(202),
        low: dec!(199.5),
        quote: Some(MarketQuote {
            symbol: "AAPL".to_string(),
            as_of: snapshot_as_of,
            bid_price: dec!(201.1),
            bid_size: 4,
            ask_price: dec!(201.2),
            ask_size: 5,
        }),
        trade: Some(MarketTradeTick {
            symbol: "AAPL".to_string(),
            as_of: snapshot_as_of,
            price: dec!(201.12),
            size: 10,
        }),
        source: MarketSnapshotSource::QuoteTrade,
    };

    let payload = crate::dispatcher::market_output::market_snapshot_payload(
        &model::Account::default(),
        requested_at,
        &snapshot,
    );
    assert_eq!(payload["report"], "market_data_snapshot");
    assert_eq!(payload["scope"]["symbol"], "AAPL");
    assert_eq!(payload["provenance"]["source_kind"], "quote_trade");
    assert_eq!(payload["provenance"]["fallback_used"], false);
    assert_eq!(payload["freshness"]["lag_seconds"], 30);
    assert_eq!(payload["data"]["quote"]["bid_price"], "201.1");
    assert_eq!(payload["data"]["trade"]["size"], 10);
}

#[test]
fn test_market_data_bars_payload_includes_request_and_ohlcv() {
    let requested_at = Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 0).unwrap();
    let start = Utc.with_ymd_and_hms(2026, 2, 27, 9, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 0).unwrap();
    let bars = vec![MarketBar {
        time: start,
        open: dec!(200),
        high: dec!(202),
        low: dec!(199),
        close: dec!(201),
        volume: 1000,
    }];

    let payload = crate::dispatcher::market_output::market_data_bars_payload(
        &model::Account {
            broker_kind: model::BrokerKind::Ibkr,
            ..model::Account::default()
        },
        requested_at,
        "AAPL",
        "1m",
        start,
        end,
        &bars,
    );
    assert_eq!(payload["report"], "market_data_bars");
    assert_eq!(payload["source"]["broker"], "ibkr");
    assert_eq!(payload["source"]["provider"], "ibkr_market_data");
    assert_eq!(payload["scope"]["symbol"], "AAPL");
    assert_eq!(payload["request"]["timeframe"], "1m");
    assert_eq!(payload["data"]["count"], 1);
    assert_eq!(payload["data"]["bars"][0]["close"], "201");
    assert_eq!(payload["data"]["bars"][0]["volume"], 1000);
}
