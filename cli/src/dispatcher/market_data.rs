//! Legacy market data command responsibility. Business mutations go through TrustFacade.
use crate::command_routing::parse_market_data_subcommand;
use crate::command_routing::MarketDataSubcommand;
use crate::dispatcher::context;
use crate::dispatcher::error::CliError;
use crate::dispatcher::market_output;
use crate::dispatcher::output;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::DateTime;
use chrono::Utc;
use clap::ArgMatches;
use core::TrustFacade;
use model::BarTimeframe;
use model::MarketDataChannel;
use model::MarketSnapshotSource;
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::dispatcher) struct MarketDataStreamRequest {
    pub(in crate::dispatcher) symbols: Vec<String>,
    pub(in crate::dispatcher) channels_raw: String,
    pub(in crate::dispatcher) channels: Vec<MarketDataChannel>,
    pub(in crate::dispatcher) max_events: usize,
    pub(in crate::dispatcher) timeout_seconds: u64,
}

pub(in crate::dispatcher) fn dispatch_market_data(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
) -> Result<(), CliError> {
    let format = output::parse_report_format(sub_matches);
    let subcommand = parse_market_data_subcommand(sub_matches);
    let (handler, target_matches) = market_data_handler_for(subcommand);
    handler(trust, target_matches, format)?;
    Ok(())
}

pub(in crate::dispatcher) fn market_snapshot_source_label(
    source: &MarketSnapshotSource,
) -> &'static str {
    match source {
        MarketSnapshotSource::QuoteTrade => "quote_trade",
        MarketSnapshotSource::BarsFallback => "bars_fallback",
    }
}

pub(in crate::dispatcher) fn market_data_channel_text_label(
    channel: MarketDataChannel,
) -> &'static str {
    match channel {
        MarketDataChannel::Bars => "bar",
        MarketDataChannel::Quotes => "quote",
        MarketDataChannel::Trades => "trade",
    }
}

pub(in crate::dispatcher) fn market_data_channel_json_label(
    channel: MarketDataChannel,
) -> &'static str {
    match channel {
        MarketDataChannel::Bars => "bars",
        MarketDataChannel::Quotes => "quotes",
        MarketDataChannel::Trades => "trades",
    }
}

pub(in crate::dispatcher) fn parse_symbol_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|value| value.trim().to_uppercase())
        .filter(|value| !value.is_empty())
        .collect()
}

pub(in crate::dispatcher) fn parse_market_data_stream_request(
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<MarketDataStreamRequest, CliError> {
    let symbols_raw = sub_matches.get_one::<String>("symbols").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --symbols")
    })?;
    let channels_raw = sub_matches.get_one::<String>("channels").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --channels")
    })?;
    let max_events_raw = sub_matches.get_one::<String>("max-events").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --max-events")
    })?;
    let timeout_raw = sub_matches
        .get_one::<String>("timeout-seconds")
        .ok_or_else(|| {
            output::report_error(
                format,
                "missing_argument",
                "Missing argument --timeout-seconds",
            )
        })?;

    let max_events = max_events_raw.parse::<usize>().map_err(|_| {
        output::report_error(
            format,
            "invalid_argument",
            "--max-events must be a positive integer",
        )
    })?;
    let timeout_seconds = timeout_raw.parse::<u64>().map_err(|_| {
        output::report_error(
            format,
            "invalid_argument",
            "--timeout-seconds must be a positive integer",
        )
    })?;
    let symbols = parse_symbol_list(symbols_raw);
    if symbols.is_empty() {
        return Err(output::report_error(
            format,
            "invalid_argument",
            "At least one symbol is required in --symbols",
        ));
    }
    let channels = parse_market_data_channels(channels_raw, format)?;

    Ok(MarketDataStreamRequest {
        symbols,
        channels_raw: channels_raw.clone(),
        channels,
        max_events,
        timeout_seconds,
    })
}

pub(in crate::dispatcher) fn parse_bar_timeframe(
    raw: &str,
    format: ReportOutputFormat,
) -> Result<BarTimeframe, CliError> {
    match raw {
        "1m" => Ok(BarTimeframe::OneMinute),
        "1h" => Ok(BarTimeframe::OneHour),
        "1d" => Ok(BarTimeframe::OneDay),
        _ => Err(output::report_error(
            format,
            "invalid_timeframe",
            format!("Unsupported timeframe '{raw}'. Allowed: 1m | 1h | 1d"),
        )),
    }
}

pub(in crate::dispatcher) fn parse_rfc3339_timestamp(
    sub_matches: &ArgMatches,
    key: &'static str,
    format: ReportOutputFormat,
) -> Result<DateTime<Utc>, CliError> {
    let raw = sub_matches.get_one::<String>(key).ok_or_else(|| {
        output::report_error(
            format,
            "missing_argument",
            format!("Missing argument --{key}"),
        )
    })?;

    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| {
            output::report_error(
                format,
                "invalid_timestamp",
                format!("Invalid RFC3339 timestamp for --{key}: {raw}"),
            )
        })
}

pub(in crate::dispatcher) fn market_data_snapshot(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --account")
    })?;
    let symbol_raw = sub_matches.get_one::<String>("symbol").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --symbol")
    })?;
    let symbol = symbol_raw.trim().to_uppercase();
    let account = context::resolve_account_arg(trust, account_raw, format)?;
    let requested_at = Utc::now();

    let snapshot = trust
        .market_snapshot_v2(&account, &symbol, requested_at)
        .map_err(|error| {
            output::report_error(
                format,
                "market_data_snapshot_failed",
                format!("Failed to retrieve market snapshot: {error}"),
            )
        })?;

    match format {
        ReportOutputFormat::Text => {
            println!("Market snapshot");
            println!("===============");
            println!("symbol: {}", snapshot.symbol);
            println!("as_of: {}", snapshot.as_of.to_rfc3339());
            println!("last_price: {}", crate::zen::amount(snapshot.last_price));
            println!("open: {}", crate::zen::amount(snapshot.open));
            println!("high: {}", crate::zen::amount(snapshot.high));
            println!("low: {}", crate::zen::amount(snapshot.low));
            println!("volume: {}", snapshot.volume);
            if let Some(quote) = &snapshot.quote {
                println!("bid: {}", crate::zen::amount(quote.bid_price));
                println!("ask: {}", crate::zen::amount(quote.ask_price));
            }
            if let Some(trade) = &snapshot.trade {
                println!("last_trade_price: {}", crate::zen::amount(trade.price));
                println!("last_trade_size: {}", trade.size);
            }
            let source = market_snapshot_source_label(&snapshot.source);
            println!("source: {source}");
        }
        ReportOutputFormat::Json => {
            let payload = market_output::market_snapshot_payload(&account, requested_at, &snapshot);
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn load_snapshot_for_symbol(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<
    (
        model::Account,
        String,
        DateTime<Utc>,
        model::MarketSnapshotV2,
    ),
    CliError,
> {
    let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --account")
    })?;
    let symbol_raw = sub_matches.get_one::<String>("symbol").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --symbol")
    })?;
    let symbol = symbol_raw.trim().to_uppercase();
    let account = context::resolve_account_arg(trust, account_raw, format)?;
    let requested_at = Utc::now();
    let snapshot = trust
        .market_snapshot_v2(&account, &symbol, requested_at)
        .map_err(|error| {
            output::report_error(
                format,
                "market_data_snapshot_failed",
                format!("Failed to retrieve market snapshot: {error}"),
            )
        })?;
    Ok((account, symbol, requested_at, snapshot))
}

pub(in crate::dispatcher) fn market_data_quote(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let (account, symbol, requested_at, snapshot) =
        load_snapshot_for_symbol(trust, sub_matches, format)?;
    let quote = snapshot.quote.ok_or_else(|| {
        output::report_error(
            format,
            "market_data_quote_unavailable",
            format!("No quote available for symbol '{symbol}'"),
        )
    })?;

    match format {
        ReportOutputFormat::Text => {
            println!("Market quote");
            println!("============");
            println!("symbol: {symbol}");
            println!("as_of: {}", quote.as_of.to_rfc3339());
            println!("bid_price: {}", crate::zen::amount(quote.bid_price));
            println!("bid_size: {}", quote.bid_size);
            println!("ask_price: {}", crate::zen::amount(quote.ask_price));
            println!("ask_size: {}", quote.ask_size);
        }
        ReportOutputFormat::Json => {
            let lag_seconds = requested_at
                .signed_duration_since(quote.as_of)
                .num_seconds()
                .max(0);
            let payload = json!({
                "report": "market_data_quote",
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
                "freshness": {
                    "as_of": quote.as_of.to_rfc3339(),
                    "lag_seconds": lag_seconds,
                },
                "data": {
                    "bid_price": output::decimal_string(quote.bid_price),
                    "bid_size": quote.bid_size,
                    "ask_price": output::decimal_string(quote.ask_price),
                    "ask_size": quote.ask_size,
                }
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn market_data_trade(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let (account, symbol, requested_at, snapshot) =
        load_snapshot_for_symbol(trust, sub_matches, format)?;
    let trade = snapshot.trade.ok_or_else(|| {
        output::report_error(
            format,
            "market_data_trade_unavailable",
            format!("No trade tick available for symbol '{symbol}'"),
        )
    })?;

    match format {
        ReportOutputFormat::Text => {
            println!("Market trade");
            println!("============");
            println!("symbol: {symbol}");
            println!("as_of: {}", trade.as_of.to_rfc3339());
            println!("price: {}", crate::zen::amount(trade.price));
            println!("size: {}", trade.size);
        }
        ReportOutputFormat::Json => {
            let lag_seconds = requested_at
                .signed_duration_since(trade.as_of)
                .num_seconds()
                .max(0);
            let payload = json!({
                "report": "market_data_trade",
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
                "freshness": {
                    "as_of": trade.as_of.to_rfc3339(),
                    "lag_seconds": lag_seconds,
                },
                "data": {
                    "price": output::decimal_string(trade.price),
                    "size": trade.size,
                }
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn market_data_session(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let (account, symbol, requested_at, snapshot) =
        load_snapshot_for_symbol(trust, sub_matches, format)?;
    let lag_seconds = requested_at
        .signed_duration_since(snapshot.as_of)
        .num_seconds()
        .max(0);
    let source_kind = market_snapshot_source_label(&snapshot.source);

    match format {
        ReportOutputFormat::Text => {
            println!("Market session");
            println!("==============");
            println!("symbol: {symbol}");
            println!("as_of: {}", snapshot.as_of.to_rfc3339());
            println!("source_kind: {source_kind}");
            println!("staleness_seconds: {lag_seconds}");
            println!("session_state: unknown");
        }
        ReportOutputFormat::Json => {
            let payload = json!({
                "report": "market_data_session",
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
                "data": {
                    "as_of": snapshot.as_of.to_rfc3339(),
                    "source_kind": source_kind,
                    "staleness_seconds": lag_seconds,
                    "session_state": "unknown",
                }
            });
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn parse_market_data_channels(
    raw: &str,
    format: ReportOutputFormat,
) -> Result<Vec<MarketDataChannel>, CliError> {
    let mut channels: Vec<MarketDataChannel> = Vec::new();
    for token in raw
        .split(',')
        .map(|value| value.trim().to_ascii_lowercase())
    {
        if token.is_empty() {
            continue;
        }
        let channel = match token.as_str() {
            "bars" => MarketDataChannel::Bars,
            "quotes" => MarketDataChannel::Quotes,
            "trades" => MarketDataChannel::Trades,
            _ => {
                return Err(output::report_error(
                    format,
                    "invalid_channel",
                    format!("Unsupported channel '{token}'. Allowed: bars,quotes,trades"),
                ))
            }
        };
        if !channels.contains(&channel) {
            channels.push(channel);
        }
    }
    if channels.is_empty() {
        return Err(output::report_error(
            format,
            "invalid_channel",
            "At least one channel is required (--channels bars,quotes,trades)",
        ));
    }
    Ok(channels)
}

pub(in crate::dispatcher) fn market_data_stream(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --account")
    })?;
    let request = parse_market_data_stream_request(sub_matches, format)?;
    let account = context::resolve_account_arg(trust, account_raw, format)?;
    let requested_at = Utc::now();
    let events = trust
        .stream_market_data(
            &account,
            &request.symbols,
            &request.channels,
            request.max_events,
            request.timeout_seconds,
        )
        .map_err(|error| {
            output::report_error(
                format,
                "market_data_stream_failed",
                format!("Failed to stream market data: {error}"),
            )
        })?;

    match format {
        ReportOutputFormat::Text => {
            println!("Market data stream");
            println!("==================");
            println!("symbols: {}", request.symbols.join(","));
            println!("events: {}", events.len());
            for event in &events {
                let channel = market_data_channel_text_label(event.channel);
                println!(
                    "{} {} {} price={} size={}",
                    event.as_of.to_rfc3339(),
                    channel,
                    event.symbol,
                    crate::zen::amount(event.price),
                    event.size
                );
            }
        }
        ReportOutputFormat::Json => {
            let payload = market_output::market_data_stream_payload(
                &account,
                requested_at,
                &request,
                &events,
            );
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn market_data_bars(
    trust: &mut TrustFacade,
    sub_matches: &ArgMatches,
    format: ReportOutputFormat,
) -> Result<(), CliError> {
    let account_raw = sub_matches.get_one::<String>("account").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --account")
    })?;
    let symbol_raw = sub_matches.get_one::<String>("symbol").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --symbol")
    })?;
    let timeframe_raw = sub_matches.get_one::<String>("timeframe").ok_or_else(|| {
        output::report_error(format, "missing_argument", "Missing argument --timeframe")
    })?;

    let start = parse_rfc3339_timestamp(sub_matches, "start", format)?;
    let end = parse_rfc3339_timestamp(sub_matches, "end", format)?;
    if end < start {
        return Err(output::report_error(
            format,
            "invalid_time_range",
            "--end must be greater than or equal to --start",
        ));
    }

    let symbol = symbol_raw.trim().to_uppercase();
    let account = context::resolve_account_arg(trust, account_raw, format)?;
    let timeframe = parse_bar_timeframe(timeframe_raw, format)?;
    let requested_at = Utc::now();

    let bars = trust
        .market_bars(&account, &symbol, start, end, timeframe)
        .map_err(|error| {
            output::report_error(
                format,
                "market_data_bars_failed",
                format!("Failed to retrieve market bars: {error}"),
            )
        })?;

    match format {
        ReportOutputFormat::Text => {
            println!("Market bars");
            println!("===========");
            println!("symbol: {symbol}");
            println!("timeframe: {timeframe_raw}");
            println!("start: {}", start.to_rfc3339());
            println!("end: {}", end.to_rfc3339());
            println!("count: {}", bars.len());
            for bar in bars.iter().take(10) {
                println!(
                    "{} o={} h={} l={} c={} v={}",
                    bar.time.to_rfc3339(),
                    crate::zen::amount(bar.open),
                    crate::zen::amount(bar.high),
                    crate::zen::amount(bar.low),
                    crate::zen::amount(bar.close),
                    bar.volume
                );
            }
            if bars.len() > 10 {
                println!("... ({} more bars omitted)", bars.len().saturating_sub(10));
            }
        }
        ReportOutputFormat::Json => {
            let payload = market_output::market_data_bars_payload(
                &account,
                requested_at,
                &symbol,
                timeframe_raw,
                start,
                end,
                &bars,
            );
            output::print_json(&payload)?;
        }
    }

    Ok(())
}

pub(in crate::dispatcher) fn market_data_handler_for(
    subcommand: MarketDataSubcommand<'_>,
) -> (output::FormattedCommand, &ArgMatches) {
    match subcommand {
        MarketDataSubcommand::Snapshot(args) => (market_data_snapshot, args),
        MarketDataSubcommand::Bars(args) => (market_data_bars, args),
        MarketDataSubcommand::Stream(args) => (market_data_stream, args),
        MarketDataSubcommand::Quote(args) => (market_data_quote, args),
        MarketDataSubcommand::Trade(args) => (market_data_trade, args),
        MarketDataSubcommand::Session(args) => (market_data_session, args),
    }
}

/// GoF Command: this group owns its workflow and uses the facade as receiver.
pub(super) struct MarketDataCommand;
impl super::Command for MarketDataCommand {
    fn execute(&self, trust: &mut TrustFacade, args: &ArgMatches) -> Result<(), CliError> {
        dispatch_market_data(trust, args)
    }
}
