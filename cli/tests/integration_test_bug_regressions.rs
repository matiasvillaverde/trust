use core::TrustFacade;
use db_sqlite::SqliteDatabase;
use model::{
    Account, Broker, BrokerLog, Currency, DraftTrade, Environment, Order, OrderIds, Status, Trade,
    TradeCategory, TradingVehicleCategory, TransactionCategory,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::error::Error;
use uuid::Uuid;

struct NoOpBroker;

impl Broker for NoOpBroker {
    fn submit_trade(
        &self,
        _trade: &Trade,
        _account: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        Ok((
            BrokerLog::default(),
            OrderIds {
                entry: Uuid::new_v4(),
                target: Uuid::new_v4(),
                stop: Uuid::new_v4(),
            },
        ))
    }

    fn sync_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        let entry = Order {
            id: trade.entry.id,
            broker_order_id: trade.entry.broker_order_id,
            filled_quantity: trade.entry.quantity,
            average_filled_price: Some(trade.entry.unit_price),
            status: model::OrderStatus::Filled,
            filled_at: Some(chrono::Utc::now().naive_utc()),
            ..Default::default()
        };
        let target = Order {
            id: trade.target.id,
            broker_order_id: trade.target.broker_order_id,
            status: model::OrderStatus::Held,
            ..Default::default()
        };
        let stop = Order {
            id: trade.safety_stop.id,
            broker_order_id: trade.safety_stop.broker_order_id,
            status: model::OrderStatus::Held,
            ..Default::default()
        };
        Ok((
            Status::Filled,
            vec![entry, target, stop],
            BrokerLog::default(),
        ))
    }

    fn close_trade(
        &self,
        trade: &Trade,
        _account: &Account,
    ) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        let mut closed_target = trade.target.clone();
        closed_target.status = model::OrderStatus::Filled;
        closed_target.filled_quantity = trade.target.quantity;
        closed_target.average_filled_price = Some(trade.entry.unit_price);
        Ok((closed_target, BrokerLog::default()))
    }

    fn cancel_trade(&self, _trade: &Trade, _account: &Account) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    fn modify_stop(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_stop_price: Decimal,
    ) -> Result<Uuid, Box<dyn Error>> {
        Ok(Uuid::new_v4())
    }

    fn modify_target(
        &self,
        _trade: &Trade,
        _account: &Account,
        _new_target_price: Decimal,
    ) -> Result<Uuid, Box<dyn Error>> {
        Ok(Uuid::new_v4())
    }
}

fn create_trust() -> TrustFacade {
    let db = SqliteDatabase::new_in_memory();
    TrustFacade::new(Box::new(db), Box::new(NoOpBroker))
}

fn setup_account_with_deposit(trust: &mut TrustFacade) -> Account {
    trust
        .create_account(
            "bug-reg",
            "bug regression tests",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account creation should succeed");

    let account = trust
        .search_account("bug-reg")
        .expect("account should be searchable");

    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(50000),
            &Currency::USD,
        )
        .expect("deposit should succeed");

    account
}

fn create_new_long_trade(trust: &mut TrustFacade, account: &Account) -> Trade {
    let trading_vehicle = trust
        .create_trading_vehicle(
            "AAPL",
            Some("US0378331005"),
            &TradingVehicleCategory::Stock,
            "NASDAQ",
        )
        .expect("trading vehicle creation should succeed");

    let draft = DraftTrade {
        account: account.clone(),
        trading_vehicle,
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
        .expect("trade creation should succeed");

    trust
        .search_trades(account.id, Status::New)
        .expect("new trades should be queryable")
        .first()
        .expect("one new trade should exist")
        .clone()
}

#[test]
fn funding_should_reject_stale_new_snapshot_after_trade_is_already_funded() {
    let mut trust = create_trust();
    let account = setup_account_with_deposit(&mut trust);
    let new_trade_snapshot = create_new_long_trade(&mut trust, &account);

    trust
        .fund_trade(&new_trade_snapshot)
        .expect("first funding should succeed");

    let second_funding = trust.fund_trade(&new_trade_snapshot);
    assert!(
        second_funding.is_err(),
        "funding via stale New snapshot must fail to prevent double-funding"
    );
}

#[test]
fn funding_should_reject_stale_new_snapshot_after_trade_is_submitted() {
    let mut trust = create_trust();
    let account = setup_account_with_deposit(&mut trust);
    let new_trade_snapshot = create_new_long_trade(&mut trust, &account);

    trust
        .fund_trade(&new_trade_snapshot)
        .expect("first funding should succeed");

    let funded_trade = trust
        .search_trades(account.id, Status::Funded)
        .expect("funded trades should be queryable")
        .first()
        .expect("one funded trade should exist")
        .clone();

    trust
        .submit_trade(&funded_trade)
        .expect("submit should succeed");

    let funding_after_submit = trust.fund_trade(&new_trade_snapshot);
    assert!(
        funding_after_submit.is_err(),
        "funding via stale New snapshot must fail once trade is already submitted"
    );
}

#[test]
fn cancel_submitted_on_non_submitted_trade_should_report_submitted_state() {
    let mut trust = create_trust();
    let account = setup_account_with_deposit(&mut trust);
    let new_trade_snapshot = create_new_long_trade(&mut trust, &account);

    let cancel_result = trust.cancel_submitted_trade(&new_trade_snapshot);
    let error_message = format!(
        "{}",
        cancel_result.expect_err("cancel_submitted on New trade should fail")
    )
    .to_lowercase();

    assert!(
        error_message.contains("not submitted"),
        "error should mention submitted-state precondition, got: {error_message}"
    );
}

#[test]
fn submit_should_reject_stale_funded_snapshot_after_trade_is_already_submitted() {
    let mut trust = create_trust();
    let account = setup_account_with_deposit(&mut trust);
    let new_trade_snapshot = create_new_long_trade(&mut trust, &account);

    trust
        .fund_trade(&new_trade_snapshot)
        .expect("first funding should succeed");

    let funded_snapshot = trust
        .search_trades(account.id, Status::Funded)
        .expect("funded trades should be queryable")
        .first()
        .expect("one funded trade should exist")
        .clone();

    trust
        .submit_trade(&funded_snapshot)
        .expect("first submit should succeed");

    let second_submit = trust.submit_trade(&funded_snapshot);
    assert!(
        second_submit.is_err(),
        "submitting with stale Funded snapshot must fail once trade is already submitted"
    );
}

#[test]
fn cancel_funded_should_reject_stale_funded_snapshot_after_trade_is_already_canceled() {
    let mut trust = create_trust();
    let account = setup_account_with_deposit(&mut trust);
    let new_trade_snapshot = create_new_long_trade(&mut trust, &account);

    trust
        .fund_trade(&new_trade_snapshot)
        .expect("first funding should succeed");

    let funded_snapshot = trust
        .search_trades(account.id, Status::Funded)
        .expect("funded trades should be queryable")
        .first()
        .expect("one funded trade should exist")
        .clone();

    trust
        .cancel_funded_trade(&funded_snapshot)
        .expect("first funded cancel should succeed");

    let second_cancel = trust.cancel_funded_trade(&funded_snapshot);
    assert!(
        second_cancel.is_err(),
        "cancel_funded with stale Funded snapshot must fail once trade is already canceled"
    );
}

#[test]
fn cancel_submitted_should_reject_stale_submitted_snapshot_after_trade_is_already_canceled() {
    let mut trust = create_trust();
    let account = setup_account_with_deposit(&mut trust);
    let new_trade_snapshot = create_new_long_trade(&mut trust, &account);

    trust
        .fund_trade(&new_trade_snapshot)
        .expect("funding should succeed");

    let funded_snapshot = trust
        .search_trades(account.id, Status::Funded)
        .expect("funded trades should be queryable")
        .first()
        .expect("one funded trade should exist")
        .clone();

    trust
        .submit_trade(&funded_snapshot)
        .expect("submit should succeed");

    let submitted_snapshot = trust
        .search_trades(account.id, Status::Submitted)
        .expect("submitted trades should be queryable")
        .first()
        .expect("one submitted trade should exist")
        .clone();

    trust
        .cancel_submitted_trade(&submitted_snapshot)
        .expect("first submitted cancel should succeed");

    let second_cancel = trust.cancel_submitted_trade(&submitted_snapshot);
    assert!(
        second_cancel.is_err(),
        "cancel_submitted with stale Submitted snapshot must fail once trade is already canceled"
    );
}

#[test]
fn close_trade_should_reject_stale_filled_snapshot_after_trade_is_already_closed() {
    let mut trust = create_trust();
    let account = setup_account_with_deposit(&mut trust);
    let new_trade_snapshot = create_new_long_trade(&mut trust, &account);

    trust
        .fund_trade(&new_trade_snapshot)
        .expect("funding should succeed");

    let funded_snapshot = trust
        .search_trades(account.id, Status::Funded)
        .expect("funded trades should be queryable")
        .first()
        .expect("one funded trade should exist")
        .clone();

    trust
        .submit_trade(&funded_snapshot)
        .expect("submit should succeed");

    let submitted_snapshot = trust
        .search_trades(account.id, Status::Submitted)
        .expect("submitted trades should be queryable")
        .first()
        .expect("one submitted trade should exist")
        .clone();

    trust
        .sync_trade(&submitted_snapshot, &account)
        .expect("sync should fill entry");

    let filled_snapshot = trust
        .search_trades(account.id, Status::Filled)
        .expect("filled trades should be queryable")
        .first()
        .expect("one filled trade should exist")
        .clone();

    trust
        .close_trade(&filled_snapshot)
        .expect("first close should succeed");

    let second_close = trust.close_trade(&filled_snapshot);
    assert!(
        second_close.is_err(),
        "closing with stale Filled snapshot must fail once trade is already closed"
    );
}
