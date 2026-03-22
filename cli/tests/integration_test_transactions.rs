use chrono::Utc;
use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Account, Broker, BrokerLog, Currency, DraftTrade, Order, OrderIds, OrderStatus, RuleLevel,
    RuleName, Status, Trade, TradeCategory, TradingVehicleCategory, TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn new_facade(broker: impl Broker + 'static) -> TrustFacade {
    TrustFacade::new(
        Box::new(SqliteDatabase::new_in_memory()),
        Box::new(broker),
    )
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
        .expect("create account");
    let account = trust.search_account("test").unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            deposit,
            &Currency::USD,
        )
        .expect("deposit");
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

/// Create a long trade through the full lifecycle up to Submitted status.
/// stop=38, entry=40, target=50, quantity=500
fn setup_submitted_trade(trust: &mut TrustFacade, account: &Account) -> Trade {
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

// ---------------------------------------------------------------------------
// Mock Brokers
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
    fn close_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
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

struct SyncBroker {
    response: fn(&Trade) -> (Status, Vec<Order>),
}

impl SyncBroker {
    fn new(response: fn(&Trade) -> (Status, Vec<Order>)) -> Self {
        Self { response }
    }
}

impl Broker for SyncBroker {
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
        let (status, orders) = (self.response)(trade);
        Ok((status, orders, BrokerLog::default()))
    }
    fn close_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
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

// ---------------------------------------------------------------------------
// Broker response helpers
// ---------------------------------------------------------------------------

fn entry_filled_at(trade: &Trade, price: Decimal) -> (Status, Vec<Order>) {
    let entry = Order {
        id: trade.entry.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        filled_quantity: trade.entry.quantity,
        average_filled_price: Some(price),
        status: OrderStatus::Filled,
        filled_at: Some(Utc::now().naive_utc()),
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
}

fn target_filled_at(
    trade: &Trade,
    entry_price: Decimal,
    target_price: Decimal,
) -> (Status, Vec<Order>) {
    let entry = Order {
        id: trade.entry.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        filled_quantity: trade.entry.quantity,
        average_filled_price: Some(entry_price),
        status: OrderStatus::Filled,
        filled_at: Some(Utc::now().naive_utc()),
        ..Default::default()
    };
    let target = Order {
        id: trade.target.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        filled_quantity: trade.entry.quantity,
        average_filled_price: Some(target_price),
        status: OrderStatus::Filled,
        filled_at: Some(Utc::now().naive_utc()),
        ..Default::default()
    };
    let stop = Order {
        id: trade.safety_stop.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        status: OrderStatus::Canceled,
        ..Default::default()
    };
    (Status::ClosedTarget, vec![entry, target, stop])
}

fn stop_filled_at(
    trade: &Trade,
    entry_price: Decimal,
    stop_price: Decimal,
) -> (Status, Vec<Order>) {
    let entry = Order {
        id: trade.entry.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        filled_quantity: trade.entry.quantity,
        average_filled_price: Some(entry_price),
        status: OrderStatus::Filled,
        filled_at: Some(Utc::now().naive_utc()),
        ..Default::default()
    };
    let target = Order {
        id: trade.target.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        status: OrderStatus::Canceled,
        ..Default::default()
    };
    let stop = Order {
        id: trade.safety_stop.id,
        broker_order_id: Some(Uuid::new_v4().to_string()),
        filled_quantity: trade.entry.quantity,
        average_filled_price: Some(stop_price),
        status: OrderStatus::Filled,
        filled_at: Some(Utc::now().naive_utc()),
        ..Default::default()
    };
    (Status::ClosedStopLoss, vec![entry, target, stop])
}

// ===========================================================================
// 1. TRANSACTION RECORD ASSERTIONS
//    Existing tests only check balance outcomes. These tests verify the
//    actual Transaction records (category, amount, currency, count).
//
//    NOTE: get_account_transactions() uses `read_all_trade_transactions_excluding_taxes`
//    which only returns: deposit, withdrawal, withdrawal_earnings, fee_open,
//    fee_close, fund_trade, payment_from_trade, payment_earnings.
//    It EXCLUDES: open_trade, close_target, close_safety_stop,
//    close_safety_stop_slippage, payment_tax, withdrawal_tax.
//    We verify excluded categories via balance/trade balance assertions.
// ===========================================================================

#[test]
fn test_deposit_creates_correct_transaction_record() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    let (tx, balance) = trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1234.56),
            &Currency::USD,
        )
        .unwrap();

    assert_eq!(tx.category, TransactionCategory::Deposit);
    assert_eq!(tx.amount, dec!(1234.56));
    assert_eq!(tx.currency, Currency::USD);
    assert_eq!(tx.account_id, account.id);

    assert_eq!(balance.total_balance, dec!(1234.56));
    assert_eq!(balance.total_available, dec!(1234.56));
    assert_eq!(balance.total_in_trade, dec!(0));
    assert_eq!(balance.taxed, dec!(0));
}

#[test]
fn test_withdrawal_creates_correct_transaction_record() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(5000),
            &Currency::USD,
        )
        .unwrap();

    let (tx, balance) = trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(1200),
            &Currency::USD,
        )
        .unwrap();

    assert_eq!(tx.category, TransactionCategory::Withdrawal);
    assert_eq!(tx.amount, dec!(1200));
    assert_eq!(tx.currency, Currency::USD);
    assert_eq!(balance.total_balance, dec!(3800));
    assert_eq!(balance.total_available, dec!(3800));
}

#[test]
fn test_fund_trade_creates_fund_trade_transaction_with_correct_amount() {
    let mut trust = new_facade(NoOpBroker);
    let account = setup_account(&mut trust, dec!(50000));
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
    // stop=38, entry=40, target=50 → funding = entry * quantity = 40*500 = 20000
    trust
        .create_trade(draft, dec!(38), dec!(40), dec!(50))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .pop()
        .unwrap();

    let (_, tx, _, _) = trust.fund_trade(&trade).unwrap();

    assert_eq!(tx.category, TransactionCategory::FundTrade(trade.id));
    assert_eq!(tx.amount, dec!(20000));
    assert_eq!(tx.currency, Currency::USD);

    // Verify balance after funding
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000)); // 50000 - 20000
    assert_eq!(balance.total_balance, dec!(50000)); // FundTrade doesn't change total_balance
    assert_eq!(balance.total_in_trade, dec!(20000));
}

#[test]
fn test_cancel_funded_trade_returns_exact_funded_amount() {
    let mut trust = new_facade(NoOpBroker);
    let account = setup_account(&mut trust, dec!(50000));
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
    trust.fund_trade(&trade).unwrap();
    let trade = trust
        .search_trades(account.id, Status::Funded)
        .unwrap()
        .pop()
        .unwrap();

    let (trade_bal, account_bal, tx) = trust.cancel_funded_trade(&trade).unwrap();

    // Transaction record
    assert_eq!(tx.category, TransactionCategory::PaymentFromTrade(trade.id));
    assert_eq!(tx.amount, dec!(20000)); // Exact funding amount returned
    assert_eq!(tx.currency, Currency::USD);

    // Account fully restored
    assert_eq!(account_bal.total_balance, dec!(50000));
    assert_eq!(account_bal.total_available, dec!(50000));
    assert_eq!(account_bal.total_in_trade, dec!(0));

    // Trade balance zeroed out
    assert_eq!(trade_bal.capital_out_market, dec!(0));
    assert_eq!(trade_bal.capital_in_market, dec!(0));
    assert_eq!(trade_bal.total_performance, dec!(0));
}

// ===========================================================================
// 2. ENTRY FILL WITH SLIPPAGE — Verify via balance and queryable transactions
// ===========================================================================

#[test]
fn test_entry_fill_with_slippage_creates_payment_from_trade() {
    // Entry at 40, filled at 39.9 → slippage refund of 50 (0.1 * 500)
    let mut trust = new_facade(SyncBroker::new(|t| entry_filled_at(t, dec!(39.9))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    // PaymentFromTrade IS included in get_account_transactions
    let txs = trust.get_account_transactions(account.id).unwrap();
    let refund_txs: Vec<_> = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::PaymentFromTrade(_)))
        .collect();
    assert_eq!(
        refund_txs.len(),
        1,
        "should have one PaymentFromTrade (slippage refund)"
    );
    assert_eq!(refund_txs[0].amount, dec!(50)); // (40 - 39.9) * 500

    // Verify balance reflects the refund
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30050)); // 30000 + 50 refund

    // Verify trade balance: capital_in_market should be 19950 (39.9 * 500)
    let filled_trade = trust
        .search_trades(account.id, Status::Filled)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(filled_trade.balance.capital_in_market, dec!(19950));
}

#[test]
fn test_entry_fill_exact_price_no_slippage_refund() {
    // Entry at 40, filled at exactly 40 → no PaymentFromTrade
    let mut trust = new_facade(SyncBroker::new(|t| entry_filled_at(t, dec!(40))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let txs = trust.get_account_transactions(account.id).unwrap();
    let refund_txs: Vec<_> = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::PaymentFromTrade(_)))
        .collect();
    assert_eq!(
        refund_txs.len(),
        0,
        "exact fill should create no slippage refund"
    );

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000)); // No refund
    assert_eq!(balance.total_in_trade, dec!(20000));

    // Trade balance: capital_in_market = 40 * 500 = 20000
    let filled_trade = trust
        .search_trades(account.id, Status::Filled)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(filled_trade.balance.capital_in_market, dec!(20000));
    assert_eq!(filled_trade.balance.capital_out_market, dec!(0));
}

// ===========================================================================
// 3. TARGET CLOSE — Verify performance and final balances
// ===========================================================================

#[test]
fn test_target_close_creates_correct_performance_and_balances() {
    // Entry at 39.9, target at 52.9 → profit
    let mut trust = new_facade(SyncBroker::new(|t| {
        target_filled_at(t, dec!(39.9), dec!(52.9))
    }));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let closed_trade = trust
        .search_trades(account.id, Status::ClosedTarget)
        .unwrap()
        .pop()
        .unwrap();

    // Trade performance = close(52.9*500) - open(39.9*500) = 26450 - 19950 = 6500
    assert_eq!(closed_trade.balance.total_performance, dec!(6500));
    assert_eq!(closed_trade.balance.capital_in_market, dec!(0));
    assert_eq!(closed_trade.balance.capital_out_market, dec!(0)); // All returned

    // Verify PaymentFromTrade transactions (queryable)
    let txs = trust.get_account_transactions(account.id).unwrap();
    let payment_txs: Vec<_> = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::PaymentFromTrade(_)))
        .collect();
    // Slippage refund (50) + final return
    assert!(
        !payment_txs.is_empty(),
        "should have PaymentFromTrade transactions"
    );

    // Verify final account balance
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(56500)); // 50000 + 6500 profit
    assert_eq!(balance.total_balance, dec!(56500));
    assert_eq!(balance.total_in_trade, dec!(0));
}

// ===========================================================================
// 4. STOP LOSS — Verify category via trade balance and account balance
// ===========================================================================

#[test]
fn test_stop_filled_at_planned_price_balance_correct() {
    // Stop planned at 38, filled at exactly 38
    let mut trust = new_facade(SyncBroker::new(|t| stop_filled_at(t, dec!(39.9), dec!(38))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let closed = trust
        .search_trades(account.id, Status::ClosedStopLoss)
        .unwrap()
        .pop()
        .unwrap();

    // Performance = stop(38*500) - open(39.9*500) = 19000 - 19950 = -950
    assert_eq!(closed.balance.total_performance, dec!(-950));
    assert_eq!(closed.balance.capital_in_market, dec!(0));

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    // 50000 + slippage_refund(50) - loss(950) = 49100
    // Wait: entry at 39.9, stop at 38
    // deposit=50000, fund=20000 → available=30000
    // fill: open=19950, refund=50 → available=30050
    // stop: close=19000, return = capital_out_market → available=30050 + returned
    // After close: total_balance = 50000 - 19950 + 19000 = 49050
    // But also the slippage refund doesn't go to total_balance (PaymentFromTrade doesn't affect total_balance)
    // Let me verify by just checking what the test gives us
    assert_eq!(balance.total_in_trade, dec!(0));
    // Net P&L = -950, so final balance = 50000 - 950 = 49050
    assert_eq!(balance.total_balance, dec!(49050));
}

#[test]
fn test_stop_filled_below_planned_price_bigger_loss() {
    // Stop planned at 38, filled at 37 → worse for long
    let mut trust = new_facade(SyncBroker::new(|t| stop_filled_at(t, dec!(39.9), dec!(37))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let closed = trust
        .search_trades(account.id, Status::ClosedStopLoss)
        .unwrap()
        .pop()
        .unwrap();

    // Performance = stop(37*500) - open(39.9*500) = 18500 - 19950 = -1450
    assert_eq!(closed.balance.total_performance, dec!(-1450));
    assert_eq!(closed.balance.capital_in_market, dec!(0));

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    // Net P&L = -1450, final balance = 50000 - 1450 = 48550
    assert_eq!(balance.total_balance, dec!(48550));
    assert_eq!(balance.total_in_trade, dec!(0));
}

#[test]
fn test_stop_filled_above_planned_price_better_than_expected() {
    // Stop planned at 38, filled at 39 → better fill for long (less loss)
    // This is the "slippage" category case: total(39*500=19500) > planned(38*500=19000)
    let mut trust = new_facade(SyncBroker::new(|t| stop_filled_at(t, dec!(39.9), dec!(39))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let closed = trust
        .search_trades(account.id, Status::ClosedStopLoss)
        .unwrap()
        .pop()
        .unwrap();

    // Performance = stop(39*500) - open(39.9*500) = 19500 - 19950 = -450
    assert_eq!(closed.balance.total_performance, dec!(-450));
    assert_eq!(closed.balance.capital_in_market, dec!(0));

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    // 50000 - 450 = 49550
    assert_eq!(balance.total_balance, dec!(49550));
    assert_eq!(balance.total_in_trade, dec!(0));
}

// ===========================================================================
// 5. TRANSACTION COUNT INTEGRITY — Verify queryable transactions only
//    (deposit, withdrawal, fund_trade, payment_from_trade, fee_open, fee_close)
// ===========================================================================

#[test]
fn test_profitable_trade_lifecycle_queryable_transaction_count() {
    // Exact price fill + target close
    let mut trust = new_facade(SyncBroker::new(|t| target_filled_at(t, dec!(40), dec!(50))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let txs = trust.get_account_transactions(account.id).unwrap();

    // Queryable transactions:
    // 1. Deposit (setup)
    // 2. FundTrade (fund_trade)
    // 3. PaymentFromTrade (final return to account after close)
    // NOT included: OpenTrade, CloseTarget (excluded by query)
    let deposit_count = txs
        .iter()
        .filter(|t| t.category == TransactionCategory::Deposit)
        .count();
    let fund_count = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::FundTrade(_)))
        .count();
    let payment_count = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::PaymentFromTrade(_)))
        .count();

    assert_eq!(deposit_count, 1, "exactly one Deposit");
    assert_eq!(fund_count, 1, "exactly one FundTrade");
    assert_eq!(payment_count, 1, "exactly one PaymentFromTrade (final return)");
}

#[test]
fn test_profitable_trade_with_slippage_has_two_payments() {
    // Entry fill with slippage adds an extra PaymentFromTrade
    let mut trust = new_facade(SyncBroker::new(|t| {
        target_filled_at(t, dec!(39.9), dec!(52.9))
    }));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let txs = trust.get_account_transactions(account.id).unwrap();
    let payment_count = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::PaymentFromTrade(_)))
        .count();
    assert_eq!(
        payment_count, 2,
        "two PaymentFromTrade: slippage refund + final return"
    );
}

#[test]
fn test_losing_trade_lifecycle_queryable_transaction_count() {
    let mut trust = new_facade(SyncBroker::new(|t| stop_filled_at(t, dec!(40), dec!(38))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let txs = trust.get_account_transactions(account.id).unwrap();

    // Queryable: Deposit, FundTrade, PaymentFromTrade (final return)
    let fund_count = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::FundTrade(_)))
        .count();
    let payment_count = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::PaymentFromTrade(_)))
        .count();

    assert_eq!(fund_count, 1);
    assert_eq!(payment_count, 1);
}

// ===========================================================================
// 6. EDGE CASES — Withdrawal and deposit boundary conditions
// ===========================================================================

#[test]
fn test_withdraw_exact_available_balance() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    let (tx, balance) = trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    assert_eq!(tx.amount, dec!(1000));
    assert_eq!(balance.total_balance, dec!(0));
    assert_eq!(balance.total_available, dec!(0));
}

#[test]
fn test_withdraw_more_than_available_fails() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Withdrawal,
        dec!(1000.01),
        &Currency::USD,
    );
    assert!(result.is_err(), "withdrawal exceeding available should fail");
}

#[test]
fn test_withdraw_zero_fails() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Withdrawal,
        dec!(0),
        &Currency::USD,
    );
    assert!(result.is_err(), "zero withdrawal should be rejected");
}

#[test]
fn test_withdraw_negative_fails() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Withdrawal,
        dec!(-100),
        &Currency::USD,
    );
    assert!(result.is_err(), "negative withdrawal should be rejected");
}

#[test]
fn test_deposit_negative_fails() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Deposit,
        dec!(-100),
        &Currency::USD,
    );
    assert!(result.is_err(), "negative deposit should be rejected");
}

#[test]
fn test_deposit_zero_succeeds() {
    // The validator allows amount >= 0 for deposits (is_sign_negative check)
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Deposit,
        dec!(0),
        &Currency::USD,
    );
    assert!(
        result.is_ok(),
        "zero deposit should be allowed by current validation"
    );
}

#[test]
fn test_manually_creating_fund_trade_transaction_is_rejected() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    let result = trust.create_transaction(
        &account,
        &TransactionCategory::FundTrade(Uuid::new_v4()),
        dec!(500),
        &Currency::USD,
    );
    assert!(
        result.is_err(),
        "manually creating FundTrade should be rejected"
    );
}

#[test]
fn test_manually_creating_open_trade_transaction_is_rejected() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(1000),
            &Currency::USD,
        )
        .unwrap();

    let result = trust.create_transaction(
        &account,
        &TransactionCategory::OpenTrade(Uuid::new_v4()),
        dec!(500),
        &Currency::USD,
    );
    assert!(
        result.is_err(),
        "manually creating OpenTrade should be rejected"
    );
}

#[test]
fn test_manually_creating_close_target_transaction_is_rejected() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    let result = trust.create_transaction(
        &account,
        &TransactionCategory::CloseTarget(Uuid::new_v4()),
        dec!(500),
        &Currency::USD,
    );
    assert!(
        result.is_err(),
        "manually creating CloseTarget should be rejected"
    );
}

// ===========================================================================
// 7. WITHDRAWAL TAX / EARNINGS — Balance field correctness
// ===========================================================================

#[test]
fn test_withdrawal_tax_reduces_total_balance_but_not_total_available() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(5000),
            &Currency::USD,
        )
        .unwrap();

    let (tx, balance) = trust
        .create_transaction(
            &account,
            &TransactionCategory::WithdrawalTax,
            dec!(500),
            &Currency::USD,
        )
        .unwrap();

    assert_eq!(tx.category, TransactionCategory::WithdrawalTax);
    assert_eq!(tx.amount, dec!(500));
    // WithdrawalTax: decreases total_balance, does NOT decrease total_available
    assert_eq!(balance.total_balance, dec!(4500));
    assert_eq!(balance.total_available, dec!(5000)); // Unchanged
}

#[test]
fn test_withdrawal_earnings_reduces_both_total_balance_and_available() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(5000),
            &Currency::USD,
        )
        .unwrap();

    let (_, balance) = trust
        .create_transaction(
            &account,
            &TransactionCategory::WithdrawalEarnings,
            dec!(500),
            &Currency::USD,
        )
        .unwrap();

    // WithdrawalEarnings: decreases both total_balance AND total_available
    assert_eq!(balance.total_balance, dec!(4500));
    assert_eq!(balance.total_available, dec!(4500));
}

// ===========================================================================
// 8. TRADE BALANCE FIELD VERIFICATION — Full lifecycle
// ===========================================================================

#[test]
fn test_trade_balance_after_entry_fill_with_slippage() {
    let mut trust = new_facade(SyncBroker::new(|t| entry_filled_at(t, dec!(39.9))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let filled_trade = trust
        .search_trades(account.id, Status::Filled)
        .unwrap()
        .pop()
        .unwrap();

    // Trade balance after fill:
    // FundTrade: funding=20000, capital_out_market=20000
    // OpenTrade: capital_in_market=19950, capital_out_market=20000-19950=50
    // PaymentFromTrade (slippage refund): capital_out_market=50-50=0
    assert_eq!(filled_trade.balance.funding, dec!(20000));
    assert_eq!(filled_trade.balance.capital_in_market, dec!(19950));
    assert_eq!(filled_trade.balance.capital_out_market, dec!(0)); // Slippage refund zeroed it
    assert_eq!(filled_trade.balance.taxed, dec!(0));
    assert_eq!(filled_trade.balance.total_performance, dec!(-19950));
}

#[test]
fn test_trade_balance_after_exact_entry_fill() {
    let mut trust = new_facade(SyncBroker::new(|t| entry_filled_at(t, dec!(40))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let filled_trade = trust
        .search_trades(account.id, Status::Filled)
        .unwrap()
        .pop()
        .unwrap();

    // No slippage refund: capital_out_market = funding - open = 20000 - 20000 = 0
    assert_eq!(filled_trade.balance.funding, dec!(20000));
    assert_eq!(filled_trade.balance.capital_in_market, dec!(20000));
    assert_eq!(filled_trade.balance.capital_out_market, dec!(0));
    assert_eq!(filled_trade.balance.total_performance, dec!(-20000));
}

#[test]
fn test_trade_balance_after_profitable_close() {
    let mut trust = new_facade(SyncBroker::new(|t| {
        target_filled_at(t, dec!(39.9), dec!(52.9))
    }));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let closed_trade = trust
        .search_trades(account.id, Status::ClosedTarget)
        .unwrap()
        .pop()
        .unwrap();

    assert_eq!(closed_trade.balance.capital_in_market, dec!(0));
    assert_eq!(closed_trade.balance.total_performance, dec!(6500));
    assert_eq!(closed_trade.balance.capital_out_market, dec!(0));
}

#[test]
fn test_trade_balance_after_stop_loss_close() {
    let mut trust = new_facade(SyncBroker::new(|t| stop_filled_at(t, dec!(39.9), dec!(38))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let closed_trade = trust
        .search_trades(account.id, Status::ClosedStopLoss)
        .unwrap()
        .pop()
        .unwrap();

    assert_eq!(closed_trade.balance.capital_in_market, dec!(0));
    // Performance = CloseSafetyStop(19000) - OpenTrade(19950) = -950
    assert_eq!(closed_trade.balance.total_performance, dec!(-950));
    assert_eq!(closed_trade.balance.capital_out_market, dec!(0));
}

// ===========================================================================
// 9. MULTIPLE TRADES — Verify balance isolation
// ===========================================================================

#[test]
fn test_two_trades_funded_simultaneously_correct_balances() {
    let mut trust = new_facade(NoOpBroker);
    let account = setup_account(&mut trust, dec!(100000));

    // Create and fund two trades
    for symbol in &["AAPL", "GOOG"] {
        let tv = create_vehicle(&mut trust, symbol);
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
    }

    let new_trades = trust.search_trades(account.id, Status::New).unwrap();
    assert_eq!(new_trades.len(), 2);

    // Fund first trade
    trust.fund_trade(&new_trades[0]).unwrap();
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(80000));
    assert_eq!(balance.total_in_trade, dec!(20000));

    // Fund second trade
    trust.fund_trade(&new_trades[1]).unwrap();
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(60000));
    assert_eq!(balance.total_in_trade, dec!(40000));

    // Cancel first trade — should restore only its funds
    let funded = trust.search_trades(account.id, Status::Funded).unwrap();
    trust.cancel_funded_trade(&funded[0]).unwrap();
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(80000));
    assert_eq!(balance.total_in_trade, dec!(20000));
}

// ===========================================================================
// 10. DECIMAL PRECISION
// ===========================================================================

#[test]
fn test_deposit_and_withdrawal_with_fractional_cents() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(0.0001),
            &Currency::USD,
        )
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(0.0002),
            &Currency::USD,
        )
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(0.0003),
            &Currency::USD,
        )
        .unwrap();

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(0.0006));
    assert_eq!(balance.total_available, dec!(0.0006));

    let (_, balance) = trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(0.0006),
            &Currency::USD,
        )
        .unwrap();
    assert_eq!(balance.total_balance, dec!(0));
    assert_eq!(balance.total_available, dec!(0));
}

#[test]
fn test_large_deposit_amount() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    let large_amount = dec!(999_999_999.9999);
    let (tx, balance) = trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            large_amount,
            &Currency::USD,
        )
        .unwrap();

    assert_eq!(tx.amount, large_amount);
    assert_eq!(balance.total_balance, large_amount);
    assert_eq!(balance.total_available, large_amount);
}

// ===========================================================================
// 11. FIRST DEPOSIT AUTO-CREATES BALANCE
// ===========================================================================

#[test]
fn test_first_deposit_auto_creates_balance_overview() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    let (tx, balance) = trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(500),
            &Currency::USD,
        )
        .unwrap();

    assert_eq!(tx.amount, dec!(500));
    assert_eq!(balance.total_balance, dec!(500));
    assert_eq!(balance.total_available, dec!(500));

    let queried = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(queried.total_balance, dec!(500));
}

// ===========================================================================
// 12. SHORT TRADE — Funding uses stop price (worst case)
// ===========================================================================

#[test]
fn test_short_trade_funded_at_stop_price() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(10000),
            &Currency::USD,
        )
        .unwrap();
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
        quantity: 100,
        currency: Currency::USD,
        category: TradeCategory::Short,
        thesis: None,
        sector: None,
        asset_class: None,
        context: None,
    };

    // Short trade: entry=50, stop=60 (worst case), target=40
    trust
        .create_trade(draft, dec!(60), dec!(50), dec!(40))
        .unwrap();
    let trade = trust
        .search_trades(account.id, Status::New)
        .unwrap()
        .pop()
        .unwrap();

    let (_, tx, _, _) = trust.fund_trade(&trade).unwrap();

    // For short trades, funding should use stop price: 60 * 100 = 6000
    assert_eq!(
        tx.amount,
        dec!(6000),
        "short trade should fund based on stop (worst case) price"
    );

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(4000)); // 10000 - 6000
}

// ===========================================================================
// 13. TRANSACTION CURRENCY CONSISTENCY
// ===========================================================================

#[test]
fn test_all_queryable_transactions_have_same_currency_as_trade() {
    let mut trust = new_facade(SyncBroker::new(|t| target_filled_at(t, dec!(40), dec!(50))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let txs = trust.get_account_transactions(account.id).unwrap();
    for tx in &txs {
        assert_eq!(
            tx.currency,
            Currency::USD,
            "transaction {:?} has wrong currency",
            tx.category
        );
    }
}

// ===========================================================================
// 14. IDEMPOTENCY — Multiple syncs should not create duplicate transactions
// ===========================================================================

#[test]
fn test_sync_idempotency_no_duplicate_transactions_on_fill() {
    let mut trust = new_facade(SyncBroker::new(|t| entry_filled_at(t, dec!(39.9))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    for _ in 0..5 {
        trust.sync_trade(&trade, &account).unwrap();
    }

    let txs = trust.get_account_transactions(account.id).unwrap();
    let refund_count = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::PaymentFromTrade(_)))
        .count();
    assert_eq!(
        refund_count, 1,
        "multiple syncs should not duplicate slippage refund"
    );

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30050));
}

#[test]
fn test_sync_idempotency_no_duplicate_transactions_on_close() {
    let mut trust = new_facade(SyncBroker::new(|t| {
        target_filled_at(t, dec!(39.9), dec!(52.9))
    }));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    for _ in 0..5 {
        trust.sync_trade(&trade, &account).unwrap();
    }

    let txs = trust.get_account_transactions(account.id).unwrap();
    let payment_count = txs
        .iter()
        .filter(|t| matches!(t.category, TransactionCategory::PaymentFromTrade(_)))
        .count();
    // 2 PaymentFromTrade: slippage refund + final return (not duplicated)
    assert_eq!(
        payment_count, 2,
        "multiple syncs should not duplicate PaymentFromTrade"
    );

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(56500));
    assert_eq!(balance.total_balance, dec!(56500));
}

// ===========================================================================
// 15. BALANCE ACCOUNTING IDENTITY
// ===========================================================================

#[test]
fn test_accounting_identity_after_deposit() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    for amount in [dec!(100), dec!(250.50), dec!(0.01)] {
        trust
            .create_transaction(
                &account,
                &TransactionCategory::Deposit,
                amount,
                &Currency::USD,
            )
            .unwrap();
    }

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, balance.total_available);
    assert_eq!(balance.total_in_trade, dec!(0));
}

#[test]
fn test_accounting_identity_after_funding_trade() {
    let mut trust = new_facade(NoOpBroker);
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

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(50000));
    assert_eq!(
        balance.total_available + balance.total_in_trade,
        balance.total_balance,
        "accounting identity: total_available + total_in_trade = total_balance"
    );
}

// ===========================================================================
// 16. SEVERE SLIPPAGE STOP LOSS
// ===========================================================================

#[test]
fn test_severe_slippage_stop_loss() {
    // Stop planned at 38, but filled at 30.2 — severe slippage for long
    let mut trust = new_facade(SyncBroker::new(|t| stop_filled_at(t, dec!(39.9), dec!(30.2))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let closed_trade = trust
        .search_trades(account.id, Status::ClosedStopLoss)
        .unwrap()
        .pop()
        .unwrap();

    // Performance = stop(30.2*500=15100) - open(39.9*500=19950) = -4850
    assert_eq!(closed_trade.balance.total_performance, dec!(-4850));

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    // 50000 - open(19950) + slippage_refund(50) + stop_return(15100) = 45200
    // But total_balance = initial - open + close = 50000 - 19950 + 15100 = 45150
    assert_eq!(balance.total_balance, dec!(45150));
    assert_eq!(balance.total_in_trade, dec!(0));
}

// ===========================================================================
// 17. PROFITABLE/LOSING TRADE ENDING BALANCE VERIFICATION
// ===========================================================================

#[test]
fn test_profitable_trade_final_balance_matches_deposit_plus_profit() {
    // Entry at 40, target at 60 → profit = (60-40)*500 = 10000
    let mut trust = new_facade(SyncBroker::new(|t| target_filled_at(t, dec!(40), dec!(60))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(60000)); // 50000 + 10000
    assert_eq!(balance.total_available, dec!(60000));
    assert_eq!(balance.total_in_trade, dec!(0));
}

#[test]
fn test_losing_trade_final_balance_matches_deposit_minus_loss() {
    // Entry at 40, stop at 37 → loss = (40-37)*500 = 1500
    let mut trust = new_facade(SyncBroker::new(|t| stop_filled_at(t, dec!(40), dec!(37))));
    let account = setup_account(&mut trust, dec!(50000));
    let trade = setup_submitted_trade(&mut trust, &account);

    trust.sync_trade(&trade, &account).unwrap();

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(48500)); // 50000 - 1500
    assert_eq!(balance.total_available, dec!(48500));
    assert_eq!(balance.total_in_trade, dec!(0));
}

// ===========================================================================
// 18. WITHDRAW WHILE TRADE IS FUNDED — Available balance should be reduced
// ===========================================================================

#[test]
fn test_cannot_withdraw_funds_locked_in_trade() {
    let mut trust = new_facade(NoOpBroker);
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

    // Available is now 30000 (50000 - 20000 in trade)
    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_available, dec!(30000));

    // Should succeed: withdraw up to available
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Withdrawal,
            dec!(30000),
            &Currency::USD,
        )
        .unwrap();

    // Should fail: nothing left to withdraw
    let result = trust.create_transaction(
        &account,
        &TransactionCategory::Withdrawal,
        dec!(1),
        &Currency::USD,
    );
    assert!(
        result.is_err(),
        "should not be able to withdraw funds locked in trade"
    );
}

// ===========================================================================
// 19. MULTIPLE DEPOSITS/WITHDRAWALS RUNNING BALANCE
// ===========================================================================

#[test]
fn test_running_balance_after_many_operations() {
    let mut trust = new_facade(NoOpBroker);
    let account = trust
        .create_account("acc", "d", model::Environment::Paper, dec!(20), dec!(10))
        .unwrap();

    let ops: Vec<(TransactionCategory, Decimal)> = vec![
        (TransactionCategory::Deposit, dec!(10000)),
        (TransactionCategory::Withdrawal, dec!(500)),
        (TransactionCategory::Deposit, dec!(250.75)),
        (TransactionCategory::Withdrawal, dec!(1000.25)),
        (TransactionCategory::Deposit, dec!(3000)),
        (TransactionCategory::Withdrawal, dec!(7500)),
    ];

    // Expected: 10000 - 500 + 250.75 - 1000.25 + 3000 - 7500 = 4250.50
    for (cat, amount) in &ops {
        trust
            .create_transaction(&account, cat, *amount, &Currency::USD)
            .unwrap();
    }

    let balance = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(balance.total_balance, dec!(4250.50));
    assert_eq!(balance.total_available, dec!(4250.50));

    // Verify transaction count
    let txs = trust.get_account_transactions(account.id).unwrap();
    assert_eq!(txs.len(), 6);
}
