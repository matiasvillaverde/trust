//! Legacy market output command responsibility. Business mutations go through TrustFacade.
use crate::dispatcher::market_data;
use crate::dispatcher::market_data::MarketDataStreamRequest;
use crate::dispatcher::output;
use chrono::DateTime;
use chrono::Utc;
use model::Account;
use model::MarketSnapshotSource;
use serde_json::json;
use serde_json::Value;

pub(in crate::dispatcher) fn market_snapshot_payload(
    account: &Account,
    requested_at: DateTime<Utc>,
    snapshot: &model::MarketSnapshotV2,
) -> Value {
    let lag_seconds = requested_at
        .signed_duration_since(snapshot.as_of)
        .num_seconds()
        .max(0);
    json!({
        "report": "market_data_snapshot",
        "schema_version": 1,
        "generated_at": requested_at.to_rfc3339(),
        "source": {
            "broker": account.broker_kind.as_str(),
            "provider": format!("{}_market_data", account.broker_kind)
        },
        "scope": {
            "account_id": account.id.to_string(),
            "symbol": snapshot.symbol,
        },
        "provenance": {
            "source_kind": market_data::market_snapshot_source_label(&snapshot.source),
            "fallback_used": matches!(snapshot.source, MarketSnapshotSource::BarsFallback),
        },
        "freshness": {
            "as_of": snapshot.as_of.to_rfc3339(),
            "lag_seconds": lag_seconds,
        },
        "data": {
            "last_price": output::decimal_string(snapshot.last_price),
            "open": output::decimal_string(snapshot.open),
            "high": output::decimal_string(snapshot.high),
            "low": output::decimal_string(snapshot.low),
            "volume": snapshot.volume,
            "quote": snapshot.quote.as_ref().map(|quote| {
                json!({
                    "as_of": quote.as_of.to_rfc3339(),
                    "bid_price": output::decimal_string(quote.bid_price),
                    "bid_size": quote.bid_size,
                    "ask_price": output::decimal_string(quote.ask_price),
                    "ask_size": quote.ask_size,
                })
            }),
            "trade": snapshot.trade.as_ref().map(|trade| {
                json!({
                    "as_of": trade.as_of.to_rfc3339(),
                    "price": output::decimal_string(trade.price),
                    "size": trade.size,
                })
            }),
        }
    })
}

pub(in crate::dispatcher) fn market_data_stream_payload(
    account: &Account,
    requested_at: DateTime<Utc>,
    request: &MarketDataStreamRequest,
    events: &[model::MarketDataStreamEvent],
) -> Value {
    let events_json: Vec<Value> = events
        .iter()
        .map(|event| {
            json!({
                "channel": market_data::market_data_channel_json_label(event.channel),
                "symbol": event.symbol,
                "as_of": event.as_of.to_rfc3339(),
                "price": output::decimal_string(event.price),
                "size": event.size,
            })
        })
        .collect();
    json!({
        "report": "market_data_stream",
        "schema_version": 1,
        "generated_at": requested_at.to_rfc3339(),
        "source": {
            "broker": account.broker_kind.as_str(),
            "provider": format!("{}_market_data_stream", account.broker_kind)
        },
        "scope": {
            "account_id": account.id.to_string(),
            "symbols": request.symbols,
            "channels": request.channels_raw,
        },
        "request": {
            "max_events": request.max_events,
            "timeout_seconds": request.timeout_seconds,
        },
        "data": {
            "count": events_json.len(),
            "events": events_json,
        }
    })
}

pub(in crate::dispatcher) fn market_data_bars_payload(
    account: &Account,
    requested_at: DateTime<Utc>,
    symbol: &str,
    timeframe_raw: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    bars: &[model::MarketBar],
) -> Value {
    let bars_json: Vec<Value> = bars
        .iter()
        .map(|bar| {
            json!({
                "time": bar.time.to_rfc3339(),
                "open": output::decimal_string(bar.open),
                "high": output::decimal_string(bar.high),
                "low": output::decimal_string(bar.low),
                "close": output::decimal_string(bar.close),
                "volume": bar.volume,
            })
        })
        .collect();

    json!({
        "report": "market_data_bars",
        "schema_version": 1,
        "generated_at": requested_at.to_rfc3339(),
        "source": {
            "broker": account.broker_kind.as_str(),
            "provider": format!("{}_market_data", account.broker_kind)
        },
        "scope": {
            "account_id": account.id.to_string(),
            "symbol": symbol,
        },
        "request": {
            "timeframe": timeframe_raw,
            "start": start.to_rfc3339(),
            "end": end.to_rfc3339(),
        },
        "data": {
            "count": bars_json.len(),
            "bars": bars_json,
        }
    })
}
