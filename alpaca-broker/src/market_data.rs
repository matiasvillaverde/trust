use crate::keys;
use apca::data::v2::bars::{
    Bar as AlpacaBar, Bars as AlpacaBars, List, ListReq as BarsListReq, ListReqInit, TimeFrame,
};
use apca::data::v2::last_quotes::{Get, GetReq as QuoteGetReq, GetReqInit, Quote as AlpacaQuote};
use apca::data::v2::stream::{
    Bar as StreamBar, Data, MarketData, Quote as StreamQuote, RealtimeData, Trade as StreamTrade,
    IEX,
};
use apca::data::v2::trades::{self, Trade as AlpacaTrade};
use apca::Client;
use chrono::{DateTime, Utc};
use futures_util::FutureExt as _;
use futures_util::StreamExt as _;
use model::{
    Account, BarTimeframe, MarketBar, MarketDataChannel, MarketDataStreamEvent, MarketQuote,
    MarketTradeTick,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use tokio::runtime::Runtime;
use tokio::time::{timeout, Duration};

fn map_timeframe(tf: BarTimeframe) -> TimeFrame {
    match tf {
        BarTimeframe::OneMinute => TimeFrame::OneMinute,
        BarTimeframe::OneHour => TimeFrame::OneHour,
        BarTimeframe::OneDay => TimeFrame::OneDay,
    }
}

fn num_to_decimal(value: &num_decimal::Num) -> Result<Decimal, Box<dyn Error>> {
    Decimal::from_str(&value.to_string())
        .map_err(|e| format!("Failed to parse decimal: {e}").into())
}

fn validate_symbol(symbol: &str) -> Result<(), Box<dyn Error>> {
    if symbol.trim().is_empty() {
        return Err("Symbol cannot be empty".into());
    }
    Ok(())
}

fn market_bar_from_alpaca(bar: AlpacaBar) -> Result<MarketBar, Box<dyn Error>> {
    Ok(MarketBar {
        time: bar.time,
        open: num_to_decimal(&bar.open)?,
        high: num_to_decimal(&bar.high)?,
        low: num_to_decimal(&bar.low)?,
        close: num_to_decimal(&bar.close)?,
        volume: u64::try_from(bar.volume).unwrap_or(0),
    })
}

fn market_quote_from_alpaca(
    symbol: &str,
    quote: AlpacaQuote,
) -> Result<MarketQuote, Box<dyn Error>> {
    Ok(MarketQuote {
        symbol: symbol.to_string(),
        as_of: quote.time,
        bid_price: num_to_decimal(&quote.bid_price)?,
        bid_size: quote.bid_size,
        ask_price: num_to_decimal(&quote.ask_price)?,
        ask_size: quote.ask_size,
    })
}

fn market_trade_tick_from_alpaca(
    symbol: &str,
    trade: AlpacaTrade,
) -> Result<MarketTradeTick, Box<dyn Error>> {
    Ok(MarketTradeTick {
        symbol: symbol.to_string(),
        as_of: trade.timestamp,
        price: num_to_decimal(&trade.price)?,
        size: u64::try_from(trade.size).unwrap_or(0),
    })
}

fn bars_request(
    symbol: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    timeframe: BarTimeframe,
) -> BarsListReq {
    ListReqInit::default().init(symbol, start, end, map_timeframe(timeframe))
}

fn market_bars_from_alpaca(response: AlpacaBars) -> Result<Vec<MarketBar>, Box<dyn Error>> {
    response
        .bars
        .into_iter()
        .map(market_bar_from_alpaca)
        .collect()
}

fn latest_quote_request(symbol: &str) -> QuoteGetReq {
    GetReqInit::default().init([symbol.to_string()])
}

fn latest_quote_from_response(
    symbol: &str,
    response: Vec<(String, AlpacaQuote)>,
) -> Result<MarketQuote, Box<dyn Error>> {
    let (_, quote) = response
        .into_iter()
        .next()
        .ok_or_else(|| "No latest quote returned by Alpaca".to_string())?;

    market_quote_from_alpaca(symbol, quote)
}

fn latest_trade_request(symbol: &str, end: DateTime<Utc>) -> trades::ListReq {
    let start = end
        .checked_sub_signed(chrono::Duration::hours(24))
        .unwrap_or(end);
    trades::ListReqInit {
        limit: Some(1000),
        ..Default::default()
    }
    .init(symbol.to_string(), start, end)
}

fn latest_trade_from_response(
    symbol: &str,
    response: trades::Trades,
) -> Result<MarketTradeTick, Box<dyn Error>> {
    let trade = response
        .trades
        .into_iter()
        .max_by_key(|trade| trade.timestamp)
        .ok_or_else(|| "No latest trade returned by Alpaca".to_string())?;
    market_trade_tick_from_alpaca(symbol, trade)
}

fn stream_bar_event(bar: StreamBar) -> Result<MarketDataStreamEvent, Box<dyn Error>> {
    let close = num_to_decimal(&bar.close_price)?;
    let volume = num_to_decimal(&bar.volume)?.trunc().to_u64().unwrap_or(0);
    Ok(MarketDataStreamEvent {
        channel: MarketDataChannel::Bars,
        symbol: bar.symbol,
        as_of: bar.timestamp,
        price: close,
        size: volume,
    })
}

fn stream_quote_event(quote: StreamQuote) -> Result<MarketDataStreamEvent, Box<dyn Error>> {
    let bid = num_to_decimal(&quote.bid_price)?;
    let ask = num_to_decimal(&quote.ask_price)?;
    let mid = bid
        .checked_add(ask)
        .and_then(|value| value.checked_div(Decimal::from(2)))
        .unwrap_or(bid);
    let size = num_to_decimal(&quote.bid_size)?
        .trunc()
        .to_u64()
        .unwrap_or(0)
        .saturating_add(
            num_to_decimal(&quote.ask_size)?
                .trunc()
                .to_u64()
                .unwrap_or(0),
        );
    Ok(MarketDataStreamEvent {
        channel: MarketDataChannel::Quotes,
        symbol: quote.symbol,
        as_of: quote.timestamp,
        price: mid,
        size,
    })
}

fn stream_trade_event(trade: StreamTrade) -> Result<MarketDataStreamEvent, Box<dyn Error>> {
    Ok(MarketDataStreamEvent {
        channel: MarketDataChannel::Trades,
        symbol: trade.symbol,
        as_of: trade.timestamp,
        price: num_to_decimal(&trade.trade_price)?,
        size: num_to_decimal(&trade.trade_size)?
            .trunc()
            .to_u64()
            .unwrap_or(0),
    })
}

pub fn get_bars(
    symbol: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    timeframe: BarTimeframe,
    account: &Account,
) -> Result<Vec<MarketBar>, Box<dyn Error>> {
    validate_symbol(symbol)?;
    if end <= start {
        return Err("Bar end time must be after start time".into());
    }

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    let request = bars_request(symbol, start, end, timeframe);
    let response = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(client.issue::<List>(&request))
        .map_err(|error| format!("Failed to fetch bars from Alpaca: {error}"))?;

    market_bars_from_alpaca(response)
}

pub fn get_latest_quote(symbol: &str, account: &Account) -> Result<MarketQuote, Box<dyn Error>> {
    validate_symbol(symbol)?;

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);
    let request = latest_quote_request(symbol);
    let response = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(client.issue::<Get>(&request))
        .map_err(|error| format!("Failed to fetch latest quote from Alpaca: {error}"))?;

    latest_quote_from_response(symbol, response)
}

pub fn get_latest_trade(
    symbol: &str,
    account: &Account,
) -> Result<MarketTradeTick, Box<dyn Error>> {
    validate_symbol(symbol)?;

    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);
    let end = Utc::now();
    let request = latest_trade_request(symbol, end);
    let response = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(client.issue::<trades::List>(&request))
        .map_err(|error| format!("Failed to fetch latest trade from Alpaca: {error}"))?;

    latest_trade_from_response(symbol, response)
}

#[allow(clippy::too_many_lines)]
pub fn stream_market_data(
    symbols: &[String],
    channels: &[MarketDataChannel],
    max_events: usize,
    timeout_seconds: u64,
    account: &Account,
) -> Result<Vec<MarketDataStreamEvent>, Box<dyn Error>> {
    if symbols.is_empty() {
        return Err("At least one symbol is required for streaming".into());
    }
    if channels.is_empty() {
        return Err("At least one channel is required for streaming".into());
    }
    if max_events == 0 {
        return Ok(Vec::new());
    }

    let api_info = keys::read_api_key(&account.environment, account)?;
    let symbols_owned: Vec<String> = symbols.iter().map(|symbol| symbol.to_string()).collect();
    let channels_owned: Vec<MarketDataChannel> = channels.to_vec();

    Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(async move {
            let client = Client::new(api_info);
            let (mut stream, mut subscription) = client
                .subscribe::<RealtimeData<IEX>>()
                .await
                .map_err(|error| format!("Failed to connect market-data stream: {error}"))?;

            let mut req = MarketData::default();
            if channels_owned.contains(&MarketDataChannel::Bars) {
                req.set_bars(symbols_owned.clone());
            }
            if channels_owned.contains(&MarketDataChannel::Quotes) {
                req.set_quotes(symbols_owned.clone());
            }
            if channels_owned.contains(&MarketDataChannel::Trades) {
                req.set_trades(symbols_owned.clone());
            }

            let subscribe = subscription.subscribe(&req).boxed_local();
            let result = apca::data::v2::stream::drive(subscribe, &mut stream)
                .await
                .map_err(|error| format!("Failed to drive market-data subscription: {error:?}"))?
                .map_err(|error| format!("Failed to subscribe market data: {error}"))?;
            result?;

            let mut events: Vec<MarketDataStreamEvent> = Vec::new();
            let timeout_at = Duration::from_secs(timeout_seconds);
            let read_loop = async {
                while let Some(message) = stream.next().await {
                    let data = message
                        .map_err(|error| format!("WebSocket market-data error: {error}"))?
                        .map_err(|error| format!("Market-data JSON parse error: {error}"))?;
                    let event = match data {
                        Data::Bar(bar) => stream_bar_event(bar)?,
                        Data::Quote(quote) => stream_quote_event(quote)?,
                        Data::Trade(trade) => stream_trade_event(trade)?,
                        _ => continue,
                    };
                    events.push(event);
                    if events.len() >= max_events {
                        break;
                    }
                }
                Ok::<(), Box<dyn Error>>(())
            };
            let _ = timeout(timeout_at, read_loop).await;
            Ok(events)
        })
}

#[cfg(test)]
mod tests {
    use super::{
        bars_request, get_bars, get_latest_quote, get_latest_trade, latest_quote_from_response,
        latest_quote_request, latest_trade_from_response, latest_trade_request, map_timeframe,
        market_bar_from_alpaca, market_bars_from_alpaca, market_quote_from_alpaca,
        market_trade_tick_from_alpaca, num_to_decimal, stream_bar_event, stream_market_data,
        stream_quote_event, stream_trade_event,
    };
    use chrono::{DateTime, Utc};
    use model::{Account, BarTimeframe, MarketDataChannel};
    use std::str::FromStr;

    fn utc(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("valid timestamp")
            .with_timezone(&Utc)
    }

    #[test]
    fn map_timeframe_maps_all_supported_variants() {
        assert!(matches!(
            map_timeframe(BarTimeframe::OneMinute),
            apca::data::v2::bars::TimeFrame::OneMinute
        ));
        assert!(matches!(
            map_timeframe(BarTimeframe::OneHour),
            apca::data::v2::bars::TimeFrame::OneHour
        ));
        assert!(matches!(
            map_timeframe(BarTimeframe::OneDay),
            apca::data::v2::bars::TimeFrame::OneDay
        ));
    }

    #[test]
    fn num_to_decimal_parses_valid_num() {
        let n = num_decimal::Num::from_str("42.001").expect("num parse");
        let d = num_to_decimal(&n).expect("decimal parse");
        assert_eq!(d.to_string(), "42.001");
    }

    #[test]
    fn market_bar_from_alpaca_preserves_ohlcv_values() {
        let raw = r#"{
            "t": "2026-05-07T13:00:00Z",
            "o": "183.10",
            "h": "184.25",
            "l": "182.99",
            "c": "184.01",
            "v": 12345,
            "vw": "183.75"
        }"#;
        let bar = serde_json::from_str(raw).expect("valid bar json");

        let mapped = market_bar_from_alpaca(bar).expect("bar maps");

        assert_eq!(mapped.time, utc("2026-05-07T13:00:00Z"));
        assert_eq!(mapped.open.to_string(), "183.1");
        assert_eq!(mapped.high.to_string(), "184.25");
        assert_eq!(mapped.low.to_string(), "182.99");
        assert_eq!(mapped.close.to_string(), "184.01");
        assert_eq!(mapped.volume, 12345);
    }

    #[test]
    fn bars_request_and_response_mapping_are_deterministic() {
        let start = utc("2026-05-07T13:00:00Z");
        let end = utc("2026-05-07T14:00:00Z");
        let request = bars_request("msft", start, end, BarTimeframe::OneHour);

        assert_eq!(request.symbol, "msft");
        assert_eq!(request.start, start);
        assert_eq!(request.end, end);
        assert!(matches!(
            request.timeframe,
            apca::data::v2::bars::TimeFrame::OneHour
        ));

        let raw = r#"{
            "bars": [
                {
                    "t": "2026-05-07T13:00:00Z",
                    "o": "101.00",
                    "h": "102.00",
                    "l": "100.50",
                    "c": "101.75",
                    "v": 100,
                    "vw": "101.40"
                },
                {
                    "t": "2026-05-07T14:00:00Z",
                    "o": "101.75",
                    "h": "103.00",
                    "l": "101.25",
                    "c": "102.25",
                    "v": 200,
                    "vw": "102.10"
                }
            ],
            "symbol": "MSFT",
            "next_page_token": null
        }"#;
        let response = serde_json::from_str(raw).expect("valid bars response");
        let mapped = market_bars_from_alpaca(response).expect("bars response maps");

        assert_eq!(mapped.len(), 2);
        assert!(mapped
            .iter()
            .any(|bar| bar.time == start && bar.close.to_string() == "101.75"));
        assert!(mapped
            .iter()
            .any(|bar| bar.time == end && bar.close.to_string() == "102.25"));
    }

    #[test]
    fn market_quote_from_alpaca_preserves_bid_ask_snapshot() {
        let raw = r#"{
            "t": "2026-05-07T13:00:01Z",
            "ap": "184.12",
            "as": 7,
            "bp": "184.10",
            "bs": 9
        }"#;
        let quote = serde_json::from_str(raw).expect("valid quote json");

        let mapped = market_quote_from_alpaca("AAPL", quote).expect("quote maps");

        assert_eq!(mapped.symbol, "AAPL");
        assert_eq!(mapped.as_of, utc("2026-05-07T13:00:01Z"));
        assert_eq!(mapped.bid_price.to_string(), "184.1");
        assert_eq!(mapped.bid_size, 9);
        assert_eq!(mapped.ask_price.to_string(), "184.12");
        assert_eq!(mapped.ask_size, 7);
    }

    #[test]
    fn latest_quote_request_and_response_mapping_are_deterministic() {
        let request = latest_quote_request("AAPL");
        assert_eq!(request.symbols, vec!["AAPL".to_string()]);

        let raw = r#"{
            "t": "2026-05-07T13:00:01Z",
            "ap": "184.12",
            "as": 7,
            "bp": "184.10",
            "bs": 9
        }"#;
        let quote = serde_json::from_str(raw).expect("valid quote json");
        let response = vec![("BROKER-SYMBOL".to_string(), quote)];

        let mapped =
            latest_quote_from_response("AAPL", response).expect("latest quote response maps");

        assert_eq!(mapped.symbol, "AAPL");
        assert_eq!(mapped.bid_price.to_string(), "184.1");
    }

    #[test]
    fn latest_quote_response_errors_when_empty() {
        let error = latest_quote_from_response("AAPL", Vec::new()).expect_err("empty quote fails");

        assert_eq!(error.to_string(), "No latest quote returned by Alpaca");
    }

    #[test]
    fn market_trade_tick_from_alpaca_uses_trade_price_and_size() {
        let raw = r#"{
            "t": "2026-05-07T13:00:02Z",
            "p": "184.11",
            "s": 33
        }"#;
        let trade = serde_json::from_str(raw).expect("valid trade json");

        let mapped = market_trade_tick_from_alpaca("AAPL", trade).expect("trade maps");

        assert_eq!(mapped.symbol, "AAPL");
        assert_eq!(mapped.as_of, utc("2026-05-07T13:00:02Z"));
        assert_eq!(mapped.price.to_string(), "184.11");
        assert_eq!(mapped.size, 33);
    }

    #[test]
    fn latest_trade_request_and_response_mapping_are_deterministic() {
        let end = utc("2026-05-07T14:00:00Z");
        let request = latest_trade_request("AAPL", end);

        assert_eq!(request.symbol, "AAPL");
        assert_eq!(request.end, end);
        assert_eq!(request.start, utc("2026-05-06T14:00:00Z"));
        assert_eq!(request.limit, Some(1000));

        let raw = r#"{
            "trades": [
                {
                    "t": "2026-05-07T13:00:02Z",
                    "p": "184.11",
                    "s": 33
                },
                {
                    "t": "2026-05-07T13:15:02Z",
                    "p": "184.99",
                    "s": 44
                }
            ],
            "symbol": "AAPL",
            "next_page_token": null
        }"#;
        let response = serde_json::from_str(raw).expect("valid trades response");

        let mapped =
            latest_trade_from_response("AAPL", response).expect("latest trade response maps");

        assert_eq!(mapped.symbol, "AAPL");
        assert_eq!(mapped.as_of, utc("2026-05-07T13:15:02Z"));
        assert_eq!(mapped.price.to_string(), "184.99");
        assert_eq!(mapped.size, 44);
    }

    #[test]
    fn latest_trade_request_falls_back_to_end_when_start_would_underflow() {
        let end = DateTime::<Utc>::MIN_UTC;

        let request = latest_trade_request("AAPL", end);

        assert_eq!(request.start, end);
        assert_eq!(request.end, end);
        assert_eq!(request.limit, Some(1000));
    }

    #[test]
    fn latest_trade_response_errors_when_empty() {
        let raw = r#"{
            "trades": [],
            "symbol": "AAPL",
            "next_page_token": null
        }"#;
        let response = serde_json::from_str(raw).expect("valid empty trades response");

        let error =
            latest_trade_from_response("AAPL", response).expect_err("empty trades should fail");

        assert_eq!(error.to_string(), "No latest trade returned by Alpaca");
    }

    #[test]
    fn stream_bar_event_uses_close_and_truncated_volume() {
        let bar = apca::data::v2::stream::Bar {
            symbol: "AAPL".to_string(),
            open_price: num_decimal::Num::from_str("183.00").expect("num"),
            high_price: num_decimal::Num::from_str("184.00").expect("num"),
            low_price: num_decimal::Num::from_str("182.50").expect("num"),
            close_price: num_decimal::Num::from_str("183.75").expect("num"),
            volume: num_decimal::Num::from_str("42.99").expect("num"),
            timestamp: utc("2026-05-07T13:00:03Z"),
        };

        let event = stream_bar_event(bar).expect("bar event");

        assert_eq!(event.channel, MarketDataChannel::Bars);
        assert_eq!(event.symbol, "AAPL");
        assert_eq!(event.as_of, utc("2026-05-07T13:00:03Z"));
        assert_eq!(event.price.to_string(), "183.75");
        assert_eq!(event.size, 42);
    }

    #[test]
    fn stream_bar_event_clamps_negative_volume_to_zero() {
        let bar = apca::data::v2::stream::Bar {
            symbol: "AAPL".to_string(),
            open_price: num_decimal::Num::from_str("183.00").expect("num"),
            high_price: num_decimal::Num::from_str("184.00").expect("num"),
            low_price: num_decimal::Num::from_str("182.50").expect("num"),
            close_price: num_decimal::Num::from_str("183.75").expect("num"),
            volume: num_decimal::Num::from_str("-42.99").expect("num"),
            timestamp: utc("2026-05-07T13:00:03Z"),
        };

        let event = stream_bar_event(bar).expect("bar event");

        assert_eq!(event.size, 0);
    }

    #[test]
    fn stream_quote_event_uses_midpoint_and_saturating_size() {
        let quote = apca::data::v2::stream::Quote {
            symbol: "AAPL".to_string(),
            bid_price: num_decimal::Num::from_str("184.10").expect("num"),
            bid_size: num_decimal::Num::from_str("18446744073709551615").expect("num"),
            ask_price: num_decimal::Num::from_str("184.12").expect("num"),
            ask_size: num_decimal::Num::from_str("2.9").expect("num"),
            timestamp: utc("2026-05-07T13:00:04Z"),
        };

        let event = stream_quote_event(quote).expect("quote event");

        assert_eq!(event.channel, MarketDataChannel::Quotes);
        assert_eq!(event.symbol, "AAPL");
        assert_eq!(event.as_of, utc("2026-05-07T13:00:04Z"));
        assert_eq!(event.price.to_string(), "184.11");
        assert_eq!(event.size, u64::MAX);
    }

    #[test]
    fn stream_quote_event_clamps_negative_sizes_to_zero() {
        let quote = apca::data::v2::stream::Quote {
            symbol: "AAPL".to_string(),
            bid_price: num_decimal::Num::from_str("184.10").expect("num"),
            bid_size: num_decimal::Num::from_str("-9.9").expect("num"),
            ask_price: num_decimal::Num::from_str("184.12").expect("num"),
            ask_size: num_decimal::Num::from_str("-2.9").expect("num"),
            timestamp: utc("2026-05-07T13:00:04Z"),
        };

        let event = stream_quote_event(quote).expect("quote event");

        assert_eq!(event.size, 0);
    }

    #[test]
    fn stream_trade_event_uses_trade_price_and_truncated_size() {
        let trade = apca::data::v2::stream::Trade {
            symbol: "AAPL".to_string(),
            trade_id: 123,
            trade_price: num_decimal::Num::from_str("184.13").expect("num"),
            trade_size: num_decimal::Num::from_str("17.8").expect("num"),
            timestamp: utc("2026-05-07T13:00:05Z"),
        };

        let event = stream_trade_event(trade).expect("trade event");

        assert_eq!(event.channel, MarketDataChannel::Trades);
        assert_eq!(event.symbol, "AAPL");
        assert_eq!(event.as_of, utc("2026-05-07T13:00:05Z"));
        assert_eq!(event.price.to_string(), "184.13");
        assert_eq!(event.size, 17);
    }

    #[test]
    fn stream_trade_event_clamps_negative_size_to_zero() {
        let trade = apca::data::v2::stream::Trade {
            symbol: "AAPL".to_string(),
            trade_id: 123,
            trade_price: num_decimal::Num::from_str("184.13").expect("num"),
            trade_size: num_decimal::Num::from_str("-17.8").expect("num"),
            timestamp: utc("2026-05-07T13:00:05Z"),
        };

        let event = stream_trade_event(trade).expect("trade event");

        assert_eq!(event.size, 0);
    }

    #[test]
    fn public_market_data_calls_reject_invalid_inputs_before_credentials() {
        let account = Account::default();
        let start = utc("2026-05-07T13:00:00Z");
        let end = utc("2026-05-07T14:00:00Z");

        let error = get_bars(" \t\n", start, end, BarTimeframe::OneMinute, &account)
            .expect_err("blank symbol should be rejected before key lookup");
        assert_eq!(error.to_string(), "Symbol cannot be empty");

        let error = get_bars("AAPL", end, start, BarTimeframe::OneMinute, &account)
            .expect_err("invalid bar window should be rejected before key lookup");
        assert_eq!(error.to_string(), "Bar end time must be after start time");

        let error = get_latest_quote("", &account)
            .expect_err("blank quote symbol should be rejected before key lookup");
        assert_eq!(error.to_string(), "Symbol cannot be empty");

        let error = get_latest_trade("\n", &account)
            .expect_err("blank trade symbol should be rejected before key lookup");
        assert_eq!(error.to_string(), "Symbol cannot be empty");
    }

    #[test]
    fn stream_market_data_rejects_empty_symbols_before_credentials() {
        let account = Account::default();
        let channels = vec![MarketDataChannel::Quotes];

        let error =
            stream_market_data(&[], &channels, 1, 1, &account).expect_err("missing symbols");

        assert_eq!(
            error.to_string(),
            "At least one symbol is required for streaming"
        );
    }

    #[test]
    fn stream_market_data_rejects_empty_channels_before_credentials() {
        let account = Account::default();
        let symbols = vec!["AAPL".to_string()];

        let error =
            stream_market_data(&symbols, &[], 1, 1, &account).expect_err("missing channels");

        assert_eq!(
            error.to_string(),
            "At least one channel is required for streaming"
        );
    }

    #[test]
    fn stream_market_data_zero_event_limit_returns_empty_before_credentials() {
        let account = Account::default();
        let symbols = vec!["AAPL".to_string()];
        let channels = vec![MarketDataChannel::Bars, MarketDataChannel::Quotes];

        let events = stream_market_data(&symbols, &channels, 0, 1, &account)
            .expect("zero max events should not connect");

        assert!(events.is_empty());
    }
}
