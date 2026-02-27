//! UI Dialog Module - Trade Watch (polling-based)
//!
//! TEMPORARY SAFETY ALLOWANCE: This dialog module contains user interaction code.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use crate::dialogs::{dialog_helpers, AccountSearchDialog, ConsoleDialogIo};
use crate::views::{ExecutionView, OrderView};
use broker_sync::{BrokerCommand, BrokerEvent, BrokerSync};
use core::TrustFacade;
use model::{Account, Execution, Status, Trade};
use std::collections::HashSet;
use std::error::Error;
use std::io::ErrorKind;
use std::thread::sleep;
use std::time::Duration;

type WatchResult = Option<Result<(), Box<dyn Error>>>;

pub struct TradeWatchDialogBuilder {
    account: Option<Account>,
    trade: Option<Trade>,
    result: WatchResult,
}

impl TradeWatchDialogBuilder {
    pub fn new() -> Self {
        Self {
            account: None,
            trade: None,
            result: None,
        }
    }

    pub fn account(mut self, trust: &mut TrustFacade) -> Self {
        let account = AccountSearchDialog::new().search(trust).build();
        match account {
            Ok(account) => self.account = Some(account),
            Err(error) => println!("Error searching account: {error:?}"),
        }
        self
    }

    pub fn search(mut self, trust: &mut TrustFacade) -> Self {
        let account = match dialog_helpers::require(
            self.account.clone(),
            ErrorKind::InvalidInput,
            "No account selected",
        ) {
            Ok(account) => account,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        let trades = match open_trades_for_account(trust, account.id) {
            Ok(trades) => trades,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        let mut io = ConsoleDialogIo::default();
        match dialog_helpers::select_from_list(
            &mut io,
            "Trade to watch:",
            &trades,
            "No trade found with status Submitted / PartiallyFilled / Filled",
            "Trade selection was canceled",
        ) {
            Ok(trade) => self.trade = Some(trade),
            Err(error) => self.result = Some(Err(error)),
        }

        self
    }

    pub fn latest(mut self, trust: &mut TrustFacade) -> Self {
        let account = match dialog_helpers::require(
            self.account.clone(),
            ErrorKind::InvalidInput,
            "No account selected",
        ) {
            Ok(account) => account,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };
        let mut trades = match open_trades_for_account(trust, account.id) {
            Ok(trades) => trades,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        trades.sort_by_key(|trade| trade.updated_at);
        let trade = match trades.pop() {
            Some(trade) => trade,
            None => {
                self.result = Some(Err(Box::new(std::io::Error::new(
                    ErrorKind::NotFound,
                    "No trade found with status Submitted / PartiallyFilled / Filled",
                ))));
                return self;
            }
        };
        self.trade = Some(trade);
        self
    }

    pub fn build(mut self, trust: &mut TrustFacade) -> Self {
        let trade = match dialog_helpers::require(
            self.trade.clone(),
            ErrorKind::InvalidInput,
            "No trade selected",
        ) {
            Ok(trade) => trade,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };
        let account = match dialog_helpers::require(
            self.account.clone(),
            ErrorKind::InvalidInput,
            "No account selected",
        ) {
            Ok(account) => account,
            Err(error) => {
                self.result = Some(Err(error));
                return self;
            }
        };

        self.result = Some(watch_loop(trust, &trade, &account));
        self
    }

    pub fn display(self) {
        match self.result {
            Some(Ok(())) => {}
            Some(Err(error)) => println!("Trade watch error: {error:?}"),
            None => println!("No result found, did you forget to call build()?"),
        }
    }
}

fn open_trades_for_account(
    trust: &mut TrustFacade,
    account_id: uuid::Uuid,
) -> Result<Vec<Trade>, Box<dyn Error>> {
    let mut trades = trust.search_trades(account_id, Status::Submitted)?;
    trades.append(&mut trust.search_trades(account_id, Status::Filled)?);
    trades.append(&mut trust.search_trades(account_id, Status::PartiallyFilled)?);
    Ok(trades)
}

fn terminal_status(status: Status) -> bool {
    matches!(
        status,
        Status::ClosedTarget
            | Status::ClosedStopLoss
            | Status::Canceled
            | Status::Expired
            | Status::Rejected
    )
}

fn watch_loop(
    trust: &mut TrustFacade,
    trade: &Trade,
    account: &Account,
) -> Result<(), Box<dyn Error>> {
    let session_actor = BrokerSync::spawn();
    session_actor.send(BrokerCommand::StartTradeSession {
        account_id: account.id,
        trade_id: trade.id,
    })?;
    let _ = session_actor.recv_timeout(Duration::from_millis(200));

    let mut seen_execution_ids: HashSet<String> = HashSet::new();
    let mut last_status: Option<Status> = None;

    println!("Watching trade {} (Ctrl-C to stop)", trade.id);
    let mut current_trade = trade.clone();

    loop {
        let (status, orders, _log) = trust.sync_trade(&current_trade, account)?;
        let _ = session_actor.send(BrokerCommand::TouchTradeSession { trade_id: trade.id });
        current_trade = refresh_trade_snapshot(trust, account.id, trade.id, status)
            .unwrap_or_else(|| current_trade.clone());

        if last_status != Some(status) {
            println!();
            println!("Status: {status:?}");
            last_status = Some(status);
        }

        if !orders.is_empty() {
            println!();
            println!("Updated orders:");
            OrderView::display_orders(orders);
        }

        let executions: Vec<Execution> = trust.executions_for_trade(current_trade.id)?;
        let new_execs: Vec<Execution> = executions
            .into_iter()
            .filter(|e| seen_execution_ids.insert(e.broker_execution_id.clone()))
            .collect();
        if !new_execs.is_empty() {
            println!();
            println!("New executions:");
            ExecutionView::display(&new_execs);
        }

        if terminal_status(status) {
            println!();
            println!("Trade reached terminal status: {status:?}");
            let _ = session_actor.send(BrokerCommand::StopTradeSession { trade_id: trade.id });
            if let Ok(BrokerEvent::TradeSessionStopped { .. }) =
                session_actor.recv_timeout(Duration::from_millis(200))
            {
                println!("Managed session stopped.");
            }
            let _ = session_actor.send(BrokerCommand::Shutdown);
            break;
        }

        sleep(Duration::from_secs(2));
    }

    Ok(())
}

fn refresh_trade_snapshot(
    trust: &mut TrustFacade,
    account_id: uuid::Uuid,
    trade_id: uuid::Uuid,
    status: Status,
) -> Option<Trade> {
    trust
        .search_trades(account_id, status)
        .ok()?
        .into_iter()
        .find(|trade| trade.id == trade_id)
}

#[cfg(test)]
mod tests {
    use super::{
        open_trades_for_account, refresh_trade_snapshot, terminal_status, TradeWatchDialogBuilder,
    };
    use crate::dialogs::io::{scripted_push_select, scripted_reset};
    use core::TrustFacade;
    use db_sqlite::SqliteDatabase;
    use model::{
        Broker, BrokerLog, Currency, DraftTrade, Environment, Order, OrderIds, OrderStatus, Status,
        TradeCategory, TradingVehicleCategory, TransactionCategory,
    };
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    #[derive(Default)]
    struct TestBroker;

    impl Broker for TestBroker {
        fn submit_trade(
            &self,
            _trade: &model::Trade,
            _account: &model::Account,
        ) -> Result<(BrokerLog, OrderIds), Box<dyn std::error::Error>> {
            Ok((
                BrokerLog::default(),
                OrderIds {
                    stop: Uuid::new_v4(),
                    entry: Uuid::new_v4(),
                    target: Uuid::new_v4(),
                },
            ))
        }

        fn sync_trade(
            &self,
            _trade: &model::Trade,
            _account: &model::Account,
        ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn std::error::Error>> {
            Ok((Status::Submitted, vec![], BrokerLog::default()))
        }

        fn close_trade(
            &self,
            _trade: &model::Trade,
            _account: &model::Account,
        ) -> Result<(Order, BrokerLog), Box<dyn std::error::Error>> {
            Err("not implemented in dialog tests".into())
        }

        fn cancel_trade(
            &self,
            _trade: &model::Trade,
            _account: &model::Account,
        ) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }

        fn modify_stop(
            &self,
            _trade: &model::Trade,
            _account: &model::Account,
            _new_stop_price: rust_decimal::Decimal,
        ) -> Result<Uuid, Box<dyn std::error::Error>> {
            Ok(Uuid::new_v4())
        }

        fn modify_target(
            &self,
            _trade: &model::Trade,
            _account: &model::Account,
            _new_price: rust_decimal::Decimal,
        ) -> Result<Uuid, Box<dyn std::error::Error>> {
            Ok(Uuid::new_v4())
        }
    }

    fn test_trust_with_broker<B: Broker + Default + 'static>() -> TrustFacade {
        let db = SqliteDatabase::new_in_memory();
        TrustFacade::new(Box::new(db), Box::<B>::default())
    }

    fn test_trust_with_boxed_broker(broker: Box<dyn Broker>) -> TrustFacade {
        let db = SqliteDatabase::new_in_memory();
        TrustFacade::new(Box::new(db), broker)
    }

    fn test_trust() -> TrustFacade {
        test_trust_with_broker::<TestBroker>()
    }

    fn seed_account_with_trade(
        trust: &mut TrustFacade,
        name: &str,
    ) -> (model::Account, model::Trade) {
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
            quantity: 10,
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
            quantity: 10,
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

    struct ScenarioBroker {
        fail_sync: bool,
    }

    impl ScenarioBroker {
        fn terminal() -> Self {
            Self { fail_sync: false }
        }

        fn failing_sync() -> Self {
            Self { fail_sync: true }
        }
    }

    impl Broker for ScenarioBroker {
        fn submit_trade(
            &self,
            _trade: &model::Trade,
            _account: &model::Account,
        ) -> Result<(BrokerLog, OrderIds), Box<dyn std::error::Error>> {
            Ok((
                BrokerLog::default(),
                OrderIds {
                    stop: Uuid::new_v4(),
                    entry: Uuid::new_v4(),
                    target: Uuid::new_v4(),
                },
            ))
        }

        fn sync_trade(
            &self,
            trade: &model::Trade,
            _account: &model::Account,
        ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn std::error::Error>> {
            if self.fail_sync {
                return Err("sync failed".into());
            }
            let mut entry = trade.entry.clone();
            entry.status = OrderStatus::Filled;
            entry.average_filled_price = Some(dec!(100));
            let mut target = trade.target.clone();
            target.status = OrderStatus::Filled;
            target.average_filled_price = Some(dec!(110));
            let mut stop = trade.safety_stop.clone();
            stop.status = OrderStatus::Canceled;

            Ok((
                Status::ClosedTarget,
                vec![entry, target, stop],
                BrokerLog::default(),
            ))
        }

        fn close_trade(
            &self,
            _trade: &model::Trade,
            _account: &model::Account,
        ) -> Result<(Order, BrokerLog), Box<dyn std::error::Error>> {
            Err("not used".into())
        }

        fn cancel_trade(
            &self,
            _trade: &model::Trade,
            _account: &model::Account,
        ) -> Result<(), Box<dyn std::error::Error>> {
            Ok(())
        }

        fn modify_stop(
            &self,
            _trade: &model::Trade,
            _account: &model::Account,
            _new_stop_price: rust_decimal::Decimal,
        ) -> Result<Uuid, Box<dyn std::error::Error>> {
            Ok(Uuid::new_v4())
        }

        fn modify_target(
            &self,
            _trade: &model::Trade,
            _account: &model::Account,
            _new_price: rust_decimal::Decimal,
        ) -> Result<Uuid, Box<dyn std::error::Error>> {
            Ok(Uuid::new_v4())
        }
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
    fn new_starts_with_empty_state() {
        let builder = TradeWatchDialogBuilder::new();
        assert!(builder.account.is_none());
        assert!(builder.trade.is_none());
        assert!(builder.result.is_none());
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
    fn display_covers_none_and_error_paths() {
        TradeWatchDialogBuilder::new().display();
        TradeWatchDialogBuilder {
            account: None,
            trade: None,
            result: Some(Err("synthetic watch failure".into())),
        }
        .display();
    }

    #[test]
    fn open_trades_for_account_returns_empty_when_no_open_statuses() {
        let mut trust = test_trust();
        let (account, _trade) = seed_account_with_trade(&mut trust, "watch-open");

        let trades = open_trades_for_account(&mut trust, account.id).expect("query should succeed");
        assert!(trades.is_empty());
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

        let missing_id =
            refresh_trade_snapshot(&mut trust, account.id, Uuid::new_v4(), Status::New);
        assert!(missing_id.is_none());
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
        let (account, submitted_trade) =
            seed_account_with_submitted_trade(&mut trust, "watch-open");

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
        let (account, first_submitted) =
            seed_account_with_submitted_trade(&mut trust, "watch-latest");
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
        let mut trust = test_trust_with_boxed_broker(Box::new(ScenarioBroker::terminal()));
        let (account, trade) = seed_account_with_submitted_trade(&mut trust, "watch-terminal");

        let result = super::watch_loop(&mut trust, &trade, &account);
        assert!(result.is_ok(), "terminal status should stop watch loop");
    }

    #[test]
    fn watch_loop_propagates_sync_error() {
        let mut trust = test_trust_with_boxed_broker(Box::new(ScenarioBroker::failing_sync()));
        let (account, trade) = seed_account_with_submitted_trade(&mut trust, "watch-sync-error");

        let result = super::watch_loop(&mut trust, &trade, &account);
        let error = result.expect_err("sync failures must be propagated");
        assert!(error.to_string().contains("sync failed"));
    }

    #[test]
    fn scenario_broker_aux_methods_have_expected_contracts() {
        let broker = ScenarioBroker::terminal();
        let mut trust = test_trust();
        let (account, trade) =
            seed_account_with_submitted_trade(&mut trust, "watch-broker-contract");

        assert!(broker.cancel_trade(&trade, &account).is_ok());
        assert!(broker.modify_stop(&trade, &account, dec!(90)).is_ok());
        assert!(broker.modify_target(&trade, &account, dec!(120)).is_ok());
        let close_error = broker
            .close_trade(&trade, &account)
            .expect_err("close should remain unsupported in this test broker");
        assert!(close_error.to_string().contains("not used"));
    }
}
