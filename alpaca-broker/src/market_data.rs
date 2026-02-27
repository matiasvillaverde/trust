use crate::keys;
use apca::data::v2::bars::{List, ListReqInit, TimeFrame};
use apca::data::v2::last_quotes::{Get, GetReqInit};
use apca::data::v2::stream::{Data, MarketData, RealtimeData, IEX};
use apca::data::v2::trades;
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

pub fn get_bars(
    symbol: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    timeframe: BarTimeframe,
    account: &Account,
) -> Result<Vec<MarketBar>, Box<dyn Error>> {
    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);

    let request = ListReqInit::default().init(symbol, start, end, map_timeframe(timeframe));
    let response = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(client.issue::<List>(&request))
        .map_err(|error| format!("Failed to fetch bars from Alpaca: {error}"))?;

    let mut bars: Vec<MarketBar> = Vec::with_capacity(response.bars.len());
    for bar in response.bars {
        bars.push(MarketBar {
            time: bar.time,
            open: num_to_decimal(&bar.open)?,
            high: num_to_decimal(&bar.high)?,
            low: num_to_decimal(&bar.low)?,
            close: num_to_decimal(&bar.close)?,
            volume: u64::try_from(bar.volume).unwrap_or(0),
        });
    }
    Ok(bars)
}

pub fn get_latest_quote(symbol: &str, account: &Account) -> Result<MarketQuote, Box<dyn Error>> {
    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);
    let request = GetReqInit::default().init([symbol.to_string()]);
    let response = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(client.issue::<Get>(&request))
        .map_err(|error| format!("Failed to fetch latest quote from Alpaca: {error}"))?;
    let (_, quote) = response
        .into_iter()
        .next()
        .ok_or_else(|| "No latest quote returned by Alpaca".to_string())?;

    Ok(MarketQuote {
        symbol: symbol.to_string(),
        as_of: quote.time,
        bid_price: num_to_decimal(&quote.bid_price)?,
        bid_size: quote.bid_size,
        ask_price: num_to_decimal(&quote.ask_price)?,
        ask_size: quote.ask_size,
    })
}

pub fn get_latest_trade(
    symbol: &str,
    account: &Account,
) -> Result<MarketTradeTick, Box<dyn Error>> {
    let api_info = keys::read_api_key(&account.environment, account)?;
    let client = Client::new(api_info);
    let end = Utc::now();
    let start = end
        .checked_sub_signed(chrono::Duration::hours(24))
        .unwrap_or(end);
    let request = trades::ListReqInit {
        limit: Some(1000),
        ..Default::default()
    }
    .init(symbol.to_string(), start, end);
    let response = Runtime::new()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?
        .block_on(client.issue::<trades::List>(&request))
        .map_err(|error| format!("Failed to fetch latest trade from Alpaca: {error}"))?;

    let trade = response
        .trades
        .into_iter()
        .max_by_key(|trade| trade.timestamp)
        .ok_or_else(|| "No latest trade returned by Alpaca".to_string())?;
    Ok(MarketTradeTick {
        symbol: symbol.to_string(),
        as_of: trade.timestamp,
        price: num_to_decimal(&trade.price)?,
        size: u64::try_from(trade.size).unwrap_or(0),
    })
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
                        Data::Bar(bar) => {
                            let close = num_to_decimal(&bar.close_price)?;
                            let volume = num_to_decimal(&bar.volume)?.trunc().to_u64().unwrap_or(0);
                            MarketDataStreamEvent {
                                channel: MarketDataChannel::Bars,
                                symbol: bar.symbol,
                                as_of: bar.timestamp,
                                price: close,
                                size: volume,
                            }
                        }
                        Data::Quote(quote) => {
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
                            MarketDataStreamEvent {
                                channel: MarketDataChannel::Quotes,
                                symbol: quote.symbol,
                                as_of: quote.timestamp,
                                price: mid,
                                size,
                            }
                        }
                        Data::Trade(trade) => MarketDataStreamEvent {
                            channel: MarketDataChannel::Trades,
                            symbol: trade.symbol,
                            as_of: trade.timestamp,
                            price: num_to_decimal(&trade.trade_price)?,
                            size: num_to_decimal(&trade.trade_size)?
                                .trunc()
                                .to_u64()
                                .unwrap_or(0),
                        },
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
    use super::{map_timeframe, num_to_decimal};
    use model::BarTimeframe;
    use std::str::FromStr;

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
}
