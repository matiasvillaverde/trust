#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::broker::*;
use super::fixtures::*;
use crate::commands::TradeCommandBuilder;
use clap::Arg;
use clap::Command;
use model::Currency;
use model::Environment;
use model::OrderCategory;
use model::Status;
use model::TradingVehicleCategory;
use rust_decimal_macros::dec;

#[test]
fn test_create_trade_interactive_dispatch_builds_trade_from_scripted_dialog() {
    let mut dispatcher = test_dispatcher();
    let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

    crate::dialogs::scripted_reset();
    crate::dialogs::scripted_push_select(Ok(Some(0))); // account
    crate::dialogs::scripted_push_select(Ok(Some(0))); // trading vehicle
    crate::dialogs::scripted_push_select(Ok(Some(0))); // long category
    crate::dialogs::scripted_push_input(Ok("100".to_string()));
    crate::dialogs::scripted_push_input(Ok("95".to_string()));
    crate::dialogs::scripted_push_select(Ok(Some(1))); // stop-limit safety order
    crate::dialogs::scripted_push_select(Ok(Some(0))); // USD balance
    crate::dialogs::scripted_push_input(Ok("2".to_string()));
    crate::dialogs::scripted_push_input(Ok("110".to_string()));
    crate::dialogs::scripted_push_input(Ok("scripted breakout".to_string()));
    crate::dialogs::scripted_push_input(Ok("technology".to_string()));
    crate::dialogs::scripted_push_input(Ok("equity".to_string()));
    crate::dialogs::scripted_push_input(Ok("daily setup".to_string()));

    let matches = trade_matches(&[]);
    crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &matches)
        .expect("scripted interactive trade creation should succeed");

    let created = dispatcher
        .trust
        .search_trades(account.id, Status::New)
        .expect("new trades should load");
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].trading_vehicle.symbol, "AAPL");
    assert_eq!(created[0].entry.quantity, dec!(2));
    assert_eq!(created[0].safety_stop.category, OrderCategory::StopLimit);
    assert_eq!(created[0].thesis.as_deref(), Some("scripted breakout"));

    crate::dialogs::scripted_reset();
}

#[test]
fn test_trade_interactive_dispatch_funds_submits_and_syncs_selected_trade() {
    let mut dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (account, _trade) = seed_new_trade(&mut dispatcher);
    dispatcher
        .trust
        .configure_advisory_thresholds(
            account.id,
            core::services::AdvisoryThresholds {
                sector_limit_pct: dec!(100),
                asset_class_limit_pct: dec!(100),
                single_position_limit_pct: dec!(100),
            },
        )
        .expect("configure permissive advisory thresholds");

    crate::dialogs::scripted_reset();
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_confirm(Ok(true));
    let fund_matches = TradeCommandBuilder::new()
        .fund_trade()
        .build()
        .get_matches_from(["trade", "fund"]);
    crate::dispatcher::trade_commands::create_funding(
        &mut dispatcher.trust,
        fund_matches.subcommand_matches("fund").unwrap(),
    )
    .expect("scripted interactive funding should succeed");
    let funded = dispatcher
        .trust
        .search_trades(account.id, Status::Funded)
        .expect("funded trades should load");
    assert_eq!(funded.len(), 1);

    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    let submit_matches = TradeCommandBuilder::new()
        .submit_trade()
        .build()
        .get_matches_from(["trade", "submit"]);
    crate::dispatcher::trade_commands::create_submit(
        &mut dispatcher.trust,
        submit_matches.subcommand_matches("submit").unwrap(),
    )
    .expect("scripted interactive submit should succeed");
    let submitted = dispatcher
        .trust
        .search_trades(account.id, Status::Submitted)
        .expect("submitted trades should load");
    assert_eq!(submitted.len(), 1);

    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    let sync_matches = TradeCommandBuilder::new()
        .sync_trade()
        .build()
        .get_matches_from(["trade", "sync"]);
    crate::dispatcher::trade_commands::create_sync(
        &mut dispatcher.trust,
        sync_matches.subcommand_matches("sync").unwrap(),
    )
    .expect("scripted interactive sync should succeed");
    let filled = dispatcher
        .trust
        .search_trades(account.id, Status::Filled)
        .expect("filled trades should load");
    assert_eq!(filled.len(), 1);

    crate::dialogs::scripted_reset();
}

#[test]
fn test_trade_interactive_dispatch_cancels_selected_funded_trade() {
    let mut dispatcher = test_dispatcher();
    let (account, trade) = seed_new_trade(&mut dispatcher);
    dispatcher.trust.fund_trade(&trade).expect("fund trade");

    crate::dialogs::scripted_reset();
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    let cancel_matches = TradeCommandBuilder::new()
        .cancel_trade()
        .build()
        .get_matches_from(["trade", "cancel"]);
    crate::dispatcher::trade_commands::create_cancel(
        &mut dispatcher.trust,
        cancel_matches.subcommand_matches("cancel").unwrap(),
    )
    .expect("scripted interactive cancel should succeed");

    let canceled = dispatcher
        .trust
        .search_trades(account.id, Status::Canceled)
        .expect("canceled trades should load");
    assert_eq!(canceled.len(), 1);

    crate::dialogs::scripted_reset();
}

#[test]
fn test_trade_interactive_dispatch_manual_fill_error_and_manual_exit_paths() {
    let mut fill_dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (fill_account, fill_trade) = seed_new_trade(&mut fill_dispatcher);
    let (funded, _, _, _) = fill_dispatcher
        .trust
        .fund_trade(&fill_trade)
        .expect("fund trade before manual fill");
    fill_dispatcher
        .trust
        .submit_trade(&funded)
        .expect("submit trade before manual fill");

    crate::dialogs::scripted_reset();
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_input(Ok("1.25".to_string()));
    crate::dispatcher::trade_commands::create_fill(&mut fill_dispatcher.trust);
    let filled_after_manual_fill_attempt = fill_dispatcher
        .trust
        .search_trades(fill_account.id, Status::Filled)
        .expect("filled trades should load after manual fill attempt");
    assert!(
        filled_after_manual_fill_attempt.is_empty(),
        "manual fill without a broker-reported average fill price should not mutate status"
    );
    let still_submitted = fill_dispatcher
        .trust
        .search_trades(fill_account.id, Status::Submitted)
        .expect("submitted trades should remain loadable");
    assert_eq!(still_submitted.len(), 1);

    let mut stop_dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (stop_account, _filled_trade) = seed_filled_trade(&mut stop_dispatcher);
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_input(Ok("2.00".to_string()));
    crate::dispatcher::trade_commands::create_stop(&mut stop_dispatcher.trust);
    let stopped = stop_dispatcher
        .trust
        .search_trades(stop_account.id, Status::ClosedStopLoss)
        .expect("stopped trades should load");
    assert!(
        stopped.is_empty(),
        "manual stop without a broker-reported stop fill price should not mutate status"
    );
    let still_filled_after_stop_attempt = stop_dispatcher
        .trust
        .search_trades(stop_account.id, Status::Filled)
        .expect("filled trades should remain loadable after manual stop attempt");
    assert_eq!(still_filled_after_stop_attempt.len(), 1);

    let mut target_dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (target_account, _filled_trade) = seed_filled_trade(&mut target_dispatcher);
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_input(Ok("2.50".to_string()));
    crate::dispatcher::trade_commands::create_target(&mut target_dispatcher.trust);
    let targeted = target_dispatcher
        .trust
        .search_trades(target_account.id, Status::ClosedTarget)
        .expect("target-closed trades should load");
    assert!(
        targeted.is_empty(),
        "manual target without a broker-reported target fill price should not mutate status"
    );
    let still_filled_after_target_attempt = target_dispatcher
        .trust
        .search_trades(target_account.id, Status::Filled)
        .expect("filled trades should remain loadable after manual target attempt");
    assert_eq!(still_filled_after_target_attempt.len(), 1);

    crate::dialogs::scripted_reset();
}

#[test]
fn test_trade_non_interactive_fund_submit_sync_and_cancel_success_paths() {
    let mut dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (account, trade) = seed_new_trade(&mut dispatcher);
    let trade_id = trade.id.to_string();

    let fund_matches = TradeCommandBuilder::new()
        .fund_trade()
        .build()
        .get_matches_from(["trade", "fund", "--trade-id", &trade_id]);
    crate::dispatcher::trade_commands::create_funding(
        &mut dispatcher.trust,
        fund_matches.subcommand_matches("fund").unwrap(),
    )
    .expect("non-interactive funding should succeed");
    let funded = dispatcher
        .trust
        .search_trades(account.id, Status::Funded)
        .expect("funded trade should load");
    assert_eq!(funded.len(), 1);
    let funded_id = funded[0].id.to_string();

    let submit_matches = TradeCommandBuilder::new()
        .submit_trade()
        .build()
        .get_matches_from(["trade", "submit", "--trade-id", &funded_id]);
    crate::dispatcher::trade_commands::create_submit(
        &mut dispatcher.trust,
        submit_matches.subcommand_matches("submit").unwrap(),
    )
    .expect("non-interactive submit should succeed");
    let submitted = dispatcher
        .trust
        .search_trades(account.id, Status::Submitted)
        .expect("submitted trade should load");
    assert_eq!(submitted.len(), 1);
    let submitted_id = submitted[0].id.to_string();

    let sync_matches = TradeCommandBuilder::new()
        .sync_trade()
        .build()
        .get_matches_from(["trade", "sync", "--trade-id", &submitted_id]);
    crate::dispatcher::trade_commands::create_sync(
        &mut dispatcher.trust,
        sync_matches.subcommand_matches("sync").unwrap(),
    )
    .expect("non-interactive sync should succeed");
    let filled = dispatcher
        .trust
        .search_trades(account.id, Status::Filled)
        .expect("filled trade should load");
    assert_eq!(filled.len(), 1);

    let mut cancel_dispatcher = test_dispatcher();
    let (cancel_account, cancel_trade) = seed_new_trade(&mut cancel_dispatcher);
    let (funded_cancel, _, _, _) = cancel_dispatcher
        .trust
        .fund_trade(&cancel_trade)
        .expect("fund trade before cancel");
    let funded_cancel_id = funded_cancel.id.to_string();
    let cancel_matches = TradeCommandBuilder::new()
        .cancel_trade()
        .build()
        .get_matches_from(["trade", "cancel", "--trade-id", &funded_cancel_id]);
    crate::dispatcher::trade_commands::create_cancel(
        &mut cancel_dispatcher.trust,
        cancel_matches.subcommand_matches("cancel").unwrap(),
    )
    .expect("non-interactive cancel should succeed");
    let canceled = cancel_dispatcher
        .trust
        .search_trades(cancel_account.id, Status::Canceled)
        .expect("canceled trade should load");
    assert_eq!(canceled.len(), 1);
}

#[test]
fn test_trade_non_interactive_handlers_surface_core_and_broker_failures() {
    let trade_id_matches = |trade_id: &str| {
        Command::new("test")
            .arg(Arg::new("trade-id").long("trade-id"))
            .get_matches_from(["test", "--trade-id", trade_id])
    };

    let mut funding_dispatcher = test_dispatcher();
    let account = funding_dispatcher
        .trust
        .create_account(
            "no-cash-fund",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account");
    let vehicle = funding_dispatcher
        .trust
        .create_trading_vehicle("NOCASH", None, &TradingVehicleCategory::Stock, "alpaca")
        .expect("vehicle");
    let trade = funding_dispatcher
        .trust
        .create_trade(
            model::DraftTrade {
                account,
                trading_vehicle: vehicle,
                quantity: 1.into(),
                category: model::TradeCategory::Long,
                currency: Currency::USD,
                thesis: None,
                sector: None,
                asset_class: None,
                context: None,
            },
            dec!(95),
            dec!(100),
            dec!(110),
        )
        .expect("trade can be drafted without cash");
    let error = crate::dispatcher::trade_commands::create_funding(
        &mut funding_dispatcher.trust,
        &trade_id_matches(&trade.id.to_string()),
    )
    .expect_err("funding without available cash should fail");
    assert!(error.to_string().contains("trade_fund_failed"));

    let trade_create_matches = trade_matches(&[
        "--account",
        "readable-account",
        "--symbol",
        "AAPL",
        "--category",
        "long",
        "--entry",
        "100",
        "--stop",
        "95",
        "--safety-order-type",
        "stop-limit",
        "--target",
        "110",
        "--quantity",
        "1",
    ]);
    let mut vehicle_lookup_dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::trading_vehicles_after_account(),
    );
    let error = crate::dispatcher::trade_commands::create_trade(
        &mut vehicle_lookup_dispatcher.trust,
        &trade_create_matches,
    )
    .expect_err("trading vehicle read failure should surface during trade creation");
    assert!(error.to_string().contains("trading_vehicle_lookup_failed"));

    let mut order_write_dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::order_write_after_reads(),
    );
    let error = crate::dispatcher::trade_commands::create_trade(
        &mut order_write_dispatcher.trust,
        &trade_create_matches,
    )
    .expect_err("order write failure should surface during trade creation");
    assert!(error.to_string().contains("trade_create_failed"));

    let mut submitted_cancel_broker = StubBroker::quote_trade_fixture();
    submitted_cancel_broker.submit_succeeds = true;
    let mut cancel_dispatcher = test_dispatcher_with_broker(Box::new(submitted_cancel_broker));
    let (_account, trade) = seed_new_trade(&mut cancel_dispatcher);
    let (funded, _, _, _) = cancel_dispatcher
        .trust
        .fund_trade(&trade)
        .expect("fund trade");
    let (submitted, _) = cancel_dispatcher
        .trust
        .submit_trade(&funded)
        .expect("submit trade");
    let error = crate::dispatcher::trade_commands::create_cancel(
        &mut cancel_dispatcher.trust,
        &trade_id_matches(&submitted.id.to_string()),
    )
    .expect_err("submitted cancel broker failure should surface");
    assert!(error.to_string().contains("trade_cancel_failed"));

    let mut sync_broker = StubBroker::quote_trade_fixture();
    sync_broker.submit_succeeds = true;
    sync_broker.sync_fills_entry = false;
    let mut sync_dispatcher = test_dispatcher_with_broker(Box::new(sync_broker));
    let (_account, trade) = seed_new_trade(&mut sync_dispatcher);
    let (funded, _, _, _) = sync_dispatcher
        .trust
        .fund_trade(&trade)
        .expect("fund trade");
    let (submitted, _) = sync_dispatcher
        .trust
        .submit_trade(&funded)
        .expect("submit trade");
    let error = crate::dispatcher::trade_commands::create_sync(
        &mut sync_dispatcher.trust,
        &trade_id_matches(&submitted.id.to_string()),
    )
    .expect_err("sync broker failure should surface");
    assert!(error.to_string().contains("trade_sync_failed"));
}

#[test]
fn test_trade_non_interactive_sync_reports_empty_order_updates() {
    let mut dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_empty_sync_fixture()));
    let (_account, trade) = seed_new_trade(&mut dispatcher);
    let (funded, _, _, _) = dispatcher.trust.fund_trade(&trade).expect("fund trade");
    let (submitted, _) = dispatcher
        .trust
        .submit_trade(&funded)
        .expect("submit trade");
    let matches = Command::new("test")
        .arg(Arg::new("trade-id").long("trade-id"))
        .get_matches_from(["test", "--trade-id", &submitted.id.to_string()]);

    crate::dispatcher::trade_commands::create_sync(&mut dispatcher.trust, &matches)
        .expect("empty broker order update sync should still succeed");
}

#[test]
fn test_create_trade_non_interactive_validates_missing_account_and_quantity() {
    let mut dispatcher = test_dispatcher();

    let missing_account = trade_matches(&["--symbol", "AAPL"]);
    let account_error =
        crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &missing_account)
            .expect_err("missing account should fail");
    assert!(account_error.to_string().contains("missing_argument"));

    let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);
    let invalid_qty = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "AAPL",
        "--category",
        "long",
        "--entry",
        "100",
        "--stop",
        "95",
        "--target",
        "110",
        "--quantity",
        "abc",
    ]);
    let qty_error =
        crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &invalid_qty)
            .expect_err("invalid quantity should fail");
    assert!(qty_error.to_string().contains("invalid_quantity"));

    let zero_qty = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "AAPL",
        "--category",
        "long",
        "--entry",
        "100",
        "--stop",
        "95",
        "--target",
        "110",
        "--quantity",
        "0",
    ]);
    let zero_error =
        crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &zero_qty)
            .expect_err("zero quantity should fail");
    assert!(zero_error
        .to_string()
        .contains("Quantity must be greater than 0"));
}

#[test]
fn test_create_trade_non_interactive_validates_required_symbol_category_and_vehicle() {
    let mut dispatcher = test_dispatcher();
    let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

    let missing_symbol = trade_matches(&[
        "--account",
        &account.name,
        "--category",
        "long",
        "--quantity",
        "1",
    ]);
    let error =
        crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &missing_symbol)
            .expect_err("missing symbol should fail");
    assert!(error.to_string().contains("missing_argument"));

    let missing_category = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "AAPL",
        "--quantity",
        "1",
    ]);
    let error =
        crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &missing_category)
            .expect_err("missing category should fail");
    assert!(error.to_string().contains("missing_argument"));

    let missing_quantity = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "AAPL",
        "--category",
        "long",
    ]);
    let error =
        crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &missing_quantity)
            .expect_err("missing quantity should fail");
    assert!(error.to_string().contains("missing_argument"));

    let unknown_vehicle = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "MSFT",
        "--category",
        "long",
        "--entry",
        "100",
        "--stop",
        "95",
        "--target",
        "110",
        "--quantity",
        "1",
    ]);
    let error =
        crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &unknown_vehicle)
            .expect_err("unknown trading vehicle should fail");
    assert!(error.to_string().contains("trading_vehicle_not_found"));

    let invalid_entry = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "AAPL",
        "--category",
        "long",
        "--entry",
        "not-a-decimal",
        "--stop",
        "95",
        "--target",
        "110",
        "--quantity",
        "1",
    ]);
    let error =
        crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &invalid_entry)
            .expect_err("invalid entry decimal should fail");
    assert!(error.to_string().contains("invalid_decimal"));
}

#[test]
fn test_create_trade_non_interactive_validates_category_and_currency() {
    let mut dispatcher = test_dispatcher();
    let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

    let invalid_category = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "AAPL",
        "--category",
        "swing",
        "--entry",
        "100",
        "--stop",
        "95",
        "--target",
        "110",
        "--quantity",
        "1",
    ]);
    let category_error =
        crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &invalid_category)
            .expect_err("invalid category should fail");
    assert!(category_error
        .to_string()
        .contains("invalid_trade_category"));

    let invalid_currency = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "AAPL",
        "--category",
        "long",
        "--entry",
        "100",
        "--stop",
        "95",
        "--target",
        "110",
        "--quantity",
        "1",
        "--currency",
        "jpy",
    ]);
    let currency_error =
        crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &invalid_currency)
            .expect_err("invalid currency should fail");
    assert!(currency_error.to_string().contains("invalid_currency"));

    let invalid_safety_order_type = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "AAPL",
        "--category",
        "long",
        "--entry",
        "100",
        "--stop",
        "95",
        "--safety-order-type",
        "market",
        "--target",
        "110",
        "--quantity",
        "1",
    ]);
    let safety_order_error = crate::dispatcher::trade_commands::create_trade(
        &mut dispatcher.trust,
        &invalid_safety_order_type,
    )
    .expect_err("invalid safety order type should fail");
    assert!(safety_order_error
        .to_string()
        .contains("invalid_safety_order_type"));
}
