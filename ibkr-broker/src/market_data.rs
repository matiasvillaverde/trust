use crate::client::IbkrClient;
use crate::contracts::fetch_contract_metadata_with_client;
use crate::parsing::{
    decimal_field, parse_epoch_datetime, string_field_optional, timestamp_field, u64_field_optional,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use model::{BarTimeframe, MarketBar, MarketQuote, MarketTradeTick};
use std::error::Error;

pub(crate) fn get_bars(
    client: &IbkrClient,
    symbol: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    timeframe: BarTimeframe,
) -> Result<Vec<MarketBar>, Box<dyn Error>> {
    let conid = fetch_contract_metadata_with_client(client, symbol)?.conid;
    let response = client.get_json_value(
        "/iserver/marketdata/history",
        &[
            ("conid", conid),
            ("bar", history_bar(timeframe).to_string()),
            ("period", history_period(start, end, timeframe)),
            ("startTime", format_ibkr_datetime(end.naive_utc())),
            ("outsideRth", "true".to_string()),
        ],
    )?;
    let bars = response
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or("IBKR history response did not include data")?;
    let mut out = Vec::with_capacity(bars.len());
    for bar in bars {
        let Some(time) = parse_epoch_datetime(bar.get("t")) else {
            continue;
        };
        if time < start || time > end {
            continue;
        }
        out.push(MarketBar {
            time,
            open: decimal_field(bar, "o")?,
            high: decimal_field(bar, "h")?,
            low: decimal_field(bar, "l")?,
            close: decimal_field(bar, "c")?,
            volume: u64_field_optional(bar, "v").unwrap_or(0),
        });
    }
    Ok(out)
}

pub(crate) fn get_latest_quote(
    client: &IbkrClient,
    symbol: &str,
) -> Result<MarketQuote, Box<dyn Error>> {
    let conid = fetch_contract_metadata_with_client(client, symbol)?.conid;
    let snapshot = client.snapshot(&conid, &["55", "84", "88", "86", "85"])?;
    Ok(MarketQuote {
        symbol: string_field_optional(&snapshot, "55").unwrap_or_else(|| symbol.to_string()),
        as_of: timestamp_field(&snapshot, "_updated").unwrap_or_else(Utc::now),
        bid_price: decimal_field(&snapshot, "84")?,
        bid_size: u64_field_optional(&snapshot, "88").unwrap_or(0),
        ask_price: decimal_field(&snapshot, "86")?,
        ask_size: u64_field_optional(&snapshot, "85").unwrap_or(0),
    })
}

pub(crate) fn get_latest_trade(
    client: &IbkrClient,
    symbol: &str,
) -> Result<MarketTradeTick, Box<dyn Error>> {
    let conid = fetch_contract_metadata_with_client(client, symbol)?.conid;
    let snapshot = client.snapshot(&conid, &["55", "31", "7059"])?;
    Ok(MarketTradeTick {
        symbol: string_field_optional(&snapshot, "55").unwrap_or_else(|| symbol.to_string()),
        as_of: timestamp_field(&snapshot, "_updated").unwrap_or_else(Utc::now),
        price: decimal_field(&snapshot, "31")?,
        size: u64_field_optional(&snapshot, "7059").unwrap_or(0),
    })
}

fn history_bar(timeframe: BarTimeframe) -> &'static str {
    match timeframe {
        BarTimeframe::OneMinute => "1min",
        BarTimeframe::OneHour => "1h",
        BarTimeframe::OneDay => "1d",
    }
}

pub(crate) fn history_period(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    timeframe: BarTimeframe,
) -> String {
    let duration = end.signed_duration_since(start);
    match timeframe {
        BarTimeframe::OneMinute => format!("{}min", duration.num_minutes().max(1)),
        BarTimeframe::OneHour => format!("{}h", duration.num_hours().max(1)),
        BarTimeframe::OneDay => format!("{}d", duration.num_days().max(1)),
    }
}

pub(crate) fn format_ibkr_datetime(value: NaiveDateTime) -> String {
    value.format("%Y%m%d-%H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::history_period;
    use chrono::{DateTime, Utc};
    use model::BarTimeframe;

    #[test]
    fn history_period_scales_with_requested_timeframe() {
        let start = DateTime::parse_from_rfc3339("2026-03-18T10:00:00Z")
            .expect("valid start")
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2026-03-18T12:00:00Z")
            .expect("valid end")
            .with_timezone(&Utc);
        assert_eq!(
            history_period(start, end, BarTimeframe::OneMinute),
            "120min"
        );
        assert_eq!(history_period(start, end, BarTimeframe::OneHour), "2h");
        assert_eq!(history_period(start, end, BarTimeframe::OneDay), "1d");
    }
}
