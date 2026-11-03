use chrono::{DateTime, Duration, Utc};
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Account, Broker, BrokerKind, BrokerLog, Currency, DraftTrade, Environment, Execution,
    ExecutionSide, ExecutionSource, FeeActivity, Order, OrderIds, OrderStatus, Status, Trade,
    TradeCategory, TradingVehicleCategory, TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::{
    error::Error,
    sync::{Arc, Mutex},
};

struct Feed {
    failure: &'static str,
    fee: Decimal,
    filled_at: DateTime<Utc>,
    submissions: usize,
}
struct RecoveryBroker(Arc<Mutex<Feed>>);
impl Broker for RecoveryBroker {
    fn kind(&self) -> BrokerKind {
        BrokerKind::Alpaca
    }
    fn submit_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        self.0.lock().unwrap().submissions += 1;
        Ok((
            BrokerLog::default(),
            OrderIds {
                entry: "entry".into(),
                stop: "stop".into(),
                target: "target".into(),
            },
        ))
    }
    fn sync_trade(
        &self,
        trade: &Trade,
        _: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        let state = self.0.lock().unwrap();
        let mut entry = trade.entry.clone();
        let mut target = trade.target.clone();
        for order in [&mut entry, &mut target] {
            order.status = OrderStatus::Filled;
            order.filled_quantity = order.quantity;
            order.average_filled_price = Some(order.unit_price);
            order.filled_at = Some(state.filled_at.naive_utc());
        }
        Ok((
            Status::ClosedTarget,
            vec![entry, target],
            BrokerLog::default(),
        ))
    }
    fn fetch_executions(
        &self,
        trade: &Trade,
        account: &Account,
        after: Option<DateTime<Utc>>,
    ) -> Result<Vec<Execution>, Box<dyn Error>> {
        assert!(
            after.is_none(),
            "Late native fills must not be excluded by a snapshot timestamp"
        );
        let state = self.0.lock().unwrap();
        if state.failure == "executions" {
            return Err("execution feed unavailable".into());
        }
        Ok([
            (&trade.entry, ExecutionSide::Buy),
            (&trade.target, ExecutionSide::Sell),
        ]
        .into_iter()
        .map(|(order, side)| {
            Execution::new(
                "alpaca".into(),
                ExecutionSource::AccountActivities,
                account.id,
                format!("native-{}", order.id),
                order.broker_order_id.clone(),
                "RECOVERY".into(),
                side,
                order.quantity,
                order.unit_price,
                state.filled_at.naive_utc(),
            )
        })
        .collect())
    }
    fn fetch_fee_activities(
        &self,
        trade: &Trade,
        account: &Account,
        after: Option<DateTime<Utc>>,
    ) -> Result<Vec<FeeActivity>, Box<dyn Error>> {
        assert!(after.is_none());
        let state = self.0.lock().unwrap();
        if state.failure == "fees" {
            return Err("commission feed unavailable".into());
        }
        if state.fee.is_zero() {
            return Ok(vec![]);
        }
        Ok(vec![FeeActivity {
            broker: "alpaca".into(),
            account_id: account.id,
            broker_activity_id: "late-commission".into(),
            broker_order_id: trade.target.broker_order_id.clone(),
            symbol: Some("RECOVERY".into()),
            activity_type: "commission".into(),
            amount: state.fee,
            occurred_at: state.filled_at.naive_utc(),
            raw_json: None,
        }])
    }
    fn close_trade(&self, _: &Trade, _: &Account) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        Err("unexpected mutation".into())
    }
    fn cancel_trade(&self, _: &Trade, _: &Account) -> Result<(), Box<dyn Error>> {
        Err("unexpected mutation".into())
    }
    fn modify_stop(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        Err("unexpected mutation".into())
    }
    fn modify_target(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        Err("unexpected mutation".into())
    }
}

fn open(directory: &tempfile::TempDir, feed: &Arc<Mutex<Feed>>) -> TrustFacade {
    TrustFacade::new(
        Box::new(SqliteDatabase::new(
            directory.path().join("recovery.db").to_str().unwrap(),
        )),
        Box::new(RecoveryBroker(feed.clone())),
    )
}
fn seed(f: &mut TrustFacade) -> (Account, Trade) {
    let account = f
        .create_account("recovery", "recovery", Environment::Paper, dec!(0), dec!(0))
        .unwrap();
    f.create_transaction(
        &account,
        &TransactionCategory::Deposit,
        dec!(10000),
        &Currency::USD,
    )
    .unwrap();
    let vehicle = f
        .create_trading_vehicle("RECOVERY", None, &TradingVehicleCategory::Stock, "alpaca")
        .unwrap();
    let trade = f
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity: dec!(10),
                currency: Currency::USD,
                category: TradeCategory::Long,
                thesis: None,
                sector: None,
                asset_class: None,
                context: None,
            },
            dec!(90),
            dec!(100),
            dec!(120),
        )
        .unwrap();
    let (funded, _, _, _) = f.fund_trade(&trade).unwrap();
    let (submitted, _) = f.submit_trade(&funded).unwrap();
    (account, submitted)
}

#[test]
fn failed_activity_reads_retain_fills_and_recover_native_evidence_and_fees_after_restart() {
    for failure in ["executions", "fees"] {
        let directory = tempfile::tempdir().unwrap();
        let feed = Arc::new(Mutex::new(Feed {
            failure,
            fee: dec!(2.5),
            filled_at: Utc::now(),
            submissions: 0,
        }));
        let mut f = open(&directory, &feed);
        let (account, submitted) = seed(&mut f);
        let error = f.sync_trade(&submitted, &account).unwrap_err();
        assert!(error.is::<core::broker_reconciliation::IncompleteEvidence>());
        let closed = f.read_trade(submitted.id).unwrap();
        assert_eq!(closed.status, Status::ClosedTarget);
        assert_eq!(closed.entry.filled_quantity, dec!(10));
        assert_eq!(closed.target.filled_quantity, dec!(10));
        assert_eq!(
            f.search_balance(account.id, &Currency::USD)
                .unwrap()
                .total_available,
            dec!(10200)
        );
        assert_eq!(
            f.trade_audit_events(closed.id)
                .unwrap()
                .last()
                .unwrap()
                .outcome,
            "incomplete"
        );
        drop(f);
        let mut f = open(&directory, &feed);
        assert!(f
            .terminal_broker_reconciliation_due(&closed, Utc::now() + Duration::days(8))
            .unwrap());
        feed.lock().unwrap().failure = "";
        f.sync_trade(&closed, &account).unwrap();
        let grade = f.latest_trade_grade(closed.id).unwrap().unwrap();
        f.sync_trade(&closed, &account).unwrap();
        assert_eq!(
            f.latest_trade_grade(closed.id).unwrap().unwrap().id,
            grade.id
        );
        assert_eq!(
            f.search_balance(account.id, &Currency::USD)
                .unwrap()
                .total_available,
            dec!(10197.5)
        );
        let executions = f.executions_for_trade(closed.id).unwrap();
        assert_eq!(
            executions
                .iter()
                .filter(|e| e.source == ExecutionSource::AccountActivities)
                .count(),
            2
        );
        assert_eq!(feed.lock().unwrap().submissions, 1);
        assert!(f
            .terminal_broker_reconciliation_due(&closed, Utc::now())
            .unwrap());
        assert!(!f
            .terminal_broker_reconciliation_due(&closed, Utc::now() + Duration::days(8))
            .unwrap());
    }
}

#[test]
fn successful_close_remains_eligible_for_a_commission_that_arrives_later() {
    let directory = tempfile::tempdir().unwrap();
    let feed = Arc::new(Mutex::new(Feed {
        failure: "",
        fee: Decimal::ZERO,
        filled_at: Utc::now(),
        submissions: 0,
    }));
    let mut f = open(&directory, &feed);
    let (account, submitted) = seed(&mut f);
    f.sync_trade(&submitted, &account).unwrap();
    drop(f);
    let mut f = open(&directory, &feed);
    let closed = f.read_trade(submitted.id).unwrap();
    assert_eq!(closed.status, Status::ClosedTarget);
    assert!(f
        .terminal_broker_reconciliation_due(&closed, Utc::now())
        .unwrap());
    feed.lock().unwrap().fee = dec!(2.5);
    for _ in 0..3 {
        f.sync_trade(&closed, &account).unwrap();
    }
    assert_eq!(
        f.search_balance(account.id, &Currency::USD)
            .unwrap()
            .total_available,
        dec!(10197.5)
    );
    assert_eq!(
        f.read_trade(closed.id).unwrap().balance.total_performance,
        dec!(197.5)
    );
    assert_eq!(feed.lock().unwrap().submissions, 1);
}
