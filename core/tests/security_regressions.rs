use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Account, Broker, BrokerKind, BrokerLog, Currency, DatabaseFactory, DraftTrade, Environment,
    Order, OrderIds, Status, Trade, TradeCategory, TradingVehicleCategory, TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;

struct NoopBroker;

impl Broker for NoopBroker {
    fn kind(&self) -> BrokerKind {
        BrokerKind::Alpaca
    }

    fn submit_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        Ok((
            BrokerLog::default(),
            OrderIds {
                stop: "stop".to_string(),
                entry: "entry".to_string(),
                target: "target".to_string(),
            },
        ))
    }

    fn sync_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        Ok((Status::Submitted, Vec::new(), BrokerLog::default()))
    }

    fn close_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        Ok((trade.target.clone(), BrokerLog::default()))
    }

    fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn modify_stop(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_stop_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        Ok("stop".to_string())
    }

    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        Ok("target".to_string())
    }
}

fn seed(facade: &mut TrustFacade, side: TradeCategory) -> (Account, Trade) {
    let account = facade
        .create_account("audit", "audit", Environment::Paper, dec!(0), dec!(0))
        .unwrap();
    facade
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(10000),
            &Currency::USD,
        )
        .unwrap();
    let vehicle = facade
        .create_trading_vehicle("AUDIT", None, &TradingVehicleCategory::Stock, "ibkr")
        .unwrap();
    let (stop, target) = if side == TradeCategory::Long {
        (dec!(90), dec!(120))
    } else {
        (dec!(110), dec!(80))
    };
    let trade = facade
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity: dec!(10),
                currency: Currency::USD,
                category: side,
                thesis: None,
                sector: None,
                asset_class: None,
                context: None,
            },
            stop,
            dec!(100),
            target,
        )
        .unwrap();
    (account, trade)
}
fn facade() -> TrustFacade {
    TrustFacade::new(
        Box::new(SqliteDatabase::new_in_memory()),
        Box::new(NoopBroker),
    )
}

#[test]
fn short_profit_has_correct_cash_direction() {
    let mut f = facade();
    let (account, t) = seed(&mut f, TradeCategory::Short);
    let (mut funded, _, _, _) = f.fund_trade(&t).unwrap();
    funded.entry.average_filled_price = Some(dec!(100));
    let (mut filled, _) = f.fill_trade(&funded, dec!(0)).unwrap();
    filled.target.average_filled_price = Some(dec!(80));
    f.target_acquired(&filled, dec!(0)).unwrap();
    let closed = f.read_trade(t.id).unwrap();
    let balance = f.search_balance(account.id, &Currency::USD).unwrap();
    println!(
        "short sell 10 at 100 buy back at 80: pnl={}, account={}",
        closed.balance.total_performance, balance.total_available
    );
    assert_eq!(closed.balance.total_performance, dec!(200));
    assert_eq!(balance.total_available, dec!(10200));
}
#[test]
fn stale_fill_replay_is_rejected() {
    let mut f = facade();
    let (account, t) = seed(&mut f, TradeCategory::Long);
    let (mut funded, _, _, _) = f.fund_trade(&t).unwrap();
    funded.entry.average_filled_price = Some(dec!(100));
    f.fill_trade(&funded, dec!(0)).unwrap();
    let replay = f.fill_trade(&funded, dec!(0));
    assert!(replay.is_err());
    let persisted = f.read_trade(t.id).unwrap();
    let balance = f.search_balance(account.id, &Currency::USD).unwrap();
    println!(
        "stale fill replay accepted: capital_in_market={}, account balance={}",
        persisted.balance.capital_in_market, balance.total_balance
    );
    assert_eq!(persisted.balance.capital_in_market, dec!(1000));
}
#[test]
fn transfer_cannot_overspend() {
    let mut f = facade();
    let (account, _) = seed(&mut f, TradeCategory::Long);
    let child = f
        .create_account_with_hierarchy(
            "child",
            "audit",
            Environment::Paper,
            dec!(0),
            dec!(0),
            model::AccountType::Earnings,
            Some(account.id),
        )
        .unwrap();
    f.transfer_between_accounts(account.id, child.id, dec!(15000), Currency::USD, "audit")
        .unwrap_err();
    let balance = f.search_balance(account.id, &Currency::USD).unwrap();
    println!(
        "transferred 15000 from 10000 account; cached available={}",
        balance.total_available
    );
    assert_eq!(balance.total_available, dec!(10000));
}
#[test]
fn funding_failure_rolls_back_status() {
    use diesel::{Connection, RunQueryDsl};
    let path = std::env::temp_dir().join(format!("trust-audit-{}.db", uuid::Uuid::new_v4()));
    let url = path.to_str().unwrap();
    let mut f = TrustFacade::new(Box::new(SqliteDatabase::new(url)), Box::new(NoopBroker));
    let (_, t) = seed(&mut f, TradeCategory::Long);
    let mut conn = diesel::SqliteConnection::establish(url).unwrap();
    diesel::sql_query("CREATE TRIGGER audit_fail_funding BEFORE INSERT ON transactions WHEN NEW.category = 'fund_trade' BEGIN SELECT RAISE(ABORT, 'audit injected failure'); END").execute(&mut conn).unwrap();
    assert!(f.fund_trade(&t).is_err());
    let persisted = f.read_trade(t.id).unwrap();
    println!(
        "failed funding status={}, funded amount={}",
        persisted.status, persisted.balance.funding
    );
    assert_eq!(persisted.status, Status::New);
    assert_eq!(persisted.balance.funding, dec!(0));
}
#[test]
fn short_funding_rejects_rule_read_failure() {
    use diesel::{Connection, RunQueryDsl};
    let path = std::env::temp_dir().join(format!("trust-audit-{}.db", uuid::Uuid::new_v4()));
    let url = path.to_str().unwrap();
    let mut f = TrustFacade::new(Box::new(SqliteDatabase::new(url)), Box::new(NoopBroker));
    let (account, t) = seed(&mut f, TradeCategory::Short);
    f.create_rule(
        &account,
        &model::RuleName::RiskPerTrade(0.01),
        "tight cap",
        &model::RuleLevel::Error,
    )
    .unwrap();
    assert!(f.fund_trade(&t).is_err());
    let mut conn = diesel::SqliteConnection::establish(url).unwrap();
    diesel::sql_query("DROP TABLE rules")
        .execute(&mut conn)
        .unwrap();
    assert!(f.fund_trade(&t).is_err());
    println!(
        "short funding rejected with configured cap, also rejected after rules became unreadable"
    );
}

#[derive(Clone)]
struct FillScenario {
    entry_qty: Decimal,
    exit_qty: Decimal,
    canceled: bool,
    fees: Vec<(String, Decimal)>,
    stop_price: Option<Decimal>,
    target_price: Option<Decimal>,
}
struct ReplayBroker(std::sync::Arc<std::sync::Mutex<FillScenario>>);
impl Broker for ReplayBroker {
    fn kind(&self) -> BrokerKind {
        BrokerKind::Alpaca
    }
    fn submit_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        NoopBroker.submit_trade(trade, account)
    }
    fn cancel_trade(&self, trade: &Trade, account: &Account) -> Result<(), Box<dyn Error>> {
        NoopBroker.cancel_trade(trade, account)
    }
    fn close_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        NoopBroker.close_trade(trade, account)
    }
    fn modify_stop(
        &self,
        _trade: &Trade,
        _account: &Account,
        _price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        Err("simulated lost acknowledgement".into())
    }
    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &Account,
        _price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        Err("simulated lost target acknowledgement".into())
    }
    fn fetch_fee_activities(
        &self,
        trade: &Trade,
        account: &Account,
        after: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<model::FeeActivity>, Box<dyn Error>> {
        assert!(
            after.is_none(),
            "Fee retrieval must not use the execution cursor"
        );
        Ok(self
            .0
            .lock()
            .unwrap()
            .fees
            .iter()
            .map(|(id, amount)| model::FeeActivity {
                broker: "alpaca".into(),
                account_id: account.id,
                broker_activity_id: id.clone(),
                broker_order_id: trade.entry.broker_order_id.clone(),
                symbol: Some("AUDIT".into()),
                activity_type: "FEE".into(),
                amount: *amount,
                occurred_at: trade.created_at,
                raw_json: None,
            })
            .collect())
    }
    fn sync_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        let scenario = self.0.lock().unwrap().clone();
        let mut entry = trade.entry.clone();
        entry.filled_quantity = scenario.entry_qty;
        entry.average_filled_price = Some(dec!(100));
        entry.status = if scenario.canceled {
            model::OrderStatus::Canceled
        } else if scenario.entry_qty == dec!(10) {
            model::OrderStatus::Filled
        } else {
            model::OrderStatus::PartiallyFilled
        };
        let mut target = trade.target.clone();
        target.unit_price = scenario.target_price.unwrap_or(target.unit_price);
        target.filled_quantity = scenario.exit_qty;
        target.average_filled_price = Some(trade.target.unit_price);
        let status = if scenario.exit_qty > Decimal::ZERO {
            target.quantity = scenario.exit_qty;
            target.status = model::OrderStatus::Filled;
            Status::ClosedTarget
        } else if scenario.canceled {
            Status::Canceled
        } else if scenario.entry_qty == dec!(10) {
            Status::Filled
        } else {
            Status::PartiallyFilled
        };
        let mut orders = vec![entry, target];
        if let Some(price) = scenario.stop_price {
            let mut stop = trade.safety_stop.clone();
            stop.unit_price = price;
            orders.push(stop);
        }
        Ok((status, orders, BrokerLog::default()))
    }
}

#[test]
fn partial_cancel_retains_exposure_and_later_exit_reconciles_once() {
    let scenario = std::sync::Arc::new(std::sync::Mutex::new(FillScenario {
        entry_qty: dec!(4),
        exit_qty: dec!(0),
        canceled: false,
        fees: Vec::new(),
        stop_price: None,
        target_price: None,
    }));
    let mut facade = TrustFacade::new(
        Box::new(SqliteDatabase::new_in_memory()),
        Box::new(ReplayBroker(scenario.clone())),
    );
    let (account, trade) = seed(&mut facade, TradeCategory::Long);
    let (funded, _, _, _) = facade.fund_trade(&trade).unwrap();
    let (submitted, _) = facade.submit_trade(&funded).unwrap();
    facade.sync_trade(&submitted, &account).unwrap();
    let partial = facade.read_trade(trade.id).unwrap();
    assert_eq!(partial.status, Status::PartiallyFilled);
    assert_eq!(partial.balance.capital_in_market, dec!(400));
    assert_eq!(partial.balance.capital_out_market, dec!(600));
    assert_eq!(
        facade
            .search_balance(account.id, &Currency::USD)
            .unwrap()
            .total_available,
        dec!(9000)
    );
    scenario.lock().unwrap().canceled = true;
    facade.sync_trade(&partial, &account).unwrap();
    let canceled = facade.read_trade(trade.id).unwrap();
    assert_eq!(canceled.status, Status::PartiallyFilled);
    assert_eq!(canceled.balance.capital_in_market, dec!(400));
    assert_eq!(canceled.balance.capital_out_market, dec!(0));
    assert_eq!(
        facade
            .search_balance(account.id, &Currency::USD)
            .unwrap()
            .total_available,
        dec!(9600)
    );
    scenario.lock().unwrap().exit_qty = dec!(4);
    facade.sync_trade(&canceled, &account).unwrap();
    let closed = facade.read_trade(trade.id).unwrap();
    assert_eq!(closed.status, Status::ClosedTarget);
    assert_eq!(closed.balance.capital_in_market, dec!(0));
    assert_eq!(closed.balance.total_performance, dec!(80));
    facade.sync_trade(&closed, &account).unwrap();
    assert_eq!(
        facade
            .search_balance(account.id, &Currency::USD)
            .unwrap()
            .total_available,
        dec!(10080)
    );
}

#[test]
fn completed_exit_with_remaining_position_does_not_cancel_protection() {
    let scenario = std::sync::Arc::new(std::sync::Mutex::new(FillScenario {
        entry_qty: dec!(10),
        exit_qty: dec!(4),
        canceled: false,
        fees: Vec::new(),
        stop_price: None,
        target_price: None,
    }));
    let mut facade = TrustFacade::new(
        Box::new(SqliteDatabase::new_in_memory()),
        Box::new(ReplayBroker(scenario)),
    );
    let (account, trade) = seed(&mut facade, TradeCategory::Long);
    let (funded, _, _, _) = facade.fund_trade(&trade).unwrap();
    let (submitted, _) = facade.submit_trade(&funded).unwrap();
    facade.sync_trade(&submitted, &account).unwrap();
    let partial = facade.read_trade(trade.id).unwrap();
    assert_eq!(partial.status, Status::PartiallyFilled);
    assert_eq!(partial.balance.capital_in_market, dec!(600));
    assert_ne!(partial.safety_stop.status, model::OrderStatus::Canceled);
    assert_eq!(partial.safety_stop.status, submitted.safety_stop.status);
}

#[test]
fn delayed_fee_batches_are_booked_once_by_activity_identity() {
    let scenario = std::sync::Arc::new(std::sync::Mutex::new(FillScenario {
        entry_qty: dec!(10),
        exit_qty: dec!(0),
        canceled: false,
        fees: vec![("fee-a".into(), dec!(0.5))],
        stop_price: None,
        target_price: None,
    }));
    let mut facade = TrustFacade::new(
        Box::new(SqliteDatabase::new_in_memory()),
        Box::new(ReplayBroker(scenario.clone())),
    );
    let (account, trade) = seed(&mut facade, TradeCategory::Long);
    let (funded, _, _, _) = facade.fund_trade(&trade).unwrap();
    let (submitted, _) = facade.submit_trade(&funded).unwrap();
    facade.sync_trade(&submitted, &account).unwrap();
    scenario.lock().unwrap().fees =
        vec![("fee-b".into(), dec!(0.75)), ("fee-b".into(), dec!(0.75))];
    let filled = facade.read_trade(trade.id).unwrap();
    facade.sync_trade(&filled, &account).unwrap();
    facade.sync_trade(&filled, &account).unwrap();
    assert_eq!(
        facade
            .search_balance(account.id, &Currency::USD)
            .unwrap()
            .total_available,
        dec!(8998.75)
    );
}

#[test]
fn unknown_stop_change_blocks_retry_until_observed_price_is_reconciled() {
    let scenario = std::sync::Arc::new(std::sync::Mutex::new(FillScenario {
        entry_qty: dec!(10),
        exit_qty: dec!(0),
        canceled: false,
        fees: Vec::new(),
        stop_price: None,
        target_price: None,
    }));
    let mut facade = TrustFacade::new(
        Box::new(SqliteDatabase::new_in_memory()),
        Box::new(ReplayBroker(scenario.clone())),
    );
    let (account, trade) = seed(&mut facade, TradeCategory::Long);
    let (funded, _, _, _) = facade.fund_trade(&trade).unwrap();
    let (submitted, _) = facade.submit_trade(&funded).unwrap();
    facade.sync_trade(&submitted, &account).unwrap();
    let filled = facade.read_trade(trade.id).unwrap();
    assert!(facade
        .modify_stop(&filled, &account, dec!(95))
        .unwrap_err()
        .to_string()
        .contains("lost acknowledgement"));
    assert!(facade
        .modify_stop(&filled, &account, dec!(92))
        .unwrap_err()
        .to_string()
        .contains("unresolved"));
    scenario.lock().unwrap().stop_price = Some(dec!(95));
    facade.sync_trade(&filled, &account).unwrap();
    assert_eq!(
        facade.read_trade(trade.id).unwrap().safety_stop.unit_price,
        dec!(95)
    );
    assert!(facade
        .modify_stop(&filled, &account, dec!(92))
        .unwrap_err()
        .to_string()
        .contains("risking more"));
}

struct TargetGate {
    entered: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

#[test]
fn reconciliation_keeps_stop_and_target_intents_independent() {
    for observe_together in [false, true] {
        let scenario = std::sync::Arc::new(std::sync::Mutex::new(FillScenario {
            entry_qty: dec!(10),
            exit_qty: dec!(0),
            canceled: false,
            fees: Vec::new(),
            stop_price: None,
            target_price: None,
        }));
        let mut facade = TrustFacade::new(
            Box::new(SqliteDatabase::new_in_memory()),
            Box::new(ReplayBroker(scenario.clone())),
        );
        let (account, trade) = seed(&mut facade, TradeCategory::Long);
        let (funded, _, _, _) = facade.fund_trade(&trade).unwrap();
        let (submitted, _) = facade.submit_trade(&funded).unwrap();
        facade.sync_trade(&submitted, &account).unwrap();
        let filled = facade.read_trade(trade.id).unwrap();
        assert!(facade
            .modify_stop(&filled, &account, dec!(95))
            .unwrap_err()
            .to_string()
            .contains("lost acknowledgement"));
        assert!(facade
            .modify_target(&filled, &account, dec!(125))
            .unwrap_err()
            .to_string()
            .contains("lost target acknowledgement"));
        {
            let mut snapshot = scenario.lock().unwrap();
            snapshot.stop_price = Some(dec!(95));
            if observe_together {
                snapshot.target_price = Some(dec!(125));
            }
        }
        facade.sync_trade(&filled, &account).unwrap();
        if !observe_together {
            let events = facade.trade_audit_events(trade.id).unwrap();
            assert_eq!(
                events
                    .iter()
                    .filter(
                        |event| event.event_type == "order.modify" && event.outcome == "reconciled"
                    )
                    .count(),
                1
            );
            assert!(facade
                .modify_target(&filled, &account, dec!(126))
                .unwrap_err()
                .to_string()
                .contains("unresolved"));
            scenario.lock().unwrap().target_price = Some(dec!(125));
            let current = facade.read_trade(trade.id).unwrap();
            facade.sync_trade(&current, &account).unwrap();
        }
        let current = facade.read_trade(trade.id).unwrap();
        facade.sync_trade(&current, &account).unwrap();
        let events = facade.trade_audit_events(trade.id).unwrap();
        for role in ["safety_stop", "target"] {
            let reconciled: Vec<_> = events
                .iter()
                .filter(|event| {
                    event.event_type == "order.modify"
                        && event.outcome == "reconciled"
                        && event.details["order_role"] == role
                })
                .collect();
            assert_eq!(reconciled.len(), 1, "{role} must reconcile exactly once");
            assert!(events.iter().any(|event| event.outcome == "attempted"
                && event.operation_id == reconciled[0].operation_id
                && event.details["order_role"] == role));
        }
        assert_eq!(current.safety_stop.unit_price, dec!(95));
        assert_eq!(current.target.unit_price, dec!(125));
        assert_eq!(
            facade
                .search_balance(account.id, &Currency::USD)
                .unwrap()
                .total_available,
            dec!(9000)
        );
        facade.verify_trade_audit_chain(trade.id).unwrap();
    }
}

struct CountingBroker(
    std::sync::Arc<std::sync::atomic::AtomicUsize>,
    Option<String>,
    Option<TargetGate>,
);

impl CountingBroker {
    fn persist_newer_fill(&self, trade: &Trade, role: &str) {
        if let Some(url) = &self.1 {
            let database = SqliteDatabase::new(url);
            let current = database.trade_read().read_trade(trade.id).unwrap();
            let mut order = match role {
                "target" => current.target.clone(),
                "stop" => current.safety_stop.clone(),
                _ => current.entry.clone(),
            };
            order.status = model::OrderStatus::Filled;
            order.filled_quantity = order.quantity;
            order.average_filled_price = Some(order.unit_price);
            order.broker_order_id = Some("newer-fill".into());
            database.order_write().update(&order).unwrap();
            let status = match role {
                "target" => Status::ClosedTarget,
                "stop" => Status::ClosedStopLoss,
                _ => Status::Filled,
            };
            database
                .trade_write()
                .update_trade_status(status, &current)
                .unwrap();
        }
    }
}
impl Broker for CountingBroker {
    fn kind(&self) -> BrokerKind {
        BrokerKind::Alpaca
    }
    fn submit_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.persist_newer_fill(trade, "entry");
        std::thread::sleep(std::time::Duration::from_millis(30));
        NoopBroker.submit_trade(trade, account)
    }
    fn sync_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        NoopBroker.sync_trade(trade, account)
    }
    fn cancel_trade(&self, trade: &Trade, account: &Account) -> Result<(), Box<dyn Error>> {
        assert_eq!(account.id, trade.account_id);
        assert_eq!(account.environment, Environment::Paper);
        NoopBroker.cancel_trade(trade, account)
    }
    fn close_trade(
        &self,
        trade: &Trade,
        account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        self.persist_newer_fill(trade, "target");
        NoopBroker.close_trade(trade, account)
    }
    fn modify_stop(
        &self,
        trade: &Trade,
        account: &Account,
        price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        assert_eq!(account.broker_kind, BrokerKind::Alpaca);
        assert_eq!(account.environment, Environment::Paper);
        assert_eq!(account.broker_account_id, None);
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.persist_newer_fill(trade, "stop");
        NoopBroker.modify_stop(trade, account, price)
    }
    fn modify_target(
        &self,
        trade: &Trade,
        account: &Account,
        price: Decimal,
    ) -> Result<String, Box<dyn Error>> {
        assert_eq!(account.broker_kind, BrokerKind::Alpaca);
        assert_eq!(account.environment, Environment::Paper);
        assert_eq!(account.broker_account_id, None);
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(gate) = &self.2 {
            gate.entered.send(()).unwrap();
            gate.release
                .recv_timeout(std::time::Duration::from_secs(10))
                .unwrap();
        }
        self.persist_newer_fill(trade, "target");
        NoopBroker.modify_target(trade, account, price)
    }
}

#[test]
fn competing_connections_submit_to_broker_only_once() {
    let path = std::env::temp_dir().join(format!("trust-submit-race-{}.db", uuid::Uuid::new_v4()));
    let url = path.to_str().unwrap().to_string();
    let mut facade = TrustFacade::new(Box::new(SqliteDatabase::new(&url)), Box::new(NoopBroker));
    let (_, trade) = seed(&mut facade, TradeCategory::Long);
    let (funded, _, _, _) = facade.fund_trade(&trade).unwrap();
    drop(facade);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let url = url.clone();
            let barrier = barrier.clone();
            let calls = calls.clone();
            let trade = funded.clone();
            std::thread::spawn(move || {
                let mut facade = TrustFacade::new(
                    Box::new(SqliteDatabase::new(&url)),
                    Box::new(CountingBroker(calls, None, None)),
                );
                barrier.wait();
                facade.submit_trade(&trade).is_ok()
            })
        })
        .collect();
    let successes = handles
        .into_iter()
        .map(|handle| usize::from(handle.join().unwrap()))
        .sum::<usize>();
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(successes, 1);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn late_submission_acknowledgement_preserves_a_newer_fill() {
    let path = std::env::temp_dir().join(format!("trust-ack-race-{}.db", uuid::Uuid::new_v4()));
    let url = path.to_str().unwrap().to_string();
    let mut facade = TrustFacade::new(
        Box::new(SqliteDatabase::new(&url)),
        Box::new(CountingBroker(
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            Some(url.clone()),
            None,
        )),
    );
    let (_, trade) = seed(&mut facade, TradeCategory::Long);
    let (funded, _, _, _) = facade.fund_trade(&trade).unwrap();
    let error = facade.submit_trade(&funded).unwrap_err().to_string();
    assert!(error.contains("Trade changed while awaiting broker response"));
    let latest = facade.read_trade(trade.id).unwrap();
    assert_eq!(latest.status, Status::Filled);
    assert_eq!(latest.entry.status, model::OrderStatus::Filled);
    assert_eq!(latest.entry.broker_order_id.as_deref(), Some("newer-fill"));
    drop(facade);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn late_close_acknowledgement_preserves_a_newer_exit_fill() {
    let path =
        std::env::temp_dir().join(format!("trust-close-ack-race-{}.db", uuid::Uuid::new_v4()));
    let url = path.to_str().unwrap().to_string();
    let mut facade = TrustFacade::new(
        Box::new(SqliteDatabase::new(&url)),
        Box::new(CountingBroker(
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            Some(url.clone()),
            None,
        )),
    );
    let (_, trade) = seed(&mut facade, TradeCategory::Long);
    let (mut funded, _, _, _) = facade.fund_trade(&trade).unwrap();
    funded.entry.average_filled_price = Some(dec!(100));
    let (filled, _) = facade.fill_trade(&funded, dec!(0)).unwrap();
    let error = facade.close_trade(&filled).unwrap_err().to_string();
    assert!(error.contains("Trade changed while awaiting broker response"));
    let latest = facade.read_trade(trade.id).unwrap();
    assert_eq!(latest.status, Status::ClosedTarget);
    assert_eq!(latest.target.status, model::OrderStatus::Filled);
    assert_eq!(latest.target.broker_order_id.as_deref(), Some("newer-fill"));
    drop(facade);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn order_modifications_use_persisted_account_metadata() {
    let mut facade = TrustFacade::new(
        Box::new(SqliteDatabase::new_in_memory()),
        Box::new(CountingBroker(
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            None,
            None,
        )),
    );
    let (mut supplied, trade) = seed(&mut facade, TradeCategory::Long);
    let (mut funded, _, _, _) = facade.fund_trade(&trade).unwrap();
    funded.entry.average_filled_price = Some(dec!(100));
    let (filled, _) = facade.fill_trade(&funded, dec!(0)).unwrap();
    supplied.broker_kind = BrokerKind::Ibkr;
    supplied.environment = Environment::Live;
    supplied.broker_account_id = Some("wrong-account".into());
    let modified = facade.modify_stop(&filled, &supplied, dec!(95)).unwrap();
    assert_eq!(modified.safety_stop.unit_price, dec!(95));
    let modified = facade
        .modify_target(&modified, &supplied, dec!(125))
        .unwrap();
    assert_eq!(modified.target.unit_price, dec!(125));
}

#[test]
fn stops_preserve_locked_profit_even_with_stale_client_snapshots() {
    for side in [TradeCategory::Long, TradeCategory::Short] {
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut facade = TrustFacade::new(
            Box::new(SqliteDatabase::new_in_memory()),
            Box::new(CountingBroker(calls.clone(), None, None)),
        );
        let (account, trade) = seed(&mut facade, side);
        let (mut funded, _, _, _) = facade.fund_trade(&trade).unwrap();
        funded.entry.average_filled_price = Some(dec!(100));
        let (original, _) = facade.fill_trade(&funded, Decimal::ZERO).unwrap();
        let (locked, looser, tighter) = if side == TradeCategory::Long {
            (dec!(105), dec!(102), dec!(108))
        } else {
            (dec!(95), dec!(98), dec!(92))
        };
        facade.modify_stop(&original, &account, locked).unwrap();
        let before = serde_json::to_value(facade.read_trade(trade.id).unwrap()).unwrap();
        let transactions = serde_json::to_value(facade.get_all_transactions().unwrap()).unwrap();
        for invalid in [looser, Decimal::ZERO, dec!(-1)] {
            assert!(facade.modify_stop(&original, &account, invalid).is_err());
            assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
            assert_eq!(
                serde_json::to_value(facade.read_trade(trade.id).unwrap()).unwrap(),
                before
            );
            assert_eq!(
                serde_json::to_value(facade.get_all_transactions().unwrap()).unwrap(),
                transactions
            );
        }
        let tightened = facade.modify_stop(&original, &account, tighter).unwrap();
        assert_eq!(tightened.safety_stop.unit_price, tighter);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }
}

#[test]
fn late_modification_acknowledgements_preserve_newer_exit_fills() {
    for role in ["stop", "target"] {
        let path =
            std::env::temp_dir().join(format!("trust-modify-race-{}.db", uuid::Uuid::new_v4()));
        let url = path.to_str().unwrap().to_string();
        let mut facade = TrustFacade::new(
            Box::new(SqliteDatabase::new(&url)),
            Box::new(CountingBroker(
                std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                Some(url.clone()),
                None,
            )),
        );
        let (account, trade) = seed(&mut facade, TradeCategory::Long);
        let (mut funded, _, _, _) = facade.fund_trade(&trade).unwrap();
        funded.entry.average_filled_price = Some(dec!(100));
        let (filled, _) = facade.fill_trade(&funded, dec!(0)).unwrap();
        let error = if role == "stop" {
            facade.modify_stop(&filled, &account, dec!(95))
        } else {
            facade.modify_target(&filled, &account, dec!(125))
        }
        .unwrap_err()
        .to_string();
        assert!(error.contains("Trade changed while awaiting broker response"));
        let latest = facade.read_trade(trade.id).unwrap();
        let order = if role == "stop" {
            latest.safety_stop
        } else {
            latest.target
        };
        assert_eq!(order.status, model::OrderStatus::Filled);
        assert_eq!(order.broker_order_id.as_deref(), Some("newer-fill"));
        drop(facade);
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn uncertain_target_change_blocks_retry_and_close_until_reconciled() {
    let scenario = std::sync::Arc::new(std::sync::Mutex::new(FillScenario {
        entry_qty: dec!(10),
        exit_qty: dec!(0),
        canceled: false,
        fees: Vec::new(),
        stop_price: None,
        target_price: None,
    }));
    let mut facade = TrustFacade::new(
        Box::new(SqliteDatabase::new_in_memory()),
        Box::new(ReplayBroker(scenario.clone())),
    );
    let (account, trade) = seed(&mut facade, TradeCategory::Long);
    let (funded, _, _, _) = facade.fund_trade(&trade).unwrap();
    let (submitted, _) = facade.submit_trade(&funded).unwrap();
    facade.sync_trade(&submitted, &account).unwrap();
    let filled = facade.read_trade(trade.id).unwrap();
    assert!(facade
        .modify_target(&filled, &account, dec!(125))
        .unwrap_err()
        .to_string()
        .contains("lost target acknowledgement"));
    assert!(facade
        .modify_target(&filled, &account, dec!(130))
        .unwrap_err()
        .to_string()
        .contains("Previous target modification"));
    assert!(facade
        .close_trade(&filled)
        .unwrap_err()
        .to_string()
        .contains("Previous target modification"));
    scenario.lock().unwrap().target_price = Some(dec!(125));
    facade.sync_trade(&filled, &account).unwrap();
    let reconciled = facade.read_trade(trade.id).unwrap();
    assert_eq!(reconciled.target.unit_price, dec!(125));
    assert!(facade
        .modify_target(&reconciled, &account, dec!(130))
        .unwrap_err()
        .to_string()
        .contains("lost target acknowledgement"));
}

#[test]
fn cancellation_ignores_a_stale_caller_trade_account_id() {
    let mut facade = TrustFacade::new(
        Box::new(SqliteDatabase::new_in_memory()),
        Box::new(CountingBroker(
            std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            None,
            None,
        )),
    );
    let (account, trade) = seed(&mut facade, TradeCategory::Long);
    let (funded, _, _, _) = facade.fund_trade(&trade).unwrap();
    let (mut submitted, _) = facade.submit_trade(&funded).unwrap();
    submitted.account_id = uuid::Uuid::new_v4();
    facade.cancel_submitted_trade(&submitted).unwrap();
    assert_eq!(
        facade.read_trade(trade.id).unwrap().status,
        Status::Canceled
    );
    assert_eq!(
        facade
            .search_balance(account.id, &Currency::USD)
            .unwrap()
            .total_available,
        dec!(10000)
    );
}

#[test]
fn competing_connections_modify_target_only_once() {
    let path = std::env::temp_dir().join(format!("trust-target-race-{}.db", uuid::Uuid::new_v4()));
    let url = path.to_str().unwrap().to_string();
    let mut facade = TrustFacade::new(Box::new(SqliteDatabase::new(&url)), Box::new(NoopBroker));
    let (account, trade) = seed(&mut facade, TradeCategory::Long);
    let (mut funded, _, _, _) = facade.fund_trade(&trade).unwrap();
    funded.entry.average_filled_price = Some(dec!(100));
    let (filled, _) = facade.fill_trade(&funded, dec!(0)).unwrap();
    drop(facade);
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let first_url = url.clone();
    let first_calls = calls.clone();
    let first_trade = filled.clone();
    let first_account = account.clone();
    let first = std::thread::spawn(move || {
        let mut facade = TrustFacade::new(
            Box::new(SqliteDatabase::new(&first_url)),
            Box::new(CountingBroker(
                first_calls,
                None,
                Some(TargetGate {
                    entered: entered_tx,
                    release: release_rx,
                }),
            )),
        );
        facade
            .modify_target(&first_trade, &first_account, dec!(125))
            .is_ok()
    });
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    let mut second = TrustFacade::new(
        Box::new(SqliteDatabase::new(&url)),
        Box::new(CountingBroker(calls.clone(), None, None)),
    );
    let result = second.modify_target(&filled, &account, dec!(130));
    release_tx.send(()).unwrap();
    assert!(first.join().unwrap());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Previous target modification"));
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    drop(second);
    std::fs::remove_file(path).unwrap();
}

fn assert_projection_matches_ledger(db: &SqliteDatabase, account: &Account) {
    let balance = db
        .account_balance_read()
        .for_currency(account.id, &Currency::USD)
        .unwrap();
    let txs = db
        .transaction_read()
        .all_transactions(account.id, &Currency::USD)
        .unwrap();
    let cash = txs.iter().fold(Decimal::ZERO, |sum, tx| match tx.category {
        TransactionCategory::Deposit
        | TransactionCategory::PaymentFromTrade(_)
        | TransactionCategory::PaymentEarnings(_) => sum + tx.amount,
        TransactionCategory::Withdrawal
        | TransactionCategory::WithdrawalEarnings
        | TransactionCategory::FundTrade(_)
        | TransactionCategory::FeeOpen(_)
        | TransactionCategory::FeeClose(_) => sum - tx.amount,
        _ => sum,
    });
    let total = txs.iter().fold(Decimal::ZERO, |sum, tx| match tx.category {
        TransactionCategory::Deposit
        | TransactionCategory::CloseTarget(_)
        | TransactionCategory::CloseSafetyStop(_)
        | TransactionCategory::CloseSafetyStopSlippage(_) => sum + tx.amount,
        TransactionCategory::Withdrawal
        | TransactionCategory::WithdrawalTax
        | TransactionCategory::WithdrawalEarnings
        | TransactionCategory::OpenTrade(_)
        | TransactionCategory::FeeOpen(_)
        | TransactionCategory::FeeClose(_) => sum - tx.amount,
        _ => sum,
    });
    assert_eq!(balance.total_available, cash);
    assert_eq!(balance.total_balance, total);
    assert_eq!(
        balance.total_in_trade,
        db.transaction_read()
            .account_market_exposure(account.id, &Currency::USD)
            .unwrap()
            .unwrap()
    );
}

#[test]
fn incremental_broker_projections_match_full_ledger_for_partial_cancel_exit_fees_and_replay() {
    for side in [TradeCategory::Long, TradeCategory::Short] {
        for quantity in 1..=10 {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("projection.db");
            let path = path.to_str().unwrap();
            let scenario = std::sync::Arc::new(std::sync::Mutex::new(FillScenario {
                entry_qty: Decimal::from(quantity),
                exit_qty: Decimal::ZERO,
                canceled: false,
                fees: vec![("opening".into(), dec!(1.25))],
                stop_price: None,
                target_price: None,
            }));
            let mut facade = TrustFacade::new(
                Box::new(SqliteDatabase::new(path)),
                Box::new(ReplayBroker(scenario.clone())),
            );
            let db = SqliteDatabase::new(path);
            let (account, trade) = seed(&mut facade, side);
            let funded = facade.fund_trade(&trade).unwrap().0;
            assert_projection_matches_ledger(&db, &account);
            let submitted = facade.submit_trade(&funded).unwrap().0;
            assert_projection_matches_ledger(&db, &account);
            if quantity == 1 {
                use diesel::{Connection, RunQueryDsl};
                let mut faults = diesel::SqliteConnection::establish(path).unwrap();
                for (event, table) in [
                    ("UPDATE", "accounts_balances"),
                    ("INSERT", "fee_activities"),
                ] {
                    let before = serde_json::to_value((
                        facade.read_trade(trade.id).unwrap(),
                        facade.search_balance(account.id, &Currency::USD).unwrap(),
                        db.transaction_read()
                            .all_transactions(account.id, &Currency::USD)
                            .unwrap(),
                    ))
                    .unwrap();
                    diesel::sql_query(format!("CREATE TRIGGER projection_failure BEFORE {event} ON {table} BEGIN SELECT RAISE(ABORT,'projection fault'); END")).execute(&mut faults).unwrap();
                    assert!(facade.sync_trade(&submitted, &account).is_err());
                    let after = serde_json::to_value((
                        facade.read_trade(trade.id).unwrap(),
                        facade.search_balance(account.id, &Currency::USD).unwrap(),
                        db.transaction_read()
                            .all_transactions(account.id, &Currency::USD)
                            .unwrap(),
                    ))
                    .unwrap();
                    assert_eq!(before, after, "partial projection or fee write at {table}");
                    diesel::sql_query("DROP TRIGGER projection_failure")
                        .execute(&mut faults)
                        .unwrap();
                }
            }
            facade.sync_trade(&submitted, &account).unwrap();
            assert_projection_matches_ledger(&db, &account);
            scenario.lock().unwrap().canceled = true;
            facade.sync_trade(&submitted, &account).unwrap();
            assert_projection_matches_ledger(&db, &account);
            {
                let mut state = scenario.lock().unwrap();
                state.exit_qty = Decimal::from(quantity);
                state.fees.push(("closing".into(), dec!(2.50)));
            }
            facade.sync_trade(&submitted, &account).unwrap();
            assert_projection_matches_ledger(&db, &account);
            let before = db
                .transaction_read()
                .all_transactions(account.id, &Currency::USD)
                .unwrap();
            facade.sync_trade(&submitted, &account).unwrap();
            assert_projection_matches_ledger(&db, &account);
            let after = db
                .transaction_read()
                .all_transactions(account.id, &Currency::USD)
                .unwrap();
            assert_eq!(before.len(), after.len());
            assert_eq!(
                facade
                    .search_balance(account.id, &Currency::USD)
                    .unwrap()
                    .total_available,
                dec!(10000) + Decimal::from(quantity) * dec!(20) - dec!(3.75)
            );
        }
    }
}

#[test]
fn broker_snapshot_cannot_erase_previous_stop_protection() {
    for side in [TradeCategory::Long, TradeCategory::Short] {
        let scenario = std::sync::Arc::new(std::sync::Mutex::new(FillScenario {
            entry_qty: dec!(10),
            exit_qty: Decimal::ZERO,
            canceled: false,
            fees: Vec::new(),
            stop_price: None,
            target_price: None,
        }));
        let mut facade = TrustFacade::new(
            Box::new(SqliteDatabase::new_in_memory()),
            Box::new(ReplayBroker(scenario.clone())),
        );
        let (account, trade) = seed(&mut facade, side);
        let funded = facade.fund_trade(&trade).unwrap().0;
        let submitted = facade.submit_trade(&funded).unwrap().0;
        facade.sync_trade(&submitted, &account).unwrap();
        let filled = facade.read_trade(trade.id).unwrap();
        let (confirmed, external_tighter, old, forbidden) = if side == TradeCategory::Long {
            (dec!(105), dec!(107), dec!(90), dec!(106))
        } else {
            (dec!(95), dec!(93), dec!(110), dec!(94))
        };
        assert!(facade
            .modify_stop(&filled, &account, confirmed)
            .unwrap_err()
            .to_string()
            .contains("lost acknowledgement"));
        for observed in [confirmed, external_tighter, old] {
            scenario.lock().unwrap().stop_price = Some(observed);
            let current = facade.read_trade(trade.id).unwrap();
            facade.sync_trade(&current, &account).unwrap();
        }
        // Accounting retains the latest observed broker state; the amendment
        // bound must independently remember stronger confirmed protection.
        let weaker = facade.read_trade(trade.id).unwrap();
        assert_eq!(weaker.safety_stop.unit_price, old);
        let error = facade
            .modify_stop(&weaker, &account, forbidden)
            .unwrap_err();
        assert!(error.to_string().contains("risking more"), "{error}");
        assert_eq!(
            facade.read_trade(trade.id).unwrap().safety_stop.unit_price,
            old
        );
    }
}
