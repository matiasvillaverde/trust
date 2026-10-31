#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::TimeZone;
use chrono::Utc;
use clap::Arg;
use clap::Command;
use model::MarketDataChannel;
use model::MarketDataStreamEvent;
use model::MarketSnapshotSource;
use rust_decimal_macros::dec;

#[test]
fn test_parse_bar_timeframe_valid_values() {
    assert!(matches!(
        crate::dispatcher::market_data::parse_bar_timeframe("1m", ReportOutputFormat::Json)
            .unwrap(),
        model::BarTimeframe::OneMinute
    ));
    assert!(matches!(
        crate::dispatcher::market_data::parse_bar_timeframe("1h", ReportOutputFormat::Json)
            .unwrap(),
        model::BarTimeframe::OneHour
    ));
    assert!(matches!(
        crate::dispatcher::market_data::parse_bar_timeframe("1d", ReportOutputFormat::Json)
            .unwrap(),
        model::BarTimeframe::OneDay
    ));
}

#[test]
fn test_parse_bar_timeframe_invalid_value() {
    let error = crate::dispatcher::market_data::parse_bar_timeframe("5m", ReportOutputFormat::Json)
        .unwrap_err();
    assert!(error.to_string().contains("invalid_timeframe"));
}

#[test]
fn test_parse_market_data_channels_valid_and_invalid_values() {
    let channels = crate::dispatcher::market_data::parse_market_data_channels(
        "quotes,trades,bars,quotes",
        ReportOutputFormat::Json,
    )
    .expect("channels should parse");
    assert_eq!(channels.len(), 3);
    assert!(channels.contains(&model::MarketDataChannel::Quotes));
    assert!(channels.contains(&model::MarketDataChannel::Trades));
    assert!(channels.contains(&model::MarketDataChannel::Bars));

    let error =
        crate::dispatcher::market_data::parse_market_data_channels("foo", ReportOutputFormat::Json)
            .expect_err("invalid channel should fail");
    assert!(error.to_string().contains("invalid_channel"));
}

#[test]
fn test_parse_market_data_stream_request_happy_path() {
    let matches = market_stream_matches(&[
        "--symbols",
        " aapl, msft ",
        "--channels",
        "quotes,trades,quotes",
        "--max-events",
        "12",
        "--timeout-seconds",
        "7",
    ]);

    let request = crate::dispatcher::market_data::parse_market_data_stream_request(
        &matches,
        ReportOutputFormat::Json,
    )
    .expect("request should parse");

    assert_eq!(
        request.symbols,
        vec!["AAPL".to_string(), "MSFT".to_string()]
    );
    assert_eq!(request.max_events, 12);
    assert_eq!(request.timeout_seconds, 7);
    assert_eq!(
        request.channels,
        vec![MarketDataChannel::Quotes, MarketDataChannel::Trades]
    );
    assert_eq!(request.channels_raw, "quotes,trades,quotes");
}

#[test]
fn test_parse_market_data_stream_request_rejects_missing_or_invalid_arguments() {
    let missing_symbols = market_stream_matches(&[
        "--channels",
        "quotes",
        "--max-events",
        "12",
        "--timeout-seconds",
        "7",
    ]);
    let error = crate::dispatcher::market_data::parse_market_data_stream_request(
        &missing_symbols,
        ReportOutputFormat::Json,
    )
    .expect_err("missing symbols should fail");
    assert!(error.to_string().contains("missing_argument"));

    let missing_channels = market_stream_matches(&[
        "--symbols",
        "AAPL",
        "--max-events",
        "12",
        "--timeout-seconds",
        "7",
    ]);
    let error = crate::dispatcher::market_data::parse_market_data_stream_request(
        &missing_channels,
        ReportOutputFormat::Json,
    )
    .expect_err("missing channels should fail");
    assert!(error.to_string().contains("missing_argument"));

    let missing_max_events = market_stream_matches(&[
        "--symbols",
        "AAPL",
        "--channels",
        "quotes",
        "--timeout-seconds",
        "7",
    ]);
    let error = crate::dispatcher::market_data::parse_market_data_stream_request(
        &missing_max_events,
        ReportOutputFormat::Json,
    )
    .expect_err("missing max-events should fail");
    assert!(error.to_string().contains("missing_argument"));

    let missing_timeout = market_stream_matches(&[
        "--symbols",
        "AAPL",
        "--channels",
        "quotes",
        "--max-events",
        "12",
    ]);
    let error = crate::dispatcher::market_data::parse_market_data_stream_request(
        &missing_timeout,
        ReportOutputFormat::Json,
    )
    .expect_err("missing timeout should fail");
    assert!(error.to_string().contains("missing_argument"));

    let invalid_max = market_stream_matches(&[
        "--symbols",
        "AAPL",
        "--channels",
        "quotes",
        "--max-events",
        "not-int",
        "--timeout-seconds",
        "7",
    ]);
    let error = crate::dispatcher::market_data::parse_market_data_stream_request(
        &invalid_max,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid max-events should fail");
    assert!(error.to_string().contains("invalid_argument"));

    let invalid_timeout = market_stream_matches(&[
        "--symbols",
        "AAPL",
        "--channels",
        "quotes",
        "--max-events",
        "10",
        "--timeout-seconds",
        "soon",
    ]);
    let error = crate::dispatcher::market_data::parse_market_data_stream_request(
        &invalid_timeout,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid timeout should fail");
    assert!(error.to_string().contains("invalid_argument"));

    let invalid_symbols = market_stream_matches(&[
        "--symbols",
        ", ,",
        "--channels",
        "quotes",
        "--max-events",
        "10",
        "--timeout-seconds",
        "7",
    ]);
    let error = crate::dispatcher::market_data::parse_market_data_stream_request(
        &invalid_symbols,
        ReportOutputFormat::Json,
    )
    .expect_err("empty symbols should fail");
    assert!(error
        .to_string()
        .contains("At least one symbol is required in --symbols"));
}

#[test]
fn test_market_data_handler_factory_routes_to_expected_handler_behavior() {
    let mut dispatcher = test_dispatcher();

    let snapshot =
        market_data_command_matches(&["snapshot", "--account", "not-a-uuid", "--symbol", "AAPL"]);
    let (handler, sub) = crate::dispatcher::market_data::market_data_handler_for(
        crate::command_routing::parse_market_data_subcommand(&snapshot),
    );
    let error = handler(&mut dispatcher.trust, sub, ReportOutputFormat::Json)
        .expect_err("invalid account should fail");
    assert!(error.to_string().contains("account_not_found"));

    let bars = market_data_command_matches(&[
        "bars",
        "--account",
        "not-a-uuid",
        "--symbol",
        "AAPL",
        "--timeframe",
        "1m",
        "--start",
        "2026-02-24T10:00:00Z",
        "--end",
        "2026-02-24T11:00:00Z",
    ]);
    let (handler, sub) = crate::dispatcher::market_data::market_data_handler_for(
        crate::command_routing::parse_market_data_subcommand(&bars),
    );
    let error = handler(&mut dispatcher.trust, sub, ReportOutputFormat::Json)
        .expect_err("invalid account should fail");
    assert!(error.to_string().contains("account_not_found"));

    let stream = market_data_command_matches(&[
        "stream",
        "--account",
        "not-a-uuid",
        "--symbols",
        "AAPL",
        "--channels",
        "quotes",
        "--max-events",
        "1",
        "--timeout-seconds",
        "1",
    ]);
    let (handler, sub) = crate::dispatcher::market_data::market_data_handler_for(
        crate::command_routing::parse_market_data_subcommand(&stream),
    );
    let error = handler(&mut dispatcher.trust, sub, ReportOutputFormat::Json)
        .expect_err("invalid account should fail");
    assert!(error.to_string().contains("account_not_found"));

    let quote =
        market_data_command_matches(&["quote", "--account", "not-a-uuid", "--symbol", "AAPL"]);
    let (handler, sub) = crate::dispatcher::market_data::market_data_handler_for(
        crate::command_routing::parse_market_data_subcommand(&quote),
    );
    let error = handler(&mut dispatcher.trust, sub, ReportOutputFormat::Json)
        .expect_err("invalid account should fail");
    assert!(error.to_string().contains("account_not_found"));

    let trade =
        market_data_command_matches(&["trade", "--account", "not-a-uuid", "--symbol", "AAPL"]);
    let (handler, sub) = crate::dispatcher::market_data::market_data_handler_for(
        crate::command_routing::parse_market_data_subcommand(&trade),
    );
    let error = handler(&mut dispatcher.trust, sub, ReportOutputFormat::Json)
        .expect_err("invalid account should fail");
    assert!(error.to_string().contains("account_not_found"));

    let session =
        market_data_command_matches(&["session", "--account", "not-a-uuid", "--symbol", "AAPL"]);
    let (handler, sub) = crate::dispatcher::market_data::market_data_handler_for(
        crate::command_routing::parse_market_data_subcommand(&session),
    );
    let error = handler(&mut dispatcher.trust, sub, ReportOutputFormat::Json)
        .expect_err("invalid account should fail");
    assert!(error.to_string().contains("account_not_found"));
}

#[test]
fn test_market_data_dispatch_validates_missing_required_arguments() {
    let mut dispatcher = test_dispatcher();
    let market_matches = |args: &[&str]| {
        let mut argv = vec!["test"];
        argv.extend_from_slice(args);
        Command::new("test")
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("symbol").long("symbol"))
            .arg(Arg::new("symbols").long("symbols"))
            .arg(Arg::new("channels").long("channels"))
            .arg(Arg::new("max-events").long("max-events"))
            .arg(Arg::new("timeout-seconds").long("timeout-seconds"))
            .arg(Arg::new("timeframe").long("timeframe"))
            .arg(Arg::new("start").long("start"))
            .arg(Arg::new("end").long("end"))
            .get_matches_from(argv)
    };

    let snapshot_missing_account = market_matches(&["--symbol", "AAPL"]);
    let error = crate::dispatcher::market_data::market_data_snapshot(
        &mut dispatcher.trust,
        &snapshot_missing_account,
        ReportOutputFormat::Json,
    )
    .expect_err("snapshot without account should fail");
    assert!(error.to_string().contains("missing_argument"));

    let snapshot_missing_symbol = market_matches(&["--account", "paper-main"]);
    let error = crate::dispatcher::market_data::market_data_snapshot(
        &mut dispatcher.trust,
        &snapshot_missing_symbol,
        ReportOutputFormat::Json,
    )
    .expect_err("snapshot without symbol should fail");
    assert!(error.to_string().contains("missing_argument"));

    let quote_missing_account = market_matches(&["--symbol", "AAPL"]);
    let error = crate::dispatcher::market_data::market_data_quote(
        &mut dispatcher.trust,
        &quote_missing_account,
        ReportOutputFormat::Json,
    )
    .expect_err("quote without account should fail");
    assert!(error.to_string().contains("missing_argument"));

    let trade_missing_symbol = market_matches(&["--account", "paper-main"]);
    let error = crate::dispatcher::market_data::market_data_trade(
        &mut dispatcher.trust,
        &trade_missing_symbol,
        ReportOutputFormat::Json,
    )
    .expect_err("trade without symbol should fail");
    assert!(error.to_string().contains("missing_argument"));

    let stream_missing_account = market_matches(&[
        "--symbols",
        "AAPL",
        "--channels",
        "quotes",
        "--max-events",
        "1",
        "--timeout-seconds",
        "1",
    ]);
    let error = crate::dispatcher::market_data::market_data_stream(
        &mut dispatcher.trust,
        &stream_missing_account,
        ReportOutputFormat::Json,
    )
    .expect_err("stream without account should fail");
    assert!(error.to_string().contains("missing_argument"));

    let bars_missing_account = market_matches(&[
        "--symbol",
        "AAPL",
        "--timeframe",
        "1m",
        "--start",
        "2026-02-24T09:00:00Z",
        "--end",
        "2026-02-24T10:00:00Z",
    ]);
    let error = crate::dispatcher::market_data::market_data_bars(
        &mut dispatcher.trust,
        &bars_missing_account,
        ReportOutputFormat::Json,
    )
    .expect_err("bars without account should fail");
    assert!(error.to_string().contains("missing_argument"));

    let bars_missing_symbol = market_matches(&[
        "--account",
        "paper-main",
        "--timeframe",
        "1m",
        "--start",
        "2026-02-24T09:00:00Z",
        "--end",
        "2026-02-24T10:00:00Z",
    ]);
    let error = crate::dispatcher::market_data::market_data_bars(
        &mut dispatcher.trust,
        &bars_missing_symbol,
        ReportOutputFormat::Json,
    )
    .expect_err("bars without symbol should fail");
    assert!(error.to_string().contains("missing_argument"));

    let bars_missing_timeframe = market_matches(&[
        "--account",
        "paper-main",
        "--symbol",
        "AAPL",
        "--start",
        "2026-02-24T09:00:00Z",
        "--end",
        "2026-02-24T10:00:00Z",
    ]);
    let error = crate::dispatcher::market_data::market_data_bars(
        &mut dispatcher.trust,
        &bars_missing_timeframe,
        ReportOutputFormat::Json,
    )
    .expect_err("bars without timeframe should fail");
    assert!(error.to_string().contains("missing_argument"));

    let bars_reversed_range = market_matches(&[
        "--account",
        "paper-main",
        "--symbol",
        "AAPL",
        "--timeframe",
        "1m",
        "--start",
        "2026-02-24T10:00:00Z",
        "--end",
        "2026-02-24T09:00:00Z",
    ]);
    let error = crate::dispatcher::market_data::market_data_bars(
        &mut dispatcher.trust,
        &bars_reversed_range,
        ReportOutputFormat::Json,
    )
    .expect_err("reversed bars range should fail before account lookup");
    assert!(error.to_string().contains("invalid_time_range"));
}

#[test]
fn test_market_data_stream_payload_normalizes_channels_and_counts() {
    let requested_at = Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 0).unwrap();
    let request = crate::dispatcher::market_data::MarketDataStreamRequest {
        symbols: vec!["AAPL".to_string()],
        channels_raw: "quotes,trades".to_string(),
        channels: vec![MarketDataChannel::Quotes, MarketDataChannel::Trades],
        max_events: 2,
        timeout_seconds: 3,
    };
    let events = vec![
        MarketDataStreamEvent {
            channel: MarketDataChannel::Quotes,
            symbol: "AAPL".to_string(),
            as_of: Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 1).unwrap(),
            price: dec!(201.05),
            size: 2,
        },
        MarketDataStreamEvent {
            channel: MarketDataChannel::Trades,
            symbol: "AAPL".to_string(),
            as_of: Utc.with_ymd_and_hms(2026, 2, 27, 10, 0, 2).unwrap(),
            price: dec!(201.1),
            size: 6,
        },
    ];

    let payload = crate::dispatcher::market_output::market_data_stream_payload(
        &model::Account::default(),
        requested_at,
        &request,
        &events,
    );
    assert_eq!(payload["report"], "market_data_stream");
    assert_eq!(payload["scope"]["symbols"][0], "AAPL");
    assert_eq!(payload["request"]["max_events"], 2);
    assert_eq!(payload["data"]["count"], 2);
    assert_eq!(payload["data"]["events"][0]["channel"], "quotes");
    assert_eq!(payload["data"]["events"][1]["channel"], "trades");
}

#[test]
fn test_market_data_label_helpers_cover_all_sources_and_channels() {
    assert_eq!(
        crate::dispatcher::market_data::market_snapshot_source_label(
            &MarketSnapshotSource::QuoteTrade
        ),
        "quote_trade"
    );
    assert_eq!(
        crate::dispatcher::market_data::market_snapshot_source_label(
            &MarketSnapshotSource::BarsFallback
        ),
        "bars_fallback"
    );
    assert_eq!(
        crate::dispatcher::market_data::market_data_channel_text_label(MarketDataChannel::Bars),
        "bar"
    );
    assert_eq!(
        crate::dispatcher::market_data::market_data_channel_text_label(MarketDataChannel::Quotes),
        "quote"
    );
    assert_eq!(
        crate::dispatcher::market_data::market_data_channel_text_label(MarketDataChannel::Trades),
        "trade"
    );
    assert_eq!(
        crate::dispatcher::market_data::market_data_channel_json_label(MarketDataChannel::Bars),
        "bars"
    );
    assert_eq!(
        crate::dispatcher::market_data::market_data_channel_json_label(MarketDataChannel::Quotes),
        "quotes"
    );
    assert_eq!(
        crate::dispatcher::market_data::market_data_channel_json_label(MarketDataChannel::Trades),
        "trades"
    );
}

#[test]
fn test_parse_rfc3339_timestamp() {
    let matches = Command::new("test")
        .arg(Arg::new("start").long("start").required(true))
        .get_matches_from(["test", "--start", "2026-02-24T10:00:00Z"]);
    let parsed = crate::dispatcher::market_data::parse_rfc3339_timestamp(
        &matches,
        "start",
        ReportOutputFormat::Json,
    )
    .unwrap();
    assert_eq!(parsed, Utc.with_ymd_and_hms(2026, 2, 24, 10, 0, 0).unwrap());
}

#[test]
fn test_parse_rfc3339_timestamp_invalid() {
    let matches = Command::new("test")
        .arg(Arg::new("start").long("start").required(true))
        .get_matches_from(["test", "--start", "not-a-time"]);
    let error = crate::dispatcher::market_data::parse_rfc3339_timestamp(
        &matches,
        "start",
        ReportOutputFormat::Json,
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid_timestamp"));
}

#[test]
fn test_parse_rfc3339_timestamp_missing_argument() {
    let matches = Command::new("test")
        .arg(Arg::new("start").long("start"))
        .get_matches_from(["test"]);
    let error = crate::dispatcher::market_data::parse_rfc3339_timestamp(
        &matches,
        "start",
        ReportOutputFormat::Json,
    )
    .expect_err("missing argument should fail");
    assert!(error.to_string().contains("missing_argument"));
    assert!(error.to_string().contains("--start"));
    assert!(error.already_printed());
}

#[test]
fn test_parse_market_data_channels_deduplicates_and_rejects_empty_input() {
    let parsed = crate::dispatcher::market_data::parse_market_data_channels(
        " quotes, bars,quotes ,trades,bars ",
        ReportOutputFormat::Json,
    )
    .expect("channels parse");
    assert_eq!(
        parsed,
        vec![
            MarketDataChannel::Quotes,
            MarketDataChannel::Bars,
            MarketDataChannel::Trades
        ]
    );

    let error = crate::dispatcher::market_data::parse_market_data_channels(
        " , , ",
        ReportOutputFormat::Json,
    )
    .expect_err("empty channels must fail");
    assert!(error.to_string().contains("invalid_channel"));
    assert!(error
        .to_string()
        .contains("At least one channel is required"));
}
