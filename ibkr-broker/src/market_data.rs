use crate::client::IbkrClient;
use crate::contracts::fetch_contract_metadata_with_client;
use crate::parsing::{
    decimal_field, parse_epoch_datetime, string_field_optional, timestamp_field, u64_field_optional,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use model::{BarTimeframe, MarketBar, MarketQuote, MarketTradeTick, TradingVehicleCategory};
use std::error::Error;

pub(crate) fn get_bars(
    client: &IbkrClient,
    symbol: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    timeframe: BarTimeframe,
) -> Result<Vec<MarketBar>, Box<dyn Error>> {
    let conid =
        fetch_contract_metadata_with_client(client, symbol, TradingVehicleCategory::Stock)?.conid;
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
    market_bars_from_history_response(&response, start, end)
}

fn market_bars_from_history_response(
    response: &serde_json::Value,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<MarketBar>, Box<dyn Error>> {
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
    let conid =
        fetch_contract_metadata_with_client(client, symbol, TradingVehicleCategory::Stock)?.conid;
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
    let conid =
        fetch_contract_metadata_with_client(client, symbol, TradingVehicleCategory::Stock)?.conid;
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
    use super::{
        format_ibkr_datetime, history_bar, history_period, market_bars_from_history_response,
    };
    use chrono::{DateTime, Utc};
    use model::{BarTimeframe, MarketBar};
    use rust_decimal_macros::dec;
    use serde_json::json;

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

    #[test]
    fn history_bar_and_datetime_formatter_match_ibkr_contracts() {
        let timestamp = DateTime::parse_from_rfc3339("2026-03-18T12:34:56Z")
            .expect("valid timestamp")
            .naive_utc();

        assert_eq!(history_bar(BarTimeframe::OneMinute), "1min");
        assert_eq!(history_bar(BarTimeframe::OneHour), "1h");
        assert_eq!(history_bar(BarTimeframe::OneDay), "1d");
        assert_eq!(format_ibkr_datetime(timestamp), "20260318-12:34:56");
    }

    #[test]
    fn history_response_parser_filters_invalid_and_out_of_window_bars() {
        let start = DateTime::parse_from_rfc3339("2026-03-18T10:00:00Z")
            .expect("valid start")
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2026-03-18T12:00:00Z")
            .expect("valid end")
            .with_timezone(&Utc);
        let response = json!({
            "data": [
                {
                    "t": start.timestamp_millis(),
                    "o": "100.00",
                    "h": "101.00",
                    "l": "99.50",
                    "c": "100.25",
                    "v": 500
                },
                {
                    "t": "not-an-epoch",
                    "o": "1",
                    "h": "1",
                    "l": "1",
                    "c": "1"
                },
                {
                    "t": (end + chrono::Duration::minutes(1)).timestamp_millis(),
                    "o": "102.00",
                    "h": "103.00",
                    "l": "101.00",
                    "c": "102.50",
                    "v": 900
                }
            ]
        });

        let bars = market_bars_from_history_response(&response, start, end)
            .expect("history response should parse");

        assert_eq!(
            bars,
            vec![MarketBar {
                time: start,
                open: dec!(100.00),
                high: dec!(101.00),
                low: dec!(99.50),
                close: dec!(100.25),
                volume: 500,
            }]
        );
    }
}
