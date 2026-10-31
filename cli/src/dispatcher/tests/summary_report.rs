#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::broker::*;
use super::fixtures::*;
use crate::commands::ReportCommandBuilder;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::Utc;
use clap::Arg;
use clap::Command;
use model::Currency;
use model::Environment;
use model::Status;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn test_report_handlers_cover_non_empty_and_zero_branches() {
    let mut dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (account, trade) = seed_new_trade(&mut dispatcher);
    let (funded, _, _, _) = dispatcher.trust.fund_trade(&trade).expect("fund trade");
    let (submitted, _) = dispatcher
        .trust
        .submit_trade(&funded)
        .expect("submit trade");
    let mut fill_ready = submitted.clone();
    fill_ready.entry.status = model::OrderStatus::Filled;
    fill_ready.entry.filled_quantity = fill_ready.entry.quantity;
    fill_ready.entry.average_filled_price = Some(fill_ready.entry.unit_price);
    fill_ready.entry.filled_at = Some(Utc::now().naive_utc());
    let (filled, _) = dispatcher
        .trust
        .fill_trade(&fill_ready, dec!(1.25))
        .expect("fill trade with opening fee");
    let mut target_ready = filled.clone();
    target_ready.target.status = model::OrderStatus::Filled;
    target_ready.target.filled_quantity = target_ready.target.quantity;
    target_ready.target.average_filled_price = Some(dec!(110));
    target_ready.target.filled_at = Some(Utc::now().naive_utc());
    dispatcher
        .trust
        .target_acquired(&target_ready, dec!(0.75))
        .expect("target close with closing fee");
    let loss_trade = dispatcher
        .trust
        .create_trade(
            model::DraftTrade {
                account: account.clone(),
                trading_vehicle: trade.trading_vehicle.clone(),
                quantity: 1.into(),
                category: model::TradeCategory::Long,
                currency: Currency::USD,
                thesis: Some("loss sample".to_string()),
                sector: Some("technology".to_string()),
                asset_class: Some("equity".to_string()),
                context: None,
            },
            dec!(95),
            dec!(100),
            dec!(110),
        )
        .expect("loss sample trade should be created");
    let (loss_funded, _, _, _) = dispatcher
        .trust
        .fund_trade(&loss_trade)
        .expect("loss sample trade should be funded");
    let (loss_submitted, _) = dispatcher
        .trust
        .submit_trade(&loss_funded)
        .expect("loss sample trade should be submitted");
    let mut loss_fill_ready = loss_submitted.clone();
    loss_fill_ready.entry.status = model::OrderStatus::Filled;
    loss_fill_ready.entry.filled_quantity = loss_fill_ready.entry.quantity;
    loss_fill_ready.entry.average_filled_price = Some(loss_fill_ready.entry.unit_price);
    loss_fill_ready.entry.filled_at = Some(Utc::now().naive_utc());
    let (loss_filled, _) = dispatcher
        .trust
        .fill_trade(&loss_fill_ready, Decimal::ZERO)
        .expect("loss sample trade should be filled");
    let mut stop_ready = loss_filled.clone();
    stop_ready.safety_stop.status = model::OrderStatus::Filled;
    stop_ready.safety_stop.filled_quantity = stop_ready.safety_stop.quantity;
    stop_ready.safety_stop.average_filled_price = Some(stop_ready.safety_stop.unit_price);
    stop_ready.safety_stop.filled_at = Some(Utc::now().naive_utc());
    dispatcher
        .trust
        .stop_trade(&stop_ready, Decimal::ZERO)
        .expect("loss sample trade should close at stop");
    let report_matches =
        report_args_matches(&["--account", &account.id.to_string(), "--days", "365"]);

    crate::dispatcher::performance_reports::performance_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Json,
    )
    .expect("non-empty performance json should render");
    crate::dispatcher::summary_report::summary_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Json,
    )
    .expect("fee-aware summary json should render");
    crate::dispatcher::metrics_report::metrics_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Json,
    )
    .expect("fee-aware metrics json should render");
    crate::dispatcher::concentration_report::concentration_report(
        &mut dispatcher.trust,
        &report_args_matches(&["--account", &account.id.to_string(), "--open-only"]),
        ReportOutputFormat::Text,
    )
    .expect("open-only concentration should handle portfolios with only closed trades");
    let open_trade = dispatcher
        .trust
        .create_trade(
            model::DraftTrade {
                account: account.clone(),
                trading_vehicle: trade.trading_vehicle.clone(),
                quantity: 1.into(),
                category: model::TradeCategory::Long,
                currency: Currency::USD,
                thesis: Some("open concentration".to_string()),
                sector: Some("technology".to_string()),
                asset_class: Some("equity".to_string()),
                context: None,
            },
            dec!(95),
            dec!(100),
            dec!(110),
        )
        .expect("open risk trade should be created");
    dispatcher
        .trust
        .fund_trade(&open_trade)
        .expect("open risk trade should be funded");
    let open_trade = dispatcher
        .trust
        .search_trades(account.id, Status::Funded)
        .expect("funded open risk trade should load")
        .into_iter()
        .find(|candidate| candidate.id == open_trade.id)
        .expect("funded open risk trade should be persisted");
    let (open_submitted, _) = dispatcher
        .trust
        .submit_trade(&open_trade)
        .expect("open risk trade should be submitted");
    let mut open_fill_ready = open_submitted.clone();
    open_fill_ready.entry.status = model::OrderStatus::Filled;
    open_fill_ready.entry.filled_quantity = open_fill_ready.entry.quantity;
    open_fill_ready.entry.average_filled_price = Some(open_fill_ready.entry.unit_price);
    open_fill_ready.entry.filled_at = Some(Utc::now().naive_utc());
    dispatcher
        .trust
        .fill_trade(&open_fill_ready, Decimal::ZERO)
        .expect("open risk trade should be filled");
    crate::dispatcher::summary_report::summary_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Json,
    )
    .expect("summary json should render open concentration percentage");

    let mut zero_pnl_dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (zero_account, filled) = seed_filled_trade(&mut zero_pnl_dispatcher);
    let mut target_ready = filled.clone();
    target_ready.target.status = model::OrderStatus::Filled;
    target_ready.target.filled_quantity = target_ready.target.quantity;
    target_ready.target.average_filled_price = Some(target_ready.entry.unit_price);
    target_ready.target.filled_at = Some(Utc::now().naive_utc());
    zero_pnl_dispatcher
        .trust
        .target_acquired(&target_ready, Decimal::ZERO)
        .expect("zero pnl target close should be persisted");
    let attribution = ReportCommandBuilder::new()
        .attribution()
        .build()
        .get_matches_from([
            "report",
            "attribution",
            "--account",
            &zero_account.id.to_string(),
            "--by",
            "symbol",
            "--from",
            "2020-01-01",
            "--to",
            "2030-01-01",
        ]);
    let attribution_matches = attribution.subcommand_matches("attribution").unwrap();
    crate::dispatcher::attribution_report::attribution_report(
        &mut zero_pnl_dispatcher.trust,
        attribution_matches,
        ReportOutputFormat::Text,
    )
    .expect("zero pnl attribution text should render");
    crate::dispatcher::attribution_report::attribution_report(
        &mut zero_pnl_dispatcher.trust,
        attribution_matches,
        ReportOutputFormat::Json,
    )
    .expect("zero pnl attribution json should render");

    let mut empty_dispatcher = test_dispatcher();
    crate::dispatcher::risk_report::risk_report(
        &mut empty_dispatcher.trust,
        &report_args_matches(&[]),
        ReportOutputFormat::Json,
    )
    .expect("empty aggregate risk json should render with zero equity");
    let empty_account = empty_dispatcher
        .trust
        .create_account(
            "zero-equity",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("empty account");
    crate::dispatcher::summary_report::summary_report(
        &mut empty_dispatcher.trust,
        &report_args_matches(&["--account", &empty_account.id.to_string()]),
        ReportOutputFormat::Json,
    )
    .expect("zero-equity summary json should render");

    let mut empty_bars = StubBroker::quote_trade_fixture();
    empty_bars.bars.clear();
    let mut dispatcher = test_dispatcher_with_broker(Box::new(empty_bars));
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);
    let benchmark = ReportCommandBuilder::new()
        .benchmark()
        .build()
        .get_matches_from([
            "report",
            "benchmark",
            "--account",
            &account.id.to_string(),
            "--benchmark",
            "SPY",
            "--from",
            "2020-01-01",
            "--to",
            "2030-01-01",
        ]);
    crate::dispatcher::benchmark_report::benchmark_report(
        &mut dispatcher.trust,
        benchmark
            .subcommand_matches("benchmark")
            .expect("benchmark subcommand"),
        ReportOutputFormat::Json,
    )
    .expect("benchmark report should render with no benchmark bars");

    let mut zero_open_bars = StubBroker::quote_trade_fixture();
    if let Some(first_bar) = zero_open_bars.bars.first_mut() {
        first_bar.open = Decimal::ZERO;
    }
    let mut dispatcher = test_dispatcher_with_broker(Box::new(zero_open_bars));
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);
    let benchmark = ReportCommandBuilder::new()
        .benchmark()
        .build()
        .get_matches_from([
            "report",
            "benchmark",
            "--account",
            &account.id.to_string(),
            "--benchmark",
            "SPY",
            "--from",
            "2020-01-01",
            "--to",
            "2030-01-01",
        ]);
    crate::dispatcher::benchmark_report::benchmark_report(
        &mut dispatcher.trust,
        benchmark
            .subcommand_matches("benchmark")
            .expect("benchmark subcommand"),
        ReportOutputFormat::Text,
    )
    .expect("benchmark report should render when first benchmark open is zero");

    let timeline_missing_granularity = Command::new("timeline")
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("granularity").long("granularity"))
        .arg(Arg::new("from").long("from"))
        .arg(Arg::new("to").long("to"))
        .get_matches_from([
            "timeline",
            "--account",
            &account.id.to_string(),
            "--from",
            "2020-01-01",
            "--to",
            "2030-01-01",
        ]);
    let error = crate::dispatcher::timeline_report::timeline_report(
        &mut dispatcher.trust,
        &timeline_missing_granularity,
        ReportOutputFormat::Json,
    )
    .expect_err("timeline should require granularity");
    assert_eq!(error.code, "missing_argument");
}
