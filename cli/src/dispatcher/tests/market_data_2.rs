#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::broker::*;
use super::fixtures::*;
use chrono::TimeZone;
use chrono::Utc;
use model::MarketBar;
use rust_decimal_macros::dec;

#[test]
fn test_market_data_dispatch_paths_succeed_with_stub_broker() {
    let mut dispatcher = test_dispatcher_with_broker(Box::new(StubBroker::quote_trade_fixture()));
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);
    let account_id = account.id.to_string();

    let snapshot = market_data_command_matches(&[
        "snapshot",
        "--account",
        &account_id,
        "--symbol",
        "aapl",
        "--format",
        "json",
    ]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &snapshot)
        .expect("snapshot dispatch should succeed");
    let snapshot_text =
        market_data_command_matches(&["snapshot", "--account", &account_id, "--symbol", "aapl"]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &snapshot_text)
        .expect("text snapshot dispatch should succeed");

    let quote = market_data_command_matches(&[
        "quote",
        "--account",
        &account_id,
        "--symbol",
        "AAPL",
        "--format",
        "json",
    ]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &quote)
        .expect("quote dispatch should succeed");
    let quote_text =
        market_data_command_matches(&["quote", "--account", &account_id, "--symbol", "AAPL"]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &quote_text)
        .expect("text quote dispatch should succeed");

    let trade = market_data_command_matches(&[
        "trade",
        "--account",
        &account_id,
        "--symbol",
        "AAPL",
        "--format",
        "json",
    ]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &trade)
        .expect("trade dispatch should succeed");
    let trade_text =
        market_data_command_matches(&["trade", "--account", &account_id, "--symbol", "AAPL"]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &trade_text)
        .expect("text trade dispatch should succeed");

    let session = market_data_command_matches(&[
        "session",
        "--account",
        &account_id,
        "--symbol",
        "AAPL",
        "--format",
        "json",
    ]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &session)
        .expect("session dispatch should succeed");
    let session_text =
        market_data_command_matches(&["session", "--account", &account_id, "--symbol", "AAPL"]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &session_text)
        .expect("text session dispatch should succeed");

    let stream = market_data_command_matches(&[
        "stream",
        "--account",
        &account_id,
        "--symbols",
        "AAPL,MSFT",
        "--channels",
        "quotes,trades",
        "--max-events",
        "5",
        "--timeout-seconds",
        "2",
        "--format",
        "json",
    ]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &stream)
        .expect("stream dispatch should succeed");
    let stream_text = market_data_command_matches(&[
        "stream",
        "--account",
        &account_id,
        "--symbols",
        "AAPL,MSFT",
        "--channels",
        "quotes,trades",
        "--max-events",
        "5",
        "--timeout-seconds",
        "2",
    ]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &stream_text)
        .expect("text stream dispatch should succeed");

    let bars = market_data_command_matches(&[
        "bars",
        "--account",
        &account_id,
        "--symbol",
        "AAPL",
        "--timeframe",
        "1m",
        "--start",
        "2026-02-24T09:00:00Z",
        "--end",
        "2026-02-24T11:00:00Z",
        "--format",
        "json",
    ]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &bars)
        .expect("bars dispatch should succeed");
    let bars_text = market_data_command_matches(&[
        "bars",
        "--account",
        &account_id,
        "--symbol",
        "AAPL",
        "--timeframe",
        "1m",
        "--start",
        "2026-02-24T09:00:00Z",
        "--end",
        "2026-02-24T11:00:00Z",
    ]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &bars_text)
        .expect("text bars dispatch should succeed");
}

#[test]
fn test_market_data_failure_paths_and_long_bars_are_reported() {
    let mut snapshot_failure_broker = StubBroker::quote_trade_fixture();
    snapshot_failure_broker.quote = None;
    snapshot_failure_broker.trade = None;
    snapshot_failure_broker.fail_bars = true;
    let mut dispatcher = test_dispatcher_with_broker(Box::new(snapshot_failure_broker));
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);
    let account_id = account.id.to_string();
    let quote = market_data_command_matches(&[
        "quote",
        "--account",
        &account_id,
        "--symbol",
        "AAPL",
        "--format",
        "json",
    ]);
    let error = crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &quote)
        .expect_err("snapshot failure should fail quote handler");
    assert!(error.to_string().contains("market_data_snapshot_failed"));

    let mut stream_failure_broker = StubBroker::quote_trade_fixture();
    stream_failure_broker.fail_stream = true;
    let mut dispatcher = test_dispatcher_with_broker(Box::new(stream_failure_broker));
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);
    let account_id = account.id.to_string();
    let stream = market_data_command_matches(&[
        "stream",
        "--account",
        &account_id,
        "--symbols",
        "AAPL",
        "--channels",
        "quotes",
        "--max-events",
        "1",
        "--timeout-seconds",
        "1",
        "--format",
        "json",
    ]);
    let error =
        crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &stream)
            .expect_err("stream broker failure should fail");
    assert!(error.to_string().contains("market_data_stream_failed"));

    let mut bars_failure_broker = StubBroker::quote_trade_fixture();
    bars_failure_broker.fail_bars = true;
    let mut dispatcher = test_dispatcher_with_broker(Box::new(bars_failure_broker));
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);
    let account_id = account.id.to_string();
    let bars = market_data_command_matches(&[
        "bars",
        "--account",
        &account_id,
        "--symbol",
        "AAPL",
        "--timeframe",
        "1m",
        "--start",
        "2026-02-24T09:00:00Z",
        "--end",
        "2026-02-24T11:00:00Z",
        "--format",
        "json",
    ]);
    let error = crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &bars)
        .expect_err("bars broker failure should fail");
    assert!(error.to_string().contains("market_data_bars_failed"));

    let mut long_bars_broker = StubBroker::quote_trade_fixture();
    let start = Utc
        .with_ymd_and_hms(2026, 2, 24, 9, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    long_bars_broker.bars = (0_u32..11)
        .map(|offset| MarketBar {
            time: start + chrono::Duration::minutes(i64::from(offset)),
            open: dec!(200),
            high: dec!(202),
            low: dec!(199),
            close: dec!(201),
            volume: 1_000 + u64::from(offset),
        })
        .collect();
    let mut dispatcher = test_dispatcher_with_broker(Box::new(long_bars_broker));
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);
    let account_id = account.id.to_string();
    let bars_text = market_data_command_matches(&[
        "bars",
        "--account",
        &account_id,
        "--symbol",
        "AAPL",
        "--timeframe",
        "1m",
        "--start",
        "2026-02-24T09:00:00Z",
        "--end",
        "2026-02-24T11:00:00Z",
    ]);
    crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &bars_text)
        .expect("long text bars should be truncated after preview");
}

#[test]
fn test_market_data_quote_and_trade_error_when_snapshot_lacks_fields() {
    let mut stub = StubBroker::quote_trade_fixture();
    stub.quote = None;
    stub.trade = None;
    let mut dispatcher = test_dispatcher_with_broker(Box::new(stub));
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);
    let account_id = account.id.to_string();

    let quote = market_data_command_matches(&[
        "quote",
        "--account",
        &account_id,
        "--symbol",
        "AAPL",
        "--format",
        "json",
    ]);
    let quote_error =
        crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &quote)
            .expect_err("quote should fail when snapshot lacks quote");
    assert!(quote_error
        .to_string()
        .contains("market_data_quote_unavailable"));

    let trade = market_data_command_matches(&[
        "trade",
        "--account",
        &account_id,
        "--symbol",
        "AAPL",
        "--format",
        "json",
    ]);
    let trade_error =
        crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &trade)
            .expect_err("trade should fail when snapshot lacks trade");
    assert!(trade_error
        .to_string()
        .contains("market_data_trade_unavailable"));
}
