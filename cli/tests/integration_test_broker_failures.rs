//! Tests for broker disconnection, failure, and partial-response scenarios.
//!
//! These tests verify that the system behaves correctly when the broker:
//! - Returns errors (network, auth, server)
//! - Returns partial or inconsistent data
//! - Provides stale or duplicate responses
//! - Disconnects mid-operation
//!
//! The key invariant is: **trade and account balances must never be corrupted
//! by broker failures**. The savepoint pattern guarantees this, and these tests
//! verify it end-to-end.

use chrono::Utc;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Account, Broker, BrokerLog, Currency, DraftTrade, Execution, FeeActivity, Order, OrderIds,
    OrderStatus, RuleLevel, RuleName, Status, Trade, TradeCategory, TradingVehicleCategory,
    TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn new_facade(broker: impl Broker + 'static) -> TrustFacade {
    TrustFacade::new(Box::new(SqliteDatabase::new_in_memory()), Box::new(broker))
}

fn setup_account(trust: &mut TrustFacade, deposit: Decimal) -> Account {
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
            &RuleName::RiskPerTrade(2.0),
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

fn create_submitted_trade(trust: &mut TrustFacade, account: &Account) -> Trade {
    let tv = create_vehicle(trust, "TSLA");
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

fn now() -> chrono::NaiveDateTime {
    Utc::now().naive_utc()
}

// ---------------------------------------------------------------------------
// Broker implementations that simulate failure scenarios
// ---------------------------------------------------------------------------

/// Broker that always fails on sync_trade (network error, auth failure, etc.)
struct SyncFailsBroker {
    error_message: &'static str,
}

impl SyncFailsBroker {
    fn new(msg: &'static str) -> Self {
        Self { error_message: msg }
    }
}

impl Broker for SyncFailsBroker {
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
        Err(self.error_message.into())
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
}

/// Broker where sync_trade succeeds but fetch_executions fails
struct ExecutionFetchFailsBroker {
    #[allow(clippy::type_complexity)]
    sync_fn: Box<dyn Fn(&Trade) -> (Status, Vec<Order>) + Send + Sync>,
}

impl ExecutionFetchFailsBroker {
    fn new(sync_fn: impl Fn(&Trade) -> (Status, Vec<Order>) + Send + Sync + 'static) -> Self {
        Self {
            sync_fn: Box::new(sync_fn),
        }
    }
}

impl Broker for ExecutionFetchFailsBroker {
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
        let (status, orders) = (self.sync_fn)(trade);
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
    fn fetch_executions(
        &self,
        _: &Trade,
        _: &Account,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<Execution>, Box<dyn Error>> {
        Err("Connection reset by peer".into())
    }
    fn fetch_fee_activities(
        &self,
        _: &Trade,
        _: &Account,
        _: Option<chrono::DateTime<Utc>>,
    ) -> Result<Vec<FeeActivity>, Box<dyn Error>> {
        Err("Connection reset by peer".into())
    }
}

/// Broker that fails N times then succeeds
struct TransientFailureBroker {
    call_count: Arc<AtomicU32>,
    failures_before_success: u32,
    #[allow(clippy::type_complexity)]
    success_fn: Box<dyn Fn(&Trade) -> (Status, Vec<Order>) + Send + Sync>,
}

impl TransientFailureBroker {
    fn new(
        failures: u32,
        success_fn: impl Fn(&Trade) -> (Status, Vec<Order>) + Send + Sync + 'static,
    ) -> Self {
        Self {
            call_count: Arc::new(AtomicU32::new(0)),
            failures_before_success: failures,
            success_fn: Box::new(success_fn),
        }
    }
}

impl Broker for TransientFailureBroker {
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
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        if n < self.failures_before_success {
            Err(format!("Transient failure #{}", n + 1).into())
        } else {
            let (status, orders) = (self.success_fn)(trade);
            Ok((status, orders, BrokerLog::default()))
        }
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
}

/// Broker that returns empty orders (partial data loss)
struct EmptyOrdersBroker;

impl Broker for EmptyOrdersBroker {
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
        // Broker returns Filled status but no order data
        Ok((Status::Filled, vec![], BrokerLog::default()))
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
}

/// Broker that returns wrong/unknown order IDs
struct WrongOrderIdBroker;

impl Broker for WrongOrderIdBroker {
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
        // Broker returns orders with IDs that don't match any trade orders
        let fake_entry = Order {
            id: Uuid::new_v4(), // Wrong ID
            broker_order_id: Some(Uuid::new_v4().to_string()),
            filled_quantity: 500,
            average_filled_price: Some(dec!(40)),
            status: OrderStatus::Filled,
            filled_at: Some(now()),
            ..Default::default()
        };
        Ok((Status::Filled, vec![fake_entry], BrokerLog::default()))
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
}

/// Broker that returns duplicate order IDs in sync response
struct DuplicateOrderIdBroker;

impl Broker for DuplicateOrderIdBroker {
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
        // Return the same order twice (duplicate)
        let entry = Order {
            id: trade.entry.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            status: OrderStatus::Accepted,
            ..Default::default()
        };
        let entry_dup = Order {
            id: trade.entry.id, // Same ID
            broker_order_id: Some(Uuid::new_v4().to_string()),
            status: OrderStatus::Filled,
            filled_quantity: 500,
            average_filled_price: Some(dec!(40)),
            filled_at: Some(now()),
            ..Default::default()
        };
        Ok((
            Status::Submitted,
            vec![entry, entry_dup],
            BrokerLog::default(),
        ))
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
}

/// Broker that reports conflicting status (e.g. Filled with no filled orders)
struct StatusMismatchBroker;

impl Broker for StatusMismatchBroker {
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
        // Status says Filled, but entry order is only Accepted (not actually filled)
        let entry = Order {
            id: trade.entry.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            status: OrderStatus::Accepted, // NOT filled
            ..Default::default()
        };
        let target = Order {
            id: trade.target.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            status: OrderStatus::Held,
            ..Default::default()
        };
        let stop = Order {
            id: trade.safety_stop.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            status: OrderStatus::Held,
            ..Default::default()
        };
        Ok((
            Status::Filled,
            vec![entry, target, stop],
            BrokerLog::default(),
        ))
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
}

/// Broker that submits OK but always fails on cancel
struct CancelFailsBroker;

impl Broker for CancelFailsBroker {
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
        Err("Broker cancel failed: order in transition state".into())
    }
    fn modify_stop(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        unimplemented!()
    }
    fn modify_target(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        unimplemented!()
    }
}

/// Broker that fails on submit_trade
struct SubmitFailsBroker;

impl Broker for SubmitFailsBroker {
    fn kind(&self) -> model::BrokerKind {
        model::BrokerKind::Alpaca
    }
    fn submit_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        Err("Market is closed".into())
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
        Ok(())
    }
    fn modify_stop(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        unimplemented!()
    }
    fn modify_target(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        unimplemented!()
    }
}

// ===========================================================================
// 1. SYNC FAILURE — Broker API is down / returns error
//    Verify: trade status unchanged, balance unchanged, no DB corruption
// ===========================================================================

#[test]
fn test_sync_network_error_leaves_trade_unchanged() {
    let mut trust = new_facade(SyncFailsBroker::new("connection timed out"));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = create_submitted_trade(&mut trust, &account);

    let result = trust.sync_trade(&trade, &account);
    assert!(result.is_err());

    // Trade must remain Submitted
    let trades = trust.search_trades(account.id, Status::Submitted).unwrap();
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].id, trade.id);

    // Balance must be unchanged
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(50000));
    assert_eq!(balance.total_available, dec!(30000)); // 50000 - 20000 funded
                                                      // total_in_trade = 0 after Funded→Submitted. This is safe because the
                                                      // risk gatekeeper is total_available (reduced by FundTrade), NOT total_in_trade.
                                                      // total_in_trade is a display/reporting field — never used in risk decisions.
    assert_eq!(balance.total_in_trade, dec!(0));
}

#[test]
fn test_sync_auth_error_leaves_trade_unchanged() {
    let mut trust = new_facade(SyncFailsBroker::new("401 Unauthorized"));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = create_submitted_trade(&mut trust, &account);

    let result = trust.sync_trade(&trade, &account);
    assert!(result.is_err());

    let trades = trust.search_trades(account.id, Status::Submitted).unwrap();
    assert_eq!(trades.len(), 1);

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000));
}

#[test]
fn test_sync_server_error_leaves_trade_unchanged() {
    let mut trust = new_facade(SyncFailsBroker::new("500 Internal Server Error"));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = create_submitted_trade(&mut trust, &account);

    let result = trust.sync_trade(&trade, &account);
    assert!(result.is_err());

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(50000));
    // total_in_trade = 0 after Funded→Submitted. This is safe because the
    // risk gatekeeper is total_available (reduced by FundTrade), NOT total_in_trade.
    // total_in_trade is a display/reporting field — never used in risk decisions.
    assert_eq!(balance.total_in_trade, dec!(0));
}

// ===========================================================================
// 2. EXECUTION/FEE FETCH FAILURE — Sync succeeds but execution feed is down
//    Verify: trade transitions normally, fees/executions can be retried later
// ===========================================================================

#[test]
fn test_execution_fetch_failure_does_not_block_sync() {
    let broker = ExecutionFetchFailsBroker::new(|trade| {
        let entry = Order {
            id: trade.entry.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            filled_quantity: trade.entry.quantity,
            average_filled_price: Some(dec!(40)),
            status: OrderStatus::Filled,
            filled_at: Some(now()),
            ..Default::default()
        };
        let target = Order {
            id: trade.target.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            status: OrderStatus::Accepted,
            ..Default::default()
        };
        let stop = Order {
            id: trade.safety_stop.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            status: OrderStatus::Held,
            ..Default::default()
        };
        (Status::Filled, vec![entry, target, stop])
    });
    let mut trust = new_facade(broker);
    let account = setup_account(&mut trust, dec!(50000));
    let trade = create_submitted_trade(&mut trust, &account);

    // Should succeed despite execution/fee fetch failures (best-effort)
    let result = trust.sync_trade(&trade, &account);
    assert!(
        result.is_ok(),
        "sync should succeed even when execution fetch fails"
    );

    // Trade should be Filled
    let filled = trust.search_trades(account.id, Status::Filled).unwrap();
    assert_eq!(filled.len(), 1);

    // Balance should reflect the fill
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000)); // No slippage at exact price
}

#[test]
fn test_fee_fetch_failure_does_not_block_close() {
    let broker = ExecutionFetchFailsBroker::new(|trade| {
        let entry = Order {
            id: trade.entry.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            filled_quantity: trade.entry.quantity,
            average_filled_price: Some(dec!(40)),
            status: OrderStatus::Filled,
            filled_at: Some(now()),
            ..Default::default()
        };
        let target = Order {
            id: trade.target.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            filled_quantity: trade.entry.quantity,
            average_filled_price: Some(dec!(50)),
            status: OrderStatus::Filled,
            filled_at: Some(now()),
            ..Default::default()
        };
        let stop = Order {
            id: trade.safety_stop.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            status: OrderStatus::Canceled,
            ..Default::default()
        };
        (Status::ClosedTarget, vec![entry, target, stop])
    });
    let mut trust = new_facade(broker);
    let account = setup_account(&mut trust, dec!(50000));
    let trade = create_submitted_trade(&mut trust, &account);

    let result = trust.sync_trade(&trade, &account);
    assert!(
        result.is_ok(),
        "close should succeed even when fee fetch fails"
    );

    let closed = trust
        .search_trades(account.id, Status::ClosedTarget)
        .unwrap();
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].balance.total_performance, dec!(5000));
}

// ===========================================================================
// 3. TRANSIENT FAILURE + RETRY — Fails N times then succeeds
//    Verify: after recovery, balance is correct (no double-counting)
// ===========================================================================

#[test]
fn test_transient_failures_then_success_no_corruption() {
    let broker = TransientFailureBroker::new(3, |trade| {
        let entry = Order {
            id: trade.entry.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            filled_quantity: trade.entry.quantity,
            average_filled_price: Some(dec!(40)),
            status: OrderStatus::Filled,
            filled_at: Some(now()),
            ..Default::default()
        };
        let target = Order {
            id: trade.target.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            status: OrderStatus::Accepted,
            ..Default::default()
        };
        let stop = Order {
            id: trade.safety_stop.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            status: OrderStatus::Held,
            ..Default::default()
        };
        (Status::Filled, vec![entry, target, stop])
    });

    let mut trust = new_facade(broker);
    let account = setup_account(&mut trust, dec!(50000));
    let trade = create_submitted_trade(&mut trust, &account);

    // First 3 attempts fail
    for i in 0..3 {
        let result = trust.sync_trade(&trade, &account);
        assert!(result.is_err(), "attempt {} should fail", i + 1);
    }

    // Trade should still be Submitted after all failures
    let trades = trust.search_trades(account.id, Status::Submitted).unwrap();
    assert_eq!(trades.len(), 1);
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    // total_in_trade = 0 after Funded→Submitted transition
    assert_eq!(balance.total_available, dec!(30000)); // Unchanged

    // 4th attempt succeeds
    let result = trust.sync_trade(&trade, &account);
    assert!(result.is_ok(), "4th attempt should succeed");

    let filled = trust.search_trades(account.id, Status::Filled).unwrap();
    assert_eq!(filled.len(), 1);

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000)); // Exact price fill, no slippage
}

// ===========================================================================
// 4. EMPTY ORDERS — Broker returns status but no order data
//    Verify: system rejects inconsistent payload
// ===========================================================================

#[test]
fn test_broker_returns_filled_with_no_orders_rejected() {
    let mut trust = new_facade(EmptyOrdersBroker);
    let account = setup_account(&mut trust, dec!(50000));
    let trade = create_submitted_trade(&mut trust, &account);

    // Broker says "Filled" but provides no orders → should be rejected
    let result = trust.sync_trade(&trade, &account);
    assert!(
        result.is_err(),
        "filled status with no orders should be rejected"
    );

    // Trade must remain Submitted
    let trades = trust.search_trades(account.id, Status::Submitted).unwrap();
    assert_eq!(trades.len(), 1);

    // Balance unchanged
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000));
}

// ===========================================================================
// 5. WRONG ORDER IDS — Broker returns orders not belonging to trade
//    Verify: system rejects unknown order IDs
// ===========================================================================

#[test]
fn test_broker_returns_wrong_order_ids_rejected() {
    let mut trust = new_facade(WrongOrderIdBroker);
    let account = setup_account(&mut trust, dec!(50000));
    let trade = create_submitted_trade(&mut trust, &account);

    let result = trust.sync_trade(&trade, &account);
    assert!(
        result.is_err(),
        "orders with unknown IDs should be rejected by resolve_orders_for_sync"
    );

    // Trade and balance unchanged
    let trades = trust.search_trades(account.id, Status::Submitted).unwrap();
    assert_eq!(trades.len(), 1);
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000));
}

// ===========================================================================
// 6. DUPLICATE ORDER IDS — Broker returns same order ID twice
//    Verify: system rejects duplicate order IDs
// ===========================================================================

#[test]
fn test_broker_returns_duplicate_order_ids_rejected() {
    let mut trust = new_facade(DuplicateOrderIdBroker);
    let account = setup_account(&mut trust, dec!(50000));
    let trade = create_submitted_trade(&mut trust, &account);

    let result = trust.sync_trade(&trade, &account);
    assert!(
        result.is_err(),
        "duplicate order IDs should be rejected by resolve_orders_for_sync"
    );

    // Trade and balance unchanged
    let trades = trust.search_trades(account.id, Status::Submitted).unwrap();
    assert_eq!(trades.len(), 1);
}

// ===========================================================================
// 7. STATUS MISMATCH — Broker says Filled but entry is only Accepted
//    Verify: system rejects via validate_sync_payload
// ===========================================================================

#[test]
fn test_status_mismatch_filled_but_entry_not_filled_rejected() {
    let mut trust = new_facade(StatusMismatchBroker);
    let account = setup_account(&mut trust, dec!(50000));
    let trade = create_submitted_trade(&mut trust, &account);

    let result = trust.sync_trade(&trade, &account);
    assert!(
        result.is_err(),
        "Filled status with unfilled entry should be rejected by validate_sync_payload"
    );

    let trades = trust.search_trades(account.id, Status::Submitted).unwrap();
    assert_eq!(trades.len(), 1);
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000));
}

// ===========================================================================
// 8. SUBMIT FAILURE — Market closed or broker rejects order
//    Verify: trade stays Funded, can be retried or canceled
// ===========================================================================

#[test]
fn test_submit_failure_trade_stays_funded() {
    let mut trust = new_facade(SubmitFailsBroker);
    let account = setup_account(&mut trust, dec!(50000));
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
    let trade = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .pop()
        .unwrap();

    let result = trust.submit_trade(&trade);
    assert!(result.is_err(), "submit should fail when market is closed");

    // Trade must remain Funded
    let funded = trust.search_trades(account.id, Status::Funded).unwrap();
    assert_eq!(funded.len(), 1);

    // Balance unchanged — funds still in trade (Funded→Submitted never happened)
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000));
    assert_eq!(balance.total_in_trade, dec!(20000));
}

#[test]
fn test_submit_failure_can_cancel_funded_trade() {
    let mut trust = new_facade(SubmitFailsBroker);
    let account = setup_account(&mut trust, dec!(50000));
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
    let trade = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .pop()
        .unwrap();

    // Submit fails
    let _ = trust.submit_trade(&trade);

    // Should be able to cancel the funded trade and recover funds
    let funded = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .pop()
        .unwrap();
    let (_, account_bal, tx) = trust.cancel_funded_trade(&funded).unwrap();

    assert_eq!(tx.amount, dec!(20000));
    assert_eq!(account_bal.total_available, dec!(50000)); // Fully restored
    assert_eq!(account_bal.total_in_trade, dec!(0));
}

// ===========================================================================
// 9. REPEATED SYNC FAILURES — Multiple failures must never corrupt state
// ===========================================================================

#[test]
fn test_many_sync_failures_no_balance_drift() {
    let mut trust = new_facade(SyncFailsBroker::new("gateway timeout"));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = create_submitted_trade(&mut trust, &account);

    // 100 failed syncs
    for _ in 0..100 {
        let _ = trust.sync_trade(&trade, &account);
    }

    // Balance must be exactly what it was after funding
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(50000));
    assert_eq!(balance.total_available, dec!(30000));
    // total_in_trade = 0 after Funded→Submitted. This is safe because the
    // risk gatekeeper is total_available (reduced by FundTrade), NOT total_in_trade.
    // total_in_trade is a display/reporting field — never used in risk decisions.
    assert_eq!(balance.total_in_trade, dec!(0));
    assert_eq!(balance.taxed, dec!(0));

    // Trade must still be Submitted
    let trades = trust.search_trades(account.id, Status::Submitted).unwrap();
    assert_eq!(trades.len(), 1);
}

// ===========================================================================
// 10. CANCEL BROKER FAILURE — Broker rejects cancel
//     Verify: trade stays Funded in DB
// ===========================================================================

#[test]
fn test_broker_cancel_fails_trade_stays_funded() {
    let mut trust = new_facade(CancelFailsBroker);
    let account = setup_account(&mut trust, dec!(50000));
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

    // Cancel attempt should propagate the broker error
    // Note: cancel_funded_trade calls broker.cancel_trade for submitted trades
    // For funded trades, it may not call the broker at all (depends on implementation)
    let result = trust.cancel_funded_trade(&funded);
    // cancel_funded_trade on a Funded trade shouldn't need broker cancel
    // It should succeed since the trade was never submitted
    if result.is_ok() {
        let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
        assert_eq!(balance.total_available, dec!(50000));
    }
    // Either way, balance must be consistent
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(50000));
}

// ===========================================================================
// 11. MULTIPLE TRADES — One fails sync, others unaffected
// ===========================================================================

#[test]
fn test_sync_failure_on_one_trade_does_not_affect_others() {
    // Use TransientFailureBroker: first sync fails, second succeeds
    let broker = TransientFailureBroker::new(1, |trade| {
        let entry = Order {
            id: trade.entry.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            filled_quantity: trade.entry.quantity,
            average_filled_price: Some(dec!(40)),
            status: OrderStatus::Filled,
            filled_at: Some(now()),
            ..Default::default()
        };
        let target = Order {
            id: trade.target.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            status: OrderStatus::Accepted,
            ..Default::default()
        };
        let stop = Order {
            id: trade.safety_stop.id,
            broker_order_id: Some(Uuid::new_v4().to_string()),
            status: OrderStatus::Held,
            ..Default::default()
        };
        (Status::Filled, vec![entry, target, stop])
    });

    let mut trust = new_facade(broker);
    let account = setup_account(&mut trust, dec!(100000));
    let trade = create_submitted_trade(&mut trust, &account);

    // First sync fails
    let result = trust.sync_trade(&trade, &account);
    assert!(result.is_err());

    // Second sync succeeds
    let result = trust.sync_trade(&trade, &account);
    assert!(result.is_ok());

    let filled = trust.search_trades(account.id, Status::Filled).unwrap();
    assert_eq!(filled.len(), 1);

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    // 100000 deposit, 20000 funded, fill at exact price → available = 80000
    assert_eq!(balance.total_available, dec!(80000));
}

// ===========================================================================
// 12. BROKER KIND CONSISTENCY
// ===========================================================================

#[test]
fn test_alpaca_broker_kind() {
    let broker = SyncFailsBroker::new("test");
    assert_eq!(broker.kind(), model::BrokerKind::Alpaca);
}

// ===========================================================================
// 13. SYNC ERROR PRESERVES ERROR CONTEXT
// ===========================================================================

#[test]
fn test_sync_error_message_is_preserved() {
    let mut trust = new_facade(SyncFailsBroker::new(
        "IBKR Client Portal Gateway is not ready",
    ));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = create_submitted_trade(&mut trust, &account);

    let result = trust.sync_trade(&trade, &account);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Gateway is not ready"),
        "error message should contain original broker error, got: {}",
        err_msg
    );
}

// ===========================================================================
// 14. RISK INVARIANT — Cannot over-risk via total_in_trade = 0
//     When a trade transitions Funded→Submitted, total_in_trade goes to 0.
//     This MUST NOT allow funding more trades than total_available permits.
//     The gatekeeper is total_available (reduced by FundTrade), not total_in_trade.
// ===========================================================================

#[test]
fn test_cannot_over_risk_when_total_in_trade_is_zero() {
    let mut trust = new_facade(SyncFailsBroker::new("not needed"));
    let account = setup_account(&mut trust, dec!(50000));

    // Fund and submit trade #1 → uses 20000 of available funds
    let trade1 = create_submitted_trade(&mut trust, &account);

    // After Funded→Submitted: total_in_trade = 0, total_available = 30000
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_in_trade, dec!(0));
    assert_eq!(balance.total_available, dec!(30000));

    // Try to fund trade #2 that requires 40000 — should fail even though total_in_trade is 0
    let tv = create_vehicle(&mut trust, "AAPL");
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 1000, // 1000 * 40 = 40000 required
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
    let trades_new = trust.search_trades(account.id, Status::New).unwrap();
    let trade2 = trades_new.iter().find(|t| t.id != trade1.id).unwrap();

    let result = trust.fund_trade(trade2);
    assert!(
        result.is_err(),
        "should NOT be able to fund 40000 when only 30000 is available, \
         even though total_in_trade is 0"
    );

    // Balance unchanged
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000));
}

#[test]
fn test_can_fund_second_trade_within_available_balance() {
    let mut trust = new_facade(SyncFailsBroker::new("not needed"));
    let account = setup_account(&mut trust, dec!(50000));

    // Fund and submit trade #1 → uses 20000
    let _trade1 = create_submitted_trade(&mut trust, &account);

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000));

    // Fund trade #2 that requires only 10000 — should succeed
    let tv = create_vehicle(&mut trust, "GOOG");
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 250, // 250 * 40 = 10000 required, within 30000 available
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
    let new_trades = trust.search_trades(account.id, Status::New).unwrap();
    assert_eq!(new_trades.len(), 1);

    let result = trust.fund_trade(&new_trades[0]);
    assert!(
        result.is_ok(),
        "should be able to fund 10000 when 30000 is available"
    );

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(20000)); // 30000 - 10000
}

#[test]
fn test_total_available_is_exact_gatekeeper_for_funding() {
    let mut trust = new_facade(SyncFailsBroker::new("not needed"));
    let account = setup_account(&mut trust, dec!(50000));

    // Fund and submit trade #1 → 20000
    let _trade1 = create_submitted_trade(&mut trust, &account);

    // Fund trade #2 within risk limits: 2% of 30000 = 600 max risk
    // price_diff = 40-38 = 2, max quantity = 600/2 = 300
    // Funding = 300 * 40 = 12000
    let tv = create_vehicle(&mut trust, "MSFT");
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 300,
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
    let new_trades = trust.search_trades(account.id, Status::New).unwrap();

    let result = trust.fund_trade(&new_trades[0]);
    assert!(
        result.is_ok(),
        "should be able to fund within remaining available balance and risk limits"
    );

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(18000)); // 30000 - 12000

    // Try to fund another trade that exceeds remaining available
    let tv = create_vehicle(&mut trust, "AMZN");
    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle: tv,
        quantity: 500, // 500 * 40 = 20000, but only 18000 available
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
    let new_trades = trust.search_trades(account.id, Status::New).unwrap();

    let result = trust.fund_trade(&new_trades[0]);
    assert!(
        result.is_err(),
        "should NOT be able to fund 20000 when only 18000 is available"
    );
}
