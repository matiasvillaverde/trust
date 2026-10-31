#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::broker::*;
use super::fixtures::*;
use crate::commands::AccountCommandBuilder;
use crate::commands::AdvisorCommandBuilder;
use crate::commands::DbCommandBuilder;
use crate::commands::DistributionCommandBuilder;
use crate::commands::GradeCommandBuilder;
use crate::commands::KeysCommandBuilder;
use crate::commands::LevelCommandBuilder;
use crate::commands::MarketDataCommandBuilder;
use crate::commands::MetricsCommandBuilder;
use crate::commands::OnboardingCommandBuilder;
use crate::commands::PolicyCommandBuilder;
use crate::commands::ReportCommandBuilder;
use crate::commands::RuleCommandBuilder;
use crate::commands::TradeCommandBuilder;
use crate::commands::TradingVehicleCommandBuilder;
use crate::commands::TransactionCommandBuilder;
use clap::Arg;
use clap::Command;
use model::Environment;
use model::OrderCategory;
use model::Status;
use model::TradingVehicleCategory;
use rust_decimal_macros::dec;

#[test]
fn test_create_trade_non_interactive_surfaces_fund_and_submit_failures() {
    let mut unfunded_dispatcher = test_dispatcher();
    let unfunded_account = unfunded_dispatcher
        .trust
        .create_account(
            "no-cash-for-auto-fund",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account");
    unfunded_dispatcher
        .trust
        .create_trading_vehicle("MSFT", None, &TradingVehicleCategory::Stock, "alpaca")
        .expect("vehicle");
    let auto_fund_without_cash = trade_matches(&[
        "--account",
        &unfunded_account.name,
        "--symbol",
        "MSFT",
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
        "--auto-fund",
    ]);
    let error = crate::dispatcher::trade_commands::create_trade(
        &mut unfunded_dispatcher.trust,
        &auto_fund_without_cash,
    )
    .expect_err("auto-fund without available cash should fail");
    assert!(error.to_string().contains("trade_fund_failed"));

    let mut submit_failure_dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::quote_trade_fixture()));
    let (submit_failure_account, _vehicle) =
        seed_account_and_vehicle(&mut submit_failure_dispatcher);
    let auto_submit_with_broker_error = trade_matches(&[
        "--account",
        &submit_failure_account.name,
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
        "--auto-submit",
    ]);
    let error = crate::dispatcher::trade_commands::create_trade(
        &mut submit_failure_dispatcher.trust,
        &auto_submit_with_broker_error,
    )
    .expect_err("broker submit failure should surface");
    assert!(error.to_string().contains("trade_submit_failed"));
}

#[test]
fn test_create_trade_non_interactive_success_can_auto_fund() {
    let mut dispatcher = test_dispatcher();
    let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

    let matches = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "aapl",
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
        "2",
        "--currency",
        "usd",
        "--thesis",
        "breakout",
        "--sector",
        "technology",
        "--asset-class",
        "equity",
        "--context",
        "daily setup",
        "--auto-fund",
    ]);

    crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &matches)
        .expect("valid non-interactive trade should be created and funded");

    let funded = dispatcher
        .trust
        .search_trades(account.id, Status::Funded)
        .expect("funded trades should load");
    assert_eq!(funded.len(), 1);
    assert_eq!(funded[0].trading_vehicle.symbol, "AAPL");
    assert_eq!(funded[0].entry.quantity, dec!(2));
    assert_eq!(funded[0].safety_stop.category, OrderCategory::StopLimit);
    assert_eq!(funded[0].thesis.as_deref(), Some("breakout"));
}

#[test]
fn test_create_trade_non_interactive_supports_crypto_and_ibkr_bonds() {
    let mut dispatcher = test_dispatcher();
    let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);
    dispatcher
        .trust
        .create_trading_vehicle("BTCUSD", None, &TradingVehicleCategory::Crypto, "alpaca")
        .expect("crypto vehicle");
    dispatcher
        .trust
        .create_trading_vehicle("9128285M8", None, &TradingVehicleCategory::Bond, "ibkr")
        .expect("bond vehicle");
    dispatcher
        .trust
        .create_trading_vehicle("ALPBOND", None, &TradingVehicleCategory::Bond, "alpaca")
        .expect("alpaca bond vehicle");

    let crypto = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "btcusd",
        "--category",
        "long",
        "--entry",
        "68123.45",
        "--stop",
        "65000",
        "--target",
        "72000",
        "--quantity",
        "0.00012345",
    ]);
    crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &crypto)
        .expect("crypto trade should create");

    let bond = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "9128285M8",
        "--category",
        "long",
        "--entry",
        "98.750",
        "--stop",
        "96.000",
        "--target",
        "101.250",
        "--quantity",
        "1.5",
    ]);
    crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &bond)
        .expect("IBKR bond trade should create");

    let alpaca_bond = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "ALPBOND",
        "--category",
        "long",
        "--entry",
        "98.750",
        "--stop",
        "96.000",
        "--target",
        "101.250",
        "--quantity",
        "1",
    ]);
    let error =
        crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &alpaca_bond)
            .expect_err("Alpaca bond trade should be rejected");
    assert!(error.to_string().contains("trade_create_failed"));
    assert!(error
        .to_string()
        .contains("Bonds are not supported by Alpaca"));

    let new_trades = dispatcher
        .trust
        .search_trades(account.id, Status::New)
        .expect("new trades");
    assert!(new_trades
        .iter()
        .any(|trade| trade.trading_vehicle.symbol == "BTCUSD"
            && trade.entry.quantity == dec!(0.00012345)));
    assert!(new_trades
        .iter()
        .any(|trade| trade.trading_vehicle.symbol == "9128285M8"
            && trade.entry.quantity == dec!(1.5)));
}

#[test]
fn test_create_trade_non_interactive_auto_submit_uses_broker_success_path() {
    let mut dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

    let matches = trade_matches(&[
        "--account",
        &account.name,
        "--symbol",
        "aapl",
        "--category",
        "long",
        "--entry",
        "100",
        "--stop",
        "95",
        "--target",
        "110",
        "--quantity",
        "2",
        "--currency",
        "usd",
        "--auto-submit",
    ]);

    crate::dispatcher::trade_commands::create_trade(&mut dispatcher.trust, &matches)
        .expect("auto-submit should fund and submit through the stub broker");

    let submitted = dispatcher
        .trust
        .search_trades(account.id, Status::Submitted)
        .expect("submitted trades should load");
    assert_eq!(submitted.len(), 1);
    assert_eq!(submitted[0].trading_vehicle.symbol, "AAPL");
    assert!(submitted[0]
        .entry
        .broker_order_id
        .as_deref()
        .expect("entry broker id should be persisted")
        .starts_with("entry-"));
}

#[test]
fn test_trade_non_interactive_lifecycle_handlers_persist_expected_statuses() {
    let trade_id_matches = |trade_id: &str| {
        Command::new("test")
            .arg(Arg::new("trade-id").long("trade-id"))
            .get_matches_from(["test", "--trade-id", trade_id])
    };

    let mut cancel_dispatcher = test_dispatcher();
    let (cancel_account, cancel_trade) = seed_new_trade(&mut cancel_dispatcher);
    crate::dispatcher::trade_commands::create_funding(
        &mut cancel_dispatcher.trust,
        &trade_id_matches(&cancel_trade.id.to_string()),
    )
    .expect("funding handler should fund a new trade");
    let funded_for_cancel = cancel_dispatcher
        .trust
        .search_trades(cancel_account.id, Status::Funded)
        .expect("funded trades should load")
        .into_iter()
        .find(|candidate| candidate.id == cancel_trade.id)
        .expect("funded trade should exist");
    crate::dispatcher::trade_commands::create_cancel(
        &mut cancel_dispatcher.trust,
        &trade_id_matches(&funded_for_cancel.id.to_string()),
    )
    .expect("cancel handler should cancel funded trade");
    let canceled = cancel_dispatcher
        .trust
        .search_trades(cancel_account.id, Status::Canceled)
        .expect("canceled trades should load");
    assert!(canceled
        .iter()
        .any(|candidate| candidate.id == funded_for_cancel.id));

    let mut sync_dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (sync_account, sync_trade) = seed_new_trade(&mut sync_dispatcher);
    crate::dispatcher::trade_commands::create_funding(
        &mut sync_dispatcher.trust,
        &trade_id_matches(&sync_trade.id.to_string()),
    )
    .expect("funding handler should fund before submit");
    let funded_for_submit = sync_dispatcher
        .trust
        .search_trades(sync_account.id, Status::Funded)
        .expect("funded trades should load")
        .into_iter()
        .find(|candidate| candidate.id == sync_trade.id)
        .expect("funded trade should exist");
    crate::dispatcher::trade_commands::create_submit(
        &mut sync_dispatcher.trust,
        &trade_id_matches(&funded_for_submit.id.to_string()),
    )
    .expect("submit handler should submit funded trade");
    let submitted = sync_dispatcher
        .trust
        .search_trades(sync_account.id, Status::Submitted)
        .expect("submitted trades should load")
        .into_iter()
        .find(|candidate| candidate.id == funded_for_submit.id)
        .expect("submitted trade should exist");
    crate::dispatcher::trade_commands::create_sync(
        &mut sync_dispatcher.trust,
        &trade_id_matches(&submitted.id.to_string()),
    )
    .expect("sync handler should apply filled broker payload");
    let filled = sync_dispatcher
        .trust
        .search_trades(sync_account.id, Status::Filled)
        .expect("filled trades should load");
    assert!(filled.iter().any(|candidate| candidate.id == submitted.id));
}

#[test]
fn test_dispatchers_cover_non_interactive_routes_with_safe_error_paths() {
    let _guard = env_guard();
    std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");

    // db
    let mut dispatcher = test_dispatcher();
    let db = DbCommandBuilder::new().import().build().get_matches_from([
        "db",
        "import",
        "--input",
        "/tmp/does-not-exist.json",
    ]);
    assert!(crate::dispatcher::database::dispatch_db(&mut dispatcher.trust, &db).is_err());

    // keys (protected-set + protected-delete)
    let mut dispatcher = test_dispatcher();
    let keys_set = KeysCommandBuilder::new()
        .protected_set()
        .build()
        .get_matches_from([
            "keys",
            "protected-set",
            "--value",
            "next-secret",
            "--confirm-protected",
            "wrong",
        ]);
    let _ = crate::dispatcher::keys::dispatch_keys(&mut dispatcher.trust, &keys_set);
    let keys_delete = KeysCommandBuilder::new()
        .protected_delete()
        .build()
        .get_matches_from(["keys", "protected-delete", "--confirm-protected", "wrong"]);
    assert!(crate::dispatcher::keys::dispatch_keys(&mut dispatcher.trust, &keys_delete).is_err());

    // account create (non-interactive args), list and balance arms
    let mut dispatcher = test_dispatcher();
    let create_account = AccountCommandBuilder::new()
        .create_account()
        .build()
        .get_matches_from([
            "accounts",
            "create",
            "--confirm-protected",
            "secret",
            "--name",
            "acc",
            "--description",
            "desc",
            "--environment",
            "invalid",
            "--taxes",
            "20",
            "--earnings",
            "10",
        ]);
    assert!(
        crate::dispatcher::accounts::dispatch_accounts(&mut dispatcher.trust, &create_account)
            .is_err()
    );
    let list = AccountCommandBuilder::new()
        .list_accounts()
        .build()
        .get_matches_from(["accounts", "list"]);
    assert!(crate::dispatcher::accounts::dispatch_accounts(&mut dispatcher.trust, &list).is_ok());
    let balance = AccountCommandBuilder::new()
        .balance_accounts()
        .build()
        .get_matches_from(["accounts", "balance"]);
    assert!(
        crate::dispatcher::accounts::dispatch_accounts(&mut dispatcher.trust, &balance).is_ok()
    );

    // transaction/rule guarded before interactive dialogs
    let mut dispatcher = test_dispatcher();
    let deposit = TransactionCommandBuilder::new()
        .deposit()
        .build()
        .get_matches_from(["transaction", "deposit"]);
    assert!(
        crate::dispatcher::capital::dispatch_transactions(&mut dispatcher.trust, &deposit).is_err()
    );
    let create_rule = RuleCommandBuilder::new()
        .create_rule()
        .build()
        .get_matches_from(["rule", "create"]);
    let rule_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = crate::dispatcher::rules::dispatch_rules(&mut dispatcher.trust, &create_rule);
    }));
    assert!(rule_result.is_err());

    // trading vehicle create (from-alpaca path)
    let mut dispatcher = test_dispatcher();
    let vehicle = TradingVehicleCommandBuilder::new()
        .create_trading_vehicle()
        .build()
        .get_matches_from([
            "trading-vehicle",
            "create",
            "--from-alpaca",
            "--symbol",
            "AAPL",
        ]);
    assert!(crate::dispatcher::instruments::dispatch_trading_vehicle(
        &mut dispatcher.trust,
        &vehicle
    )
    .is_err());

    // trade non-interactive branches with invalid IDs
    let mut dispatcher = test_dispatcher();
    let fund = TradeCommandBuilder::new()
        .fund_trade()
        .build()
        .get_matches_from(["trade", "fund", "--trade-id", "not-a-uuid"]);
    assert!(
        crate::dispatcher::trade_commands::dispatch_trade(&mut dispatcher.trust, &fund).is_err()
    );
    let submit = TradeCommandBuilder::new()
        .submit_trade()
        .build()
        .get_matches_from(["trade", "submit", "--trade-id", "not-a-uuid"]);
    assert!(
        crate::dispatcher::trade_commands::dispatch_trade(&mut dispatcher.trust, &submit).is_err()
    );
    let cancel = TradeCommandBuilder::new()
        .cancel_trade()
        .build()
        .get_matches_from(["trade", "cancel", "--trade-id", "not-a-uuid"]);
    assert!(
        crate::dispatcher::trade_commands::dispatch_trade(&mut dispatcher.trust, &cancel).is_err()
    );
    let sync = TradeCommandBuilder::new()
        .sync_trade()
        .build()
        .get_matches_from(["trade", "sync", "--trade-id", "not-a-uuid"]);
    assert!(
        crate::dispatcher::trade_commands::dispatch_trade(&mut dispatcher.trust, &sync).is_err()
    );
    let search = TradeCommandBuilder::new()
        .search_trade()
        .build()
        .get_matches_from(["trade", "search", "--status", "nope", "--format", "json"]);
    assert!(
        crate::dispatcher::trade_commands::dispatch_trade(&mut dispatcher.trust, &search).is_err()
    );
    let list_open = TradeCommandBuilder::new()
        .list_open()
        .build()
        .get_matches_from([
            "trade",
            "list-open",
            "--account",
            "not-a-uuid",
            "--format",
            "json",
        ]);
    assert!(
        crate::dispatcher::trade_commands::dispatch_trade(&mut dispatcher.trust, &list_open)
            .is_err()
    );
    let reconcile = TradeCommandBuilder::new()
        .reconcile()
        .build()
        .get_matches_from([
            "trade",
            "reconcile",
            "--trade-id",
            "not-a-uuid",
            "--format",
            "json",
        ]);
    assert!(
        crate::dispatcher::trade_commands::dispatch_trade(&mut dispatcher.trust, &reconcile)
            .is_err()
    );

    // distribution/report/market-data/grade/level
    let mut dispatcher = test_dispatcher();
    let dist = DistributionCommandBuilder::new()
        .history()
        .build()
        .get_matches_from(["distribution", "history", "--account-id", "not-a-uuid"]);
    assert!(
        crate::dispatcher::distribution::dispatch_distribution(&mut dispatcher.trust, &dist)
            .is_err()
    );
    let report = ReportCommandBuilder::new()
        .performance()
        .build()
        .get_matches_from([
            "report",
            "--format",
            "json",
            "performance",
            "--account",
            "bad",
        ]);
    assert!(crate::dispatcher::reports::dispatch_report(&mut dispatcher.trust, &report).is_err());
    let market = MarketDataCommandBuilder::new()
        .bars()
        .build()
        .get_matches_from([
            "market-data",
            "bars",
            "--account",
            "bad",
            "--symbol",
            "AAPL",
            "--timeframe",
            "1d",
            "--start",
            "not-a-date",
            "--end",
            "2026-01-02T00:00:00Z",
        ]);
    assert!(
        crate::dispatcher::market_data::dispatch_market_data(&mut dispatcher.trust, &market)
            .is_err()
    );
    let market_quote = MarketDataCommandBuilder::new()
        .quote()
        .build()
        .get_matches_from([
            "market-data",
            "quote",
            "--account",
            "bad",
            "--symbol",
            "AAPL",
            "--format",
            "json",
        ]);
    assert!(crate::dispatcher::market_data::dispatch_market_data(
        &mut dispatcher.trust,
        &market_quote
    )
    .is_err());
    let market_trade = MarketDataCommandBuilder::new()
        .trade()
        .build()
        .get_matches_from([
            "market-data",
            "trade",
            "--account",
            "bad",
            "--symbol",
            "AAPL",
            "--format",
            "json",
        ]);
    assert!(crate::dispatcher::market_data::dispatch_market_data(
        &mut dispatcher.trust,
        &market_trade
    )
    .is_err());
    let market_session = MarketDataCommandBuilder::new()
        .session()
        .build()
        .get_matches_from([
            "market-data",
            "session",
            "--account",
            "bad",
            "--symbol",
            "AAPL",
            "--format",
            "json",
        ]);
    assert!(crate::dispatcher::market_data::dispatch_market_data(
        &mut dispatcher.trust,
        &market_session
    )
    .is_err());
    let grade =
        GradeCommandBuilder::new()
            .show()
            .build()
            .get_matches_from(["grade", "show", "not-a-uuid"]);
    assert!(crate::dispatcher::grades::dispatch_grade(&mut dispatcher.trust, &grade).is_err());
    let level = LevelCommandBuilder::new()
        .status()
        .build()
        .get_matches_from(["level", "status", "--account", "bad"]);
    assert!(crate::dispatcher::levels::dispatch_level(&mut dispatcher.trust, &level).is_err());

    // onboarding/policy/metrics/advisor
    let mut dispatcher = test_dispatcher();
    let onboarding = OnboardingCommandBuilder::new()
        .init()
        .build()
        .get_matches_from([
            "onboarding",
            "--format",
            "json",
            "init",
            "--protected-keyword",
            "secret2",
        ]);
    assert!(
        crate::dispatcher::onboarding::dispatch_onboarding(&mut dispatcher.trust, &onboarding)
            .is_err()
    );
    let policy = PolicyCommandBuilder::new()
        .build()
        .get_matches_from(["policy", "--format", "json"]);
    assert!(crate::dispatcher::policy::dispatch_policy(&mut dispatcher.trust, &policy).is_ok());
    let metrics = MetricsCommandBuilder::new()
        .compare()
        .build()
        .get_matches_from([
            "metrics",
            "compare",
            "--period1",
            "invalid",
            "--period2",
            "last-7-days",
            "--format",
            "json",
        ]);
    assert!(
        crate::dispatcher::advanced_metrics::dispatch_metrics(&mut dispatcher.trust, &metrics)
            .is_ok()
    );
    let metrics_bond = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--face-value",
            "1000",
            "--market-price",
            "950",
            "--coupon-rate",
            "4",
            "--quantity",
            "3",
            "--years-to-maturity",
            "5",
            "--format",
            "json",
        ]);
    assert!(crate::dispatcher::advanced_metrics::dispatch_metrics(
        &mut dispatcher.trust,
        &metrics_bond
    )
    .is_ok());
    let advisor = AdvisorCommandBuilder::new()
        .configure()
        .build()
        .get_matches_from([
            "advisor",
            "configure",
            "--account",
            "bad",
            "--sector-limit",
            "30",
            "--asset-class-limit",
            "40",
            "--single-position-limit",
            "20",
            "--confirm-protected",
            "secret",
        ]);
    assert!(
        crate::dispatcher::advisor_config::dispatch_advisor(&mut dispatcher.trust, &advisor)
            .is_err()
    );

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
}
