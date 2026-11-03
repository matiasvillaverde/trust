#![cfg(test)]
use super::{
    open_trades_for_account, open_trades_for_account_with, refresh_trade_snapshot,
    refresh_trade_snapshot_with, terminal_status, TradeWatchDialogBuilder,
};
use crate::dialogs::io::{scripted_push_select, scripted_reset};
use crate::test_support::broker::{DialogBroker, SyncBehavior};
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Broker, Currency, DraftTrade, Environment, OrderStatus, Status, TradeCategory,
    TradingVehicleCategory, TransactionCategory,
};
use rust_decimal_macros::dec;
use std::sync::atomic::Ordering;
use uuid::Uuid;

fn test_trust_with_broker<B: Broker + Default + 'static>() -> TrustFacade {
    let db = SqliteDatabase::new_in_memory();
    TrustFacade::new(Box::new(db), Box::<B>::default())
}

fn test_trust_with_boxed_broker(broker: Box<dyn Broker>) -> TrustFacade {
    let db = SqliteDatabase::new_in_memory();
    TrustFacade::new(Box::new(db), broker)
}

fn test_trust() -> TrustFacade {
    test_trust_with_broker::<DialogBroker>()
}

fn seed_account_with_trade(trust: &mut TrustFacade, name: &str) -> (model::Account, model::Trade) {
    let account = trust
        .create_account(name, "test", Environment::Paper, dec!(20), dec!(10))
        .expect("account");
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(10_000),
            &Currency::USD,
        )
        .expect("fund account");
    let vehicle = trust
        .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
        .expect("vehicle");
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: vehicle,
        quantity: 10.into(),
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };
    let trade = trust
        .create_trade(draft, dec!(95), dec!(100), dec!(110))
        .expect("trade");
    (account, trade)
}

fn seed_account_with_submitted_trade(
    trust: &mut TrustFacade,
    name: &str,
) -> (model::Account, model::Trade) {
    let (account, trade) = seed_account_with_trade(trust, name);
    let (funded_trade, _tx, _acc_balance, _trade_balance) =
        trust.fund_trade(&trade).expect("fund trade");
    let (submitted, _log) = trust.submit_trade(&funded_trade).expect("submit trade");
    (account, submitted)
}

fn create_submitted_trade_for_account(
    trust: &mut TrustFacade,
    account: &model::Account,
    symbol: &str,
) -> model::Trade {
    let vehicle = trust
        .create_trading_vehicle(symbol, None, &TradingVehicleCategory::Stock, "alpaca")
        .expect("vehicle");
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: vehicle,
        quantity: 10.into(),
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };
    let trade = trust
        .create_trade(draft, dec!(95), dec!(100), dec!(110))
        .expect("trade");
    let (funded_trade, _tx, _acc_balance, _trade_balance) =
        trust.fund_trade(&trade).expect("fund trade");
    let (submitted, _log) = trust.submit_trade(&funded_trade).expect("submit trade");
    submitted
}

#[test]
fn terminal_status_identifies_all_terminal_variants() {
    assert!(terminal_status(Status::ClosedTarget));
    assert!(terminal_status(Status::ClosedStopLoss));
    assert!(terminal_status(Status::Canceled));
    assert!(terminal_status(Status::Expired));
    assert!(terminal_status(Status::Rejected));
    assert!(!terminal_status(Status::Submitted));
    assert!(!terminal_status(Status::Filled));
}

#[test]
fn search_and_latest_fail_fast_without_account() {
    let mut trust = test_trust();

    let searched = TradeWatchDialogBuilder::new().search(&mut trust);
    let search_error = searched
        .result
        .expect("result must be set")
        .expect_err("search should fail without account");
    assert!(search_error.to_string().contains("No account selected"));

    let latest = TradeWatchDialogBuilder::new().latest(&mut trust);
    let latest_error = latest
        .result
        .expect("result must be set")
        .expect_err("latest should fail without account");
    assert!(latest_error.to_string().contains("No account selected"));
}

#[test]
fn latest_returns_not_found_when_no_open_trades_exist() {
    let mut trust = test_trust();
    let (account, _trade) = seed_account_with_trade(&mut trust, "watch-empty");

    let builder = TradeWatchDialogBuilder {
        account: Some(account),
        trade: None,
        result: None,
    }
    .latest(&mut trust);

    let error = builder
        .result
        .expect("result must be set")
        .expect_err("no submitted/filled trades should fail");
    assert!(error
        .to_string()
        .contains("No trade found with status Submitted / PartiallyFilled / Filled"));
}

#[test]
fn search_and_latest_store_open_trade_read_errors() {
    let mut trust = TrustFacade::new(
        Box::new(crate::test_support::ReadFailureFactory::trades()),
        Box::<DialogBroker>::default(),
    );
    let account = model::Account {
        id: Uuid::new_v4(),
        ..model::Account::default()
    };

    let searched = TradeWatchDialogBuilder {
        account: Some(account.clone()),
        trade: None,
        result: None,
    }
    .search(&mut trust);
    let search_error = searched
        .result
        .expect("result must be set")
        .expect_err("search should surface trade read failures");
    assert!(search_error.to_string().contains("trade read failed"));

    let latest = TradeWatchDialogBuilder {
        account: Some(account),
        trade: None,
        result: None,
    }
    .latest(&mut trust);
    let latest_error = latest
        .result
        .expect("result must be set")
        .expect_err("latest should surface trade read failures");
    assert!(latest_error.to_string().contains("trade read failed"));
}

#[test]
fn build_requires_trade_and_account() {
    let mut trust = test_trust();
    let missing_trade = TradeWatchDialogBuilder::new().build(&mut trust);
    let trade_error = missing_trade
        .result
        .expect("result must be set")
        .expect_err("missing trade should fail");
    assert!(trade_error.to_string().contains("No trade selected"));

    let (_, trade) = seed_account_with_trade(&mut trust, "watch-build");
    let missing_account = TradeWatchDialogBuilder {
        account: None,
        trade: Some(trade),
        result: None,
    }
    .build(&mut trust);
    let account_error = missing_account
        .result
        .expect("result must be set")
        .expect_err("missing account should fail");
    assert!(account_error.to_string().contains("No account selected"));
}

#[test]
fn watch_reports_missing_run_and_broker_failure() {
    crate::test_support::capture::assert_stdout(
        module_path!(),
        "watch_reports_missing_run_and_broker_failure",
        || {
            TradeWatchDialogBuilder::new().display();
            TradeWatchDialogBuilder {
                account: None,
                trade: None,
                result: Some(Err("synthetic watch failure".into())),
            }
            .display();
        },
        |output| {
            assert!(output.contains("No result found"), "{output}");
            assert!(output.contains("Trade watch error:"), "{output}");
            assert!(output.contains("synthetic watch failure"), "{output}");
        },
    );
}

#[test]
fn successful_watch_does_not_report_an_error() {
    crate::test_support::capture::assert_stdout(
        module_path!(),
        "successful_watch_does_not_report_an_error",
        || {
            TradeWatchDialogBuilder {
                account: None,
                trade: None,
                result: Some(Ok(())),
            }
            .display();
        },
        |output| {
            assert!(
                output.trim().is_empty(),
                "successful watch must not print a false error: {output}"
            );
        },
    );
}

#[test]
fn open_trades_for_account_returns_empty_when_no_open_statuses() {
    let mut trust = test_trust();
    let (account, _trade) = seed_account_with_trade(&mut trust, "watch-open");

    let trades = open_trades_for_account(&mut trust, account.id).expect("query should succeed");
    assert!(trades.is_empty());
}

#[test]
fn open_trades_for_account_propagates_each_status_query_failure() {
    for fail_status in [Status::Submitted, Status::Filled, Status::PartiallyFilled] {
        let account_id = Uuid::new_v4();
        let mut queried_statuses = Vec::new();
        let error = open_trades_for_account_with(account_id, |queried_account_id, status| {
            assert_eq!(queried_account_id, account_id);
            queried_statuses.push(status);
            if status == fail_status {
                Err(format!("trade read failed for {status:?}").into())
            } else {
                Ok(vec![])
            }
        })
        .expect_err("status query failure should be returned");

        assert!(error
            .to_string()
            .contains(&format!("trade read failed for {fail_status:?}")));
        assert_eq!(queried_statuses.last().copied(), Some(fail_status));
    }
}

#[test]
fn refresh_trade_snapshot_finds_and_misses_by_status_and_id() {
    let mut trust = test_trust();
    let (account, trade) = seed_account_with_trade(&mut trust, "watch-refresh");

    let found = refresh_trade_snapshot(&mut trust, account.id, trade.id, Status::New);
    assert!(found.is_some());
    assert_eq!(found.expect("trade").id, trade.id);

    let missing_status =
        refresh_trade_snapshot(&mut trust, account.id, trade.id, Status::Submitted);
    assert!(missing_status.is_none());

    let missing_id = refresh_trade_snapshot(&mut trust, account.id, Uuid::new_v4(), Status::New);
    assert!(missing_id.is_none());
}

#[test]
fn refresh_trade_snapshot_returns_none_when_status_query_fails() {
    let account_id = Uuid::new_v4();
    let trade_id = Uuid::new_v4();

    assert!(refresh_trade_snapshot_with(
        account_id,
        trade_id,
        Status::Submitted,
        |queried_account_id, status| {
            assert_eq!(queried_account_id, account_id);
            assert_eq!(status, Status::Submitted);
            Err("trade query failed".into())
        }
    )
    .is_none());
}

#[test]
fn wrapper_account_and_search_use_default_console_io_in_tests() {
    let mut trust = test_trust();
    let (account, _trade) = seed_account_with_trade(&mut trust, "watch-wrap");

    scripted_reset();
    scripted_push_select(Ok(Some(0)));
    let with_account = TradeWatchDialogBuilder::new().account(&mut trust);
    assert_eq!(
        with_account.account.as_ref().expect("selected account").id,
        account.id
    );

    let searched = with_account.search(&mut trust);
    let error = searched
        .result
        .expect("result should exist")
        .expect_err("no open trades should return error");
    assert!(error.to_string().contains("No trade found with status"));
    scripted_reset();
}

#[test]
fn search_with_io_selects_open_trade_when_present() {
    let mut trust = test_trust();
    let (account, submitted_trade) = seed_account_with_submitted_trade(&mut trust, "watch-open");

    scripted_reset();
    scripted_push_select(Ok(Some(0)));

    let selected = TradeWatchDialogBuilder {
        account: Some(account),
        trade: None,
        result: None,
    }
    .search(&mut trust);
    assert!(
        selected.result.is_none(),
        "search should succeed with open trade"
    );
    assert_eq!(
        selected.trade.as_ref().expect("selected trade").id,
        submitted_trade.id
    );

    scripted_reset();
}

#[test]
fn latest_selects_most_recent_open_trade() {
    let mut trust = test_trust();
    let (account, first_submitted) = seed_account_with_submitted_trade(&mut trust, "watch-latest");
    let second_submitted = create_submitted_trade_for_account(&mut trust, &account, "MSFT");

    let selected = TradeWatchDialogBuilder {
        account: Some(account),
        trade: Some(first_submitted),
        result: None,
    }
    .latest(&mut trust);

    assert!(selected.result.is_none(), "latest should succeed");
    assert_eq!(
        selected.trade.as_ref().expect("latest trade").id,
        second_submitted.id
    );
}

#[test]
fn watch_loop_stops_when_terminal_status_is_reported() {
    let mut trust = test_trust_with_boxed_broker(Box::new(DialogBroker {
        sync: SyncBehavior::Terminal,
        ..DialogBroker::default()
    }));
    let (account, trade) = seed_account_with_submitted_trade(&mut trust, "watch-terminal");

    let result = super::watch_loop(&mut trust, &trade, &account);
    assert!(result.is_ok(), "terminal status should stop watch loop");
}

#[test]
fn watch_loop_polls_until_terminal_and_displays_new_executions() {
    let scenario = DialogBroker {
        sync: SyncBehavior::SubmittedThenTerminal,
        ..DialogBroker::default()
    };
    let calls = scenario.sync_calls.clone();
    let mut trust = test_trust_with_boxed_broker(Box::new(scenario));
    let (account, trade) = seed_account_with_submitted_trade(&mut trust, "watch-poll");
    super::watch_loop(&mut trust, &trade, &account).unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "stop polling immediately after terminal confirmation"
    );
    let closed = trust.read_trade(trade.id).unwrap();
    assert_eq!(closed.status, Status::ClosedTarget);
    assert_eq!(closed.balance.total_performance, dec!(100));
    assert_eq!(closed.balance.capital_in_market, dec!(0));
    assert_eq!(closed.safety_stop.status, OrderStatus::Canceled);
    let mut fills: Vec<_> = trust
        .executions_for_trade(trade.id)
        .unwrap()
        .into_iter()
        .map(|fill| (fill.qty, fill.price))
        .collect();
    fills.sort();
    assert_eq!(fills, vec![(dec!(10), dec!(100)), (dec!(10), dec!(110))]);
}

#[test]
fn build_runs_watch_loop_when_account_and_trade_are_present() {
    let mut trust = test_trust_with_boxed_broker(Box::new(DialogBroker {
        sync: SyncBehavior::Terminal,
        ..DialogBroker::default()
    }));
    let (account, trade) = seed_account_with_submitted_trade(&mut trust, "watch-build-loop");

    let builder = TradeWatchDialogBuilder {
        account: Some(account),
        trade: Some(trade),
        result: None,
    }
    .build(&mut trust);

    assert!(builder.result.expect("watch result should be set").is_ok());
}

#[test]
fn watch_loop_propagates_sync_error() {
    let mut trust = test_trust_with_boxed_broker(Box::new(DialogBroker {
        sync: SyncBehavior::Failing,
        ..DialogBroker::default()
    }));
    let (account, trade) = seed_account_with_submitted_trade(&mut trust, "watch-sync-error");

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    let receipts = trust.get_account_transactions(account.id).unwrap();
    let result = super::watch_loop(&mut trust, &trade, &account);
    let error = result.expect_err("sync failures must be propagated");
    assert!(error.to_string().contains("sync failed"));

    assert_eq!(trust.read_trade(trade.id).unwrap(), trade);
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        balance
    );
    assert_eq!(
        trust.get_account_transactions(account.id).unwrap(),
        receipts
    );
    assert!(trust.executions_for_trade(trade.id).unwrap().is_empty());
}
