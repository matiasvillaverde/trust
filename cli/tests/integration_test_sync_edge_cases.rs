use chrono::{NaiveDateTime, Utc};
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Account, Broker, BrokerLog, Currency, DraftTrade, FeeActivity, Order, OrderIds, OrderStatus,
    RuleLevel, RuleName, Status, Trade, TradeCategory, TradingVehicleCategory, TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup_facade(broker: impl Broker + 'static) -> TrustFacade {
    TrustFacade::new(Box::new(SqliteDatabase::new_in_memory()), Box::new(broker))
}

fn setup_account_with_rules(
    trust: &mut TrustFacade,
    deposit: Decimal,
    risk_per_trade: f32,
) -> Account {
    trust
        .create_account(
            "test",
            "default",
            model::Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .unwrap();
    let account = trust.search_account("test").unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            deposit,
            &Currency::USD,
        )
        .unwrap();
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerMonth(6.0),
            "risk month",
            &RuleLevel::Error,
        )
        .unwrap();
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(risk_per_trade),
            "risk trade",
            &RuleLevel::Error,
        )
        .unwrap();
    account
}

fn create_vehicle(trust: &mut TrustFacade, symbol: &str) -> model::TradingVehicle {
    let isin = format!("US{}", Uuid::new_v4().simple());
    trust
        .create_trading_vehicle(
            symbol,
            Some(&isin),
            &TradingVehicleCategory::Stock,
            "NASDAQ",
        )
        .unwrap()
}

fn create_submitted_trade(
    trust: &mut TrustFacade,
    account: &Account,
    symbol: &str,
    stop: Decimal,
    entry: Decimal,
    target: Decimal,
    quantity: u64,
) -> Trade {
    let tv = create_vehicle(trust, symbol);
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: quantity as i64,
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };
    trust.create_trade(draft, stop, entry, target).unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .pop()
        .unwrap();
    trust.fund_trade(&trade).unwrap();
    let trade = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .pop()
        .unwrap();
    trust.submit_trade(&trade).unwrap();
    trust
        .search_trades(account.id, Status::Submitted)
        .unwrap()
        .pop()
        .unwrap()
}

// ---------------------------------------------------------------------------
// Stateful broker that supports fee activities and configurable sync responses
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct StatefulBroker {
    state: Arc<Mutex<BrokerState>>,
}

struct BrokerState {
    #[allow(clippy::type_complexity)]
    sync_fn: Box<dyn Fn(&Trade) -> (Status, Vec<Order>) + Send>,
    fees: Vec<FeeActivity>,
}

impl StatefulBroker {
    fn new(sync_fn: impl Fn(&Trade) -> (Status, Vec<Order>) + Send + 'static) -> Self {
        Self {
            state: Arc::new(Mutex::new(BrokerState {
                sync_fn: Box::new(sync_fn),
                fees: vec![],
            })),
        }
    }

    fn with_fees(
        sync_fn: impl Fn(&Trade) -> (Status, Vec<Order>) + Send + 'static,
        fees: Vec<FeeActivity>,
    ) -> Self {
        Self {
            state: Arc::new(Mutex::new(BrokerState {
                sync_fn: Box::new(sync_fn),
                fees,
            })),
        }
    }
}

impl Broker for StatefulBroker {
    fn kind(&self) -> model::BrokerKind {
        model::BrokerKind::Alpaca
    }

    fn submit_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        Ok((
            BrokerLog::default(),
            OrderIds {
                entry: Uuid::new_v4().to_string(),
                target: Uuid::new_v4().to_string(),
                stop: Uuid::new_v4().to_string(),
            },
        ))
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        _: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        let state = self.state.lock().unwrap();
        let (status, orders) = (state.sync_fn)(trade);
        Ok((status, orders, BrokerLog::default()))
    }

    fn close_trade(&self, _: &Trade, _: &Account) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        unimplemented!()
    }

    fn cancel_trade(&self, _: &Trade, _: &Account) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn modify_stop(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        unimplemented!()
    }

    fn modify_target(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        unimplemented!()
    }

    fn fetch_fee_activities(
        &self,
        _trade: &Trade,
        _account: &Account,
        _after: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<FeeActivity>, Box<dyn Error>> {
        let state = self.state.lock().unwrap();
        Ok(state.fees.clone())
    }
}

// ---------------------------------------------------------------------------
// Simple NoOp broker
// ---------------------------------------------------------------------------

struct NoOpBroker;
impl Broker for NoOpBroker {
    fn kind(&self) -> model::BrokerKind {
        model::BrokerKind::Alpaca
    }
    fn submit_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        Ok((
            BrokerLog::default(),
            OrderIds {
                entry: Uuid::new_v4().to_string(),
                target: Uuid::new_v4().to_string(),
                stop: Uuid::new_v4().to_string(),
            },
        ))
    }
    fn sync_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        unimplemented!()
    }
    fn close_trade(&self, _: &Trade, _: &Account) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        unimplemented!()
    }
    fn cancel_trade(&self, _: &Trade, _: &Account) -> Result<(), Box<dyn Error>> {
        unimplemented!()
    }
    fn modify_stop(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        unimplemented!()
    }
    fn modify_target(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Response helpers
// ---------------------------------------------------------------------------

fn now() -> NaiveDateTime {
    Utc::now().naive_utc()
}

fn make_filled_entry(trade: &Trade, price: Decimal) -> Order {
    Order {
        id: trade.entry.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        filled_quantity: trade.entry.quantity,
        average_filled_price: Some(price),
        status: OrderStatus::Filled,
        filled_at: Some(now()),
        ..Default::default()
    }
}

fn make_filled_target(trade: &Trade, price: Decimal) -> Order {
    Order {
        id: trade.target.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        filled_quantity: trade.entry.quantity,
        average_filled_price: Some(price),
        status: OrderStatus::Filled,
        filled_at: Some(now()),
        ..Default::default()
    }
}

fn make_filled_stop(trade: &Trade, price: Decimal) -> Order {
    Order {
        id: trade.safety_stop.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        filled_quantity: trade.entry.quantity,
        average_filled_price: Some(price),
        status: OrderStatus::Filled,
        filled_at: Some(now()),
        ..Default::default()
    }
}

fn make_canceled_order(trade_order: &Order) -> Order {
    Order {
        id: trade_order.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        status: OrderStatus::Canceled,
        ..Default::default()
    }
}

fn make_held_order(trade_order: &Order) -> Order {
    Order {
        id: trade_order.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        status: OrderStatus::Held,
        ..Default::default()
    }
}

fn make_accepted_order(trade_order: &Order) -> Order {
    Order {
        id: trade_order.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        status: OrderStatus::Accepted,
        ..Default::default()
    }
}

// ===========================================================================
// 1. SUBMITTED → CLOSED DIRECTLY (single sync fills entry + closes)
//    This is the "fast path" where broker reports everything in one update.
// ===========================================================================

#[test]
fn test_submitted_to_closed_target_in_single_sync() {
    // Broker reports entry filled AND target filled in one sync call
    let broker = StatefulBroker::new(|trade| {
        let entry = make_filled_entry(trade, dec!(40));
        let target = make_filled_target(trade, dec!(50));
        let stop = make_canceled_order(&trade.safety_stop);
        (Status::ClosedTarget, vec![entry, target, stop])
    });
    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    trust.sync_trade(&trade, &account).unwrap();

    let closed = trust
        .search_trades(account.id, Status::ClosedTarget)
        .unwrap();
    assert_eq!(closed.len(), 1);
    let closed_trade = &closed[0];

    // Performance = target(50*500) - entry(40*500) = 25000 - 20000 = 5000
    assert_eq!(closed_trade.balance.total_performance, dec!(5000));
    assert_eq!(closed_trade.balance.capital_in_market, dec!(0));
    assert_eq!(closed_trade.balance.capital_out_market, dec!(0));

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(55000)); // 50000 + 5000
    assert_eq!(balance.total_available, dec!(55000));
    assert_eq!(balance.total_in_trade, dec!(0));
}

#[test]
fn test_submitted_to_closed_stop_in_single_sync() {
    // Broker reports entry filled AND stop filled in one sync call
    let broker = StatefulBroker::new(|trade| {
        let entry = make_filled_entry(trade, dec!(40));
        let stop = make_filled_stop(trade, dec!(38));
        let target = make_canceled_order(&trade.target);
        (Status::ClosedStopLoss, vec![entry, stop, target])
    });
    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    trust.sync_trade(&trade, &account).unwrap();

    let closed = trust
        .search_trades(account.id, Status::ClosedStopLoss)
        .unwrap();
    assert_eq!(closed.len(), 1);

    // Performance = stop(38*500) - entry(40*500) = 19000 - 20000 = -1000
    assert_eq!(closed[0].balance.total_performance, dec!(-1000));

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(49000)); // 50000 - 1000
    assert_eq!(balance.total_available, dec!(49000));
    assert_eq!(balance.total_in_trade, dec!(0));
}

// ===========================================================================
// 2. IDEMPOTENT RE-SYNC — already-closed trade synced again
// ===========================================================================

#[test]
fn test_resync_already_closed_target_is_noop() {
    let broker = StatefulBroker::new(|trade| {
        let entry = make_filled_entry(trade, dec!(40));
        let target = make_filled_target(trade, dec!(50));
        let stop = make_canceled_order(&trade.safety_stop);
        (Status::ClosedTarget, vec![entry, target, stop])
    });
    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    // Sync multiple times
    for _ in 0..5 {
        trust.sync_trade(&trade, &account).unwrap();
    }

    let closed = trust
        .search_trades(account.id, Status::ClosedTarget)
        .unwrap();
    assert_eq!(closed.len(), 1, "no duplicate closed trades");

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(55000));
    assert_eq!(balance.total_available, dec!(55000));
}

#[test]
fn test_resync_already_closed_stop_is_noop() {
    let broker = StatefulBroker::new(|trade| {
        let entry = make_filled_entry(trade, dec!(40));
        let stop = make_filled_stop(trade, dec!(38));
        let target = make_canceled_order(&trade.target);
        (Status::ClosedStopLoss, vec![entry, stop, target])
    });
    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    for _ in 0..5 {
        trust.sync_trade(&trade, &account).unwrap();
    }

    let closed = trust
        .search_trades(account.id, Status::ClosedStopLoss)
        .unwrap();
    assert_eq!(closed.len(), 1);

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(49000));
}

// ===========================================================================
// 3. SUBMITTED → SUBMITTED (no fills yet, orders just accepted)
// ===========================================================================

#[test]
fn test_sync_submitted_no_fills_is_noop() {
    let broker = StatefulBroker::new(|trade| {
        let entry = make_accepted_order(&trade.entry);
        let target = make_held_order(&trade.target);
        let stop = make_held_order(&trade.safety_stop);
        (Status::Submitted, vec![entry, target, stop])
    });
    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    trust.sync_trade(&trade, &account).unwrap();

    let trades = trust.search_trades(account.id, Status::Submitted).unwrap();
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].status, Status::Submitted);

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000)); // Unchanged from funding
}

// ===========================================================================
// 4. FEE RECONCILIATION — Broker-reported fees create fee transactions
// ===========================================================================

#[test]
fn test_fee_activity_matched_by_broker_order_id() {
    let entry_broker_id = Uuid::new_v4().to_string();
    let entry_broker_id_clone = entry_broker_id.clone();

    let broker = StatefulBroker::with_fees(
        move |trade| {
            let entry = Order {
                id: trade.entry.id,
                broker_order_id: Some(entry_broker_id_clone.clone()),
                filled_quantity: trade.entry.quantity,
                average_filled_price: Some(dec!(40)),
                status: OrderStatus::Filled,
                filled_at: Some(now()),
                ..Default::default()
            };
            let target = make_accepted_order(&trade.target);
            let stop = make_held_order(&trade.safety_stop);
            (Status::Filled, vec![entry, target, stop])
        },
        vec![FeeActivity {
            broker: "alpaca".to_string(),
            broker_activity_id: "fee-001".to_string(),
            account_id: Uuid::nil(), // Will be ignored in matching
            broker_order_id: Some(entry_broker_id.clone()),
            symbol: Some("TSLA".to_string()),
            activity_type: "FEE".to_string(),
            amount: dec!(5.50),
            occurred_at: now(),
            raw_json: None,
        }],
    );

    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    trust.sync_trade(&trade, &account).unwrap();

    // Fee should reduce total_available
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    // Without fee: 30000 (from funding). With 5.50 fee: 30000 - 5.50 = 29994.50
    assert_eq!(balance.total_available, dec!(29994.50));

    // Verify fee transaction exists
    let txs = trust.get_account_transactions(account.id).unwrap();
    let fee_txs: Vec<_> = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::FeeOpen(_)))
        .collect();
    assert_eq!(
        fee_txs.len(),
        1,
        "should have exactly one FeeOpen transaction"
    );
    assert_eq!(fee_txs[0].amount, dec!(5.50));
}

#[test]
fn test_fee_activity_matched_by_symbol_and_day_heuristic() {
    // Fee has no broker_order_id, but matches symbol + day
    let broker = StatefulBroker::with_fees(
        |trade| {
            let entry = Order {
                id: trade.entry.id,
                broker_order_id: Some(Uuid::new_v4().to_string()),
                filled_quantity: trade.entry.quantity,
                average_filled_price: Some(dec!(40)),
                status: OrderStatus::Filled,
                filled_at: Some(now()),
                ..Default::default()
            };
            let target = make_accepted_order(&trade.target);
            let stop = make_held_order(&trade.safety_stop);
            (Status::Filled, vec![entry, target, stop])
        },
        vec![FeeActivity {
            broker: "alpaca".to_string(),
            broker_activity_id: "fee-heuristic-001".to_string(),
            account_id: Uuid::nil(),
            broker_order_id: None, // No direct match
            symbol: Some("TSLA".to_string()),
            activity_type: "FEE".to_string(),
            amount: dec!(3.25),
            occurred_at: now(), // Same day as fill
            raw_json: None,
        }],
    );

    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    trust.sync_trade(&trade, &account).unwrap();

    let txs = trust.get_account_transactions(account.id).unwrap();
    let fee_txs: Vec<_> = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::FeeOpen(_)))
        .collect();
    assert_eq!(
        fee_txs.len(),
        1,
        "fee should match via symbol+day heuristic"
    );
    assert_eq!(fee_txs[0].amount, dec!(3.25));
}

#[test]
fn test_fee_activity_wrong_symbol_not_matched() {
    let broker = StatefulBroker::with_fees(
        |trade| {
            let entry = make_filled_entry(trade, dec!(40));
            let target = make_accepted_order(&trade.target);
            let stop = make_held_order(&trade.safety_stop);
            (Status::Filled, vec![entry, target, stop])
        },
        vec![FeeActivity {
            broker: "alpaca".to_string(),
            broker_activity_id: "fee-wrong-symbol".to_string(),
            account_id: Uuid::nil(),
            broker_order_id: None,
            symbol: Some("AAPL".to_string()), // Different symbol
            activity_type: "FEE".to_string(),
            amount: dec!(10.00),
            occurred_at: now(),
            raw_json: None,
        }],
    );

    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    trust.sync_trade(&trade, &account).unwrap();

    let txs = trust.get_account_transactions(account.id).unwrap();
    let fee_txs: Vec<_> = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::FeeOpen(_)))
        .collect();
    assert_eq!(
        fee_txs.len(),
        0,
        "fee for wrong symbol should not be matched"
    );

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000)); // No fee deducted
}

#[test]
fn test_fee_not_duplicated_on_resync() {
    let entry_broker_id = Uuid::new_v4().to_string();
    let entry_broker_id_clone = entry_broker_id.clone();

    let broker = StatefulBroker::with_fees(
        move |trade| {
            let entry = Order {
                id: trade.entry.id,
                broker_order_id: Some(entry_broker_id_clone.clone()),
                filled_quantity: trade.entry.quantity,
                average_filled_price: Some(dec!(40)),
                status: OrderStatus::Filled,
                filled_at: Some(now()),
                ..Default::default()
            };
            let target = make_accepted_order(&trade.target);
            let stop = make_held_order(&trade.safety_stop);
            (Status::Filled, vec![entry, target, stop])
        },
        vec![FeeActivity {
            broker: "alpaca".to_string(),
            broker_activity_id: "fee-idempotent".to_string(),
            account_id: Uuid::nil(),
            broker_order_id: Some(entry_broker_id),
            symbol: Some("TSLA".to_string()),
            activity_type: "FEE".to_string(),
            amount: dec!(7.00),
            occurred_at: now(),
            raw_json: None,
        }],
    );

    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    // Sync multiple times — fee should only be created once
    for _ in 0..5 {
        trust.sync_trade(&trade, &account).unwrap();
    }

    let txs = trust.get_account_transactions(account.id).unwrap();
    let fee_txs: Vec<_> = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::FeeOpen(_)))
        .collect();
    assert_eq!(fee_txs.len(), 1, "fee should not be duplicated on resync");
    assert_eq!(fee_txs[0].amount, dec!(7.00));

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(29993)); // 30000 - 7
}

// ===========================================================================
// 5. CLOSING FEE — Fee matched to exit order
// ===========================================================================

#[test]
fn test_closing_fee_matched_to_target_order() {
    let target_broker_id = Uuid::new_v4().to_string();
    let target_broker_id_clone = target_broker_id.clone();

    let broker = StatefulBroker::with_fees(
        move |trade| {
            let entry = make_filled_entry(trade, dec!(40));
            let target = Order {
                id: trade.target.id,
                broker_order_id: Some(target_broker_id_clone.clone()),
                filled_quantity: trade.entry.quantity,
                average_filled_price: Some(dec!(50)),
                status: OrderStatus::Filled,
                filled_at: Some(now()),
                ..Default::default()
            };
            let stop = make_canceled_order(&trade.safety_stop);
            (Status::ClosedTarget, vec![entry, target, stop])
        },
        vec![FeeActivity {
            broker: "alpaca".to_string(),
            broker_activity_id: "fee-close-001".to_string(),
            account_id: Uuid::nil(),
            broker_order_id: Some(target_broker_id),
            symbol: Some("TSLA".to_string()),
            activity_type: "FEE".to_string(),
            amount: dec!(4.00),
            occurred_at: now(),
            raw_json: None,
        }],
    );

    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    trust.sync_trade(&trade, &account).unwrap();

    let txs = trust.get_account_transactions(account.id).unwrap();
    let fee_close_txs: Vec<_> = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::FeeClose(_)))
        .collect();
    assert_eq!(
        fee_close_txs.len(),
        1,
        "should have one FeeClose transaction"
    );
    assert_eq!(fee_close_txs[0].amount, dec!(4.00));

    let closed = trust
        .search_trades(account.id, Status::ClosedTarget)
        .unwrap();
    // Performance = target(25000) - entry(20000) - fee(4) = 4996
    assert_eq!(closed[0].balance.total_performance, dec!(4996));

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    // 50000 + 5000 profit - 4 fee = 54996
    assert_eq!(balance.total_balance, dec!(54996));
}

// ===========================================================================
// 6. SHORT TRADE VALIDATION — Edge cases
// ===========================================================================

#[test]
fn test_short_trade_entry_equals_stop_rejected() {
    // entry == stop means zero risk → should be rejected
    let mut trust = setup_facade(NoOpBroker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let tv = create_vehicle(&mut trust, "AAPL");

    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 100,
        currency: Currency::USD,
        category: TradeCategory::Short,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };

    // entry=50, stop=50 → price_diff = 0 → rejected
    let result = trust.create_trade(draft, dec!(50), dec!(50), dec!(40));
    // Trade creation might succeed but funding should fail
    if result.is_ok() {
        let trade = trust
            .search_trades(account.id, Status::New)
            .unwrap()
            .pop()
            .unwrap();
        let fund_result = trust.fund_trade(&trade);
        assert!(
            fund_result.is_err(),
            "funding short trade with entry==stop should fail"
        );
    }
}

#[test]
fn test_short_trade_level_adjustment_skipped() {
    // Short trades skip level-adjusted quantity validation
    // This test verifies that a short trade with quantity exceeding level max still funds
    let mut trust = setup_facade(NoOpBroker);
    trust
        .create_account(
            "test",
            "default",
            model::Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .unwrap();
    let account = trust.search_account("test").unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(100000),
            &Currency::USD,
        )
        .unwrap();
    // Only risk per trade rule, high enough to allow large trades
    trust
        .create_rule(
            &account,
            &RuleName::RiskPerTrade(50.0),
            "high risk",
            &RuleLevel::Error,
        )
        .unwrap();

    let tv = create_vehicle(&mut trust, "TSLA");
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 1000,
        currency: Currency::USD,
        category: TradeCategory::Short,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };

    // Short trade: entry=50, stop=55, target=40, quantity=1000
    // Funding = stop * quantity = 55000
    trust
        .create_trade(draft, dec!(55), dec!(50), dec!(40))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .pop()
        .unwrap();

    // This should succeed because level validation is skipped for shorts
    let result = trust.fund_trade(&trade);
    assert!(
        result.is_ok(),
        "short trade should bypass level-adjusted quantity validation"
    );

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(45000)); // 100000 - 55000
}

// ===========================================================================
// 7. STOP MODIFICATION — Edge cases
// ===========================================================================

#[test]
fn test_modify_stop_to_zero_long_trade() {
    // Long trade: lowering stop increases risk → should be rejected
    let broker = StatefulBroker::new(|trade| {
        let entry = make_filled_entry(trade, dec!(40));
        let target = make_accepted_order(&trade.target);
        let stop = make_held_order(&trade.safety_stop);
        (Status::Filled, vec![entry, target, stop])
    });
    let mut trust = setup_facade(broker.clone());
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    trust.sync_trade(&trade, &account).unwrap();
    let filled = trust
        .search_trades(account.id, Status::Filled)
        .unwrap()
        .pop()
        .unwrap();

    // Try to modify stop to 0 (way below current 38 → risking more)
    let result = trust.modify_stop(&filled, &account, dec!(0));
    assert!(
        result.is_err(),
        "lowering stop on long trade should be rejected"
    );
}

#[test]
fn test_modify_stop_not_filled_rejected() {
    // Can only modify stop on Filled trades
    let mut trust = setup_facade(NoOpBroker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let tv = create_vehicle(&mut trust, "TSLA");
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 500,
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .pop()
        .unwrap();
    trust.fund_trade(&trade).unwrap();
    let funded = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .pop()
        .unwrap();

    let result = trust.modify_stop(&funded, &account, dec!(39));
    assert!(
        result.is_err(),
        "modifying stop on funded (not filled) trade should fail"
    );
}

// ===========================================================================
// 8. MULTIPLE TRADES ON SAME SYMBOL — Balance isolation
// ===========================================================================

#[test]
fn test_two_trades_same_symbol_independent_lifecycles() {
    // First trade fills, second stays submitted
    let call_count = Arc::new(Mutex::new(0u32));
    let call_count_clone = call_count.clone();

    let broker = StatefulBroker::new(move |trade| {
        let mut count = call_count_clone.lock().unwrap();
        *count += 1;
        // First sync: fill entry
        let entry = make_filled_entry(trade, dec!(40));
        let target = make_accepted_order(&trade.target);
        let stop = make_held_order(&trade.safety_stop);
        (Status::Filled, vec![entry, target, stop])
    });
    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(100000), 2.0);

    let trade1 = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    // Create second trade with different vehicle
    let _trade2 = create_submitted_trade(
        &mut trust,
        &account,
        "AAPL",
        dec!(148),
        dec!(150),
        dec!(160),
        200,
    );

    // Only sync trade1
    trust.sync_trade(&trade1, &account).unwrap();

    // Trade1 should be Filled
    let filled = trust.search_trades(account.id, Status::Filled).unwrap();
    assert_eq!(filled.len(), 1);
    assert_eq!(filled[0].trading_vehicle.symbol, "TSLA");

    // Trade2 should still be Submitted
    let submitted = trust.search_trades(account.id, Status::Submitted).unwrap();
    assert_eq!(submitted.len(), 1);
    assert_eq!(submitted[0].trading_vehicle.symbol, "AAPL");

    // Balance should reflect trade states
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    // TSLA filled: in_trade reduces by funding and then increases by capital_in_market
    // AAPL still submitted: in_trade = 0 (submitted releases in_trade delta)
    // The key point: two independent trades don't corrupt each other's balances
    assert!(
        balance.total_in_trade >= dec!(0),
        "total_in_trade should never be negative"
    );
    // Available should account for both trades
    assert!(
        balance.total_available < dec!(100000),
        "available should be less than deposit"
    );
}

// ===========================================================================
// 9. RISK VALIDATION EDGE CASES
// ===========================================================================

#[test]
fn test_trade_exceeding_risk_per_trade_rejected() {
    let mut trust = setup_facade(NoOpBroker);
    // 2% risk per trade on 10000 = 200 max risk
    let account = setup_account_with_rules(&mut trust, dec!(10000), 2.0);
    let tv = create_vehicle(&mut trust, "AAPL");

    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 500, // Risk = (40-38)*500 = 1000, way above 200 limit
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .pop()
        .unwrap();

    let result = trust.fund_trade(&trade);
    assert!(
        result.is_err(),
        "trade exceeding risk limit should be rejected"
    );
}

#[test]
fn test_trade_at_exact_risk_limit_accepted() {
    let mut trust = setup_facade(NoOpBroker);
    // 2% risk on 50000 = 1000 max risk
    // price_diff = 40-38 = 2, quantity = 500, total_risk = 1000
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let tv = create_vehicle(&mut trust, "AAPL");

    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 500,
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .pop()
        .unwrap();

    let result = trust.fund_trade(&trade);
    assert!(
        result.is_ok(),
        "trade at exact risk limit should be accepted"
    );
}

#[test]
fn test_trade_with_zero_quantity_rejected() {
    let mut trust = setup_facade(NoOpBroker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let tv = create_vehicle(&mut trust, "AAPL");

    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 0,
        currency: Currency::USD,
        category: TradeCategory::Long,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };

    let result = trust.create_trade(draft, dec!(38), dec!(40), dec!(50));
    // Zero quantity should be rejected either at trade creation or funding
    if result.is_ok() {
        let trade = trust
            .search_trades(account.id, Status::New)
            .unwrap()
            .pop()
            .unwrap();
        let fund_result = trust.fund_trade(&trade);
        assert!(
            fund_result.is_err(),
            "zero quantity trade should be rejected at funding"
        );
    }
}

// ===========================================================================
// 10. ENTRY PRICE WORSE THAN EXPECTED — Fill costs more than planned
// ===========================================================================

#[test]
fn test_entry_fill_at_higher_price_than_planned_no_refund() {
    // Entry planned at 40, filled at 40.5 → costs more, no refund
    // But still within funding (40*500=20000), so 40.5*500=20250 > 20000...
    // Actually this would exceed funding! Let's verify the behavior.
    let broker = StatefulBroker::new(|trade| {
        let entry = make_filled_entry(trade, dec!(40.5));
        let target = make_accepted_order(&trade.target);
        let stop = make_held_order(&trade.safety_stop);
        (Status::Filled, vec![entry, target, stop])
    });
    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    // Fill at 40.5: total = 40.5 * 500 = 20250, but funding was 40*500=20000
    // This should fail because fill exceeds funding
    let result = trust.sync_trade(&trade, &account);
    assert!(
        result.is_err(),
        "fill exceeding funding should be rejected by can_transfer_fill"
    );

    // Trade should remain Submitted
    let submitted = trust.search_trades(account.id, Status::Submitted).unwrap();
    assert_eq!(submitted.len(), 1);
}

// ===========================================================================
// 11. SIBLING ORDER RECONCILIATION — When one exit fills, other is canceled
// ===========================================================================

#[test]
fn test_target_fill_cancels_stop_order() {
    let broker = StatefulBroker::new(|trade| {
        let entry = make_filled_entry(trade, dec!(40));
        let target = make_filled_target(trade, dec!(50));
        let stop = make_canceled_order(&trade.safety_stop);
        (Status::ClosedTarget, vec![entry, target, stop])
    });
    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    trust.sync_trade(&trade, &account).unwrap();

    let closed = trust
        .search_trades(account.id, Status::ClosedTarget)
        .unwrap()
        .pop()
        .unwrap();

    // Stop order should be marked as Canceled
    assert_eq!(closed.safety_stop.status, OrderStatus::Canceled);
    // Target should be Filled
    assert_eq!(closed.target.status, OrderStatus::Filled);
    // Entry should be Filled
    assert_eq!(closed.entry.status, OrderStatus::Filled);
}

#[test]
fn test_stop_fill_cancels_target_order() {
    let broker = StatefulBroker::new(|trade| {
        let entry = make_filled_entry(trade, dec!(40));
        let stop = make_filled_stop(trade, dec!(38));
        let target = make_canceled_order(&trade.target);
        (Status::ClosedStopLoss, vec![entry, stop, target])
    });
    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    trust.sync_trade(&trade, &account).unwrap();

    let closed = trust
        .search_trades(account.id, Status::ClosedStopLoss)
        .unwrap()
        .pop()
        .unwrap();

    // Target should be marked as Canceled
    assert_eq!(closed.target.status, OrderStatus::Canceled);
    // Stop should be Filled
    assert_eq!(closed.safety_stop.status, OrderStatus::Filled);
}

// ===========================================================================
// 12. TRADE BALANCE CONSERVATION — Verify no money is lost or created
// ===========================================================================

#[test]
fn test_profitable_trade_money_conservation() {
    // Total money in system should always be accounted for:
    // deposit = account_balance + trade_balance_capital_out_market + open_trade_value
    let broker = StatefulBroker::new(|trade| {
        let entry = make_filled_entry(trade, dec!(40));
        let target = make_filled_target(trade, dec!(50));
        let stop = make_canceled_order(&trade.safety_stop);
        (Status::ClosedTarget, vec![entry, target, stop])
    });
    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    trust.sync_trade(&trade, &account).unwrap();

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    let closed = trust
        .search_trades(account.id, Status::ClosedTarget)
        .unwrap()
        .pop()
        .unwrap();

    // After close: all capital should be back in account
    assert_eq!(closed.balance.capital_in_market, dec!(0));
    assert_eq!(closed.balance.capital_out_market, dec!(0));

    // Account balance = initial + profit
    let expected_profit = dec!(5000); // (50-40)*500
    assert_eq!(balance.total_balance, dec!(50000) + expected_profit);
    assert_eq!(balance.total_available, balance.total_balance);
    assert_eq!(balance.total_in_trade, dec!(0));
}

#[test]
fn test_losing_trade_money_conservation() {
    let broker = StatefulBroker::new(|trade| {
        let entry = make_filled_entry(trade, dec!(40));
        let stop = make_filled_stop(trade, dec!(36));
        let target = make_canceled_order(&trade.target);
        (Status::ClosedStopLoss, vec![entry, stop, target])
    });
    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    trust.sync_trade(&trade, &account).unwrap();

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    let closed = trust
        .search_trades(account.id, Status::ClosedStopLoss)
        .unwrap()
        .pop()
        .unwrap();

    let expected_loss = dec!(2000); // (40-36)*500
    assert_eq!(closed.balance.total_performance, dec!(0) - expected_loss);
    assert_eq!(balance.total_balance, dec!(50000) - expected_loss);
    assert_eq!(balance.total_available, balance.total_balance);
    assert_eq!(balance.total_in_trade, dec!(0));
    assert_eq!(closed.balance.capital_in_market, dec!(0));
    assert_eq!(closed.balance.capital_out_market, dec!(0));
}

// ===========================================================================
// 13. SMALL TRADE — Minimum viable amounts
// ===========================================================================

#[test]
fn test_single_share_trade_lifecycle() {
    let broker = StatefulBroker::new(|trade| {
        let entry = make_filled_entry(trade, dec!(100));
        let target = make_filled_target(trade, dec!(105));
        let stop = make_canceled_order(&trade.safety_stop);
        (Status::ClosedTarget, vec![entry, target, stop])
    });
    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(10000), 5.0);
    // 1 share, entry=100, stop=95, target=105, risk = 5*1 = 5
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "AAPL",
        dec!(95),
        dec!(100),
        dec!(105),
        1,
    );

    trust.sync_trade(&trade, &account).unwrap();

    let closed = trust
        .search_trades(account.id, Status::ClosedTarget)
        .unwrap()
        .pop()
        .unwrap();

    assert_eq!(closed.balance.total_performance, dec!(5)); // 105 - 100
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(10005));
}

// ===========================================================================
// 14. INSUFFICIENT FUNDS FOR FEE — Fee exceeds available balance
// ===========================================================================

#[test]
fn test_fee_exceeding_available_balance_fails() {
    // Deposit just enough to fund the trade, then fee should fail
    let entry_broker_id = Uuid::new_v4().to_string();
    let entry_broker_id_clone = entry_broker_id.clone();

    let broker = StatefulBroker::with_fees(
        move |trade| {
            let entry = Order {
                id: trade.entry.id,
                broker_order_id: Some(entry_broker_id_clone.clone()),
                filled_quantity: trade.entry.quantity,
                average_filled_price: Some(dec!(40)),
                status: OrderStatus::Filled,
                filled_at: Some(now()),
                ..Default::default()
            };
            let target = make_accepted_order(&trade.target);
            let stop = make_held_order(&trade.safety_stop);
            (Status::Filled, vec![entry, target, stop])
        },
        vec![FeeActivity {
            broker: "alpaca".to_string(),
            broker_activity_id: "fee-huge".to_string(),
            account_id: Uuid::nil(),
            broker_order_id: Some(entry_broker_id),
            symbol: Some("TSLA".to_string()),
            activity_type: "FEE".to_string(),
            amount: dec!(50000), // Huge fee exceeding available balance
            occurred_at: now(),
            raw_json: None,
        }],
    );

    let mut trust = setup_facade(broker);
    let account = setup_account_with_rules(&mut trust, dec!(50000), 2.0);
    let trade = create_submitted_trade(
        &mut trust,
        &account,
        "TSLA",
        dec!(38),
        dec!(40),
        dec!(50),
        500,
    );

    // Should fail because fee exceeds available balance
    let result = trust.sync_trade(&trade, &account);
    assert!(
        result.is_err(),
        "fee exceeding available balance should fail"
    );
}
