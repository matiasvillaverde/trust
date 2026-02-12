use crate::keys;
use apca::data::v2::bars::{List, ListReqInit, TimeFrame};
use apca::Client;
use chrono::{DateTime, Utc};
use model::{Account, BarTimeframe, MarketBar};
use rust_decimal::Decimal;
use std::error::Error;
use std::str::FromStr;
use tokio::runtime::Runtime;

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
