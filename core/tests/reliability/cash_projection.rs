//! Saved cash must never let a failed write or an invalid receipt authorize risk.
use super::*;
use diesel::connection::SimpleConnection;
use std::sync::{atomic::Ordering, Arc, Mutex};

fn connected_fixture() -> (Fixture, Arc<Mutex<diesel::SqliteConnection>>) {
    let mut fixture = Fixture::new();
    let mut connection = diesel::SqliteConnection::establish(
        fixture
            .directory
            .path()
            .join("financial.db")
            .to_str()
            .unwrap(),
    )
    .unwrap();
    connection
        .batch_execute("PRAGMA temp_store=MEMORY; PRAGMA foreign_keys=ON;")
        .unwrap();
    let connection = Arc::new(Mutex::new(connection));
    fixture.facade = TrustFacade::new(
        Box::new(SqliteDatabase::new_from(connection.clone())),
        Box::new(BrokerStub(fixture.broker_calls.clone())),
    );
    fixture.rule(RuleName::RiskPerTrade(2.0));
    fixture.rule(RuleName::RiskPerMonth(5.0));
    (fixture, connection)
}

#[test]
fn a_projection_write_failure_rolls_back_funding_and_retry_reserves_once() {
    let (mut fixture, connection) = connected_fixture();
    let draft = fixture.trade(TradeCategory::Long, dec!(1));
    let database = SqliteDatabase::new_from(connection.clone());
    assert_eq!(
        database
            .transaction_read()
            .verified_cash_balance(fixture.account.id, &Currency::USD)
            .unwrap(),
        dec!(10000)
    );
    connection
        .lock()
        .unwrap()
        .batch_execute(
            "CREATE TEMP TRIGGER reject_cash_save BEFORE UPDATE ON trust_verified_cash
      WHEN NEW.total != OLD.total BEGIN SELECT RAISE(ABORT, 'injected cash save failure'); END",
        )
        .unwrap();
    let before = fixture.tables();
    let error = fixture.facade.fund_trade(&draft).unwrap_err();
    assert!(
        error.to_string().contains("injected cash save failure"),
        "{error}"
    );
    assert_eq!(
        fixture.tables(),
        before,
        "cash, orders, receipts and status must all roll back"
    );
    assert_eq!(fixture.broker_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        database
            .transaction_read()
            .verified_cash_balance(fixture.account.id, &Currency::USD)
            .unwrap(),
        dec!(10000)
    );
    connection
        .lock()
        .unwrap()
        .batch_execute("DROP TRIGGER temp.reject_cash_save")
        .unwrap();
    let funded = fixture.facade.fund_trade(&draft).unwrap().0;
    assert_eq!(funded.balance.funding, dec!(100));
    assert_eq!(
        database
            .transaction_read()
            .verified_cash_balance(fixture.account.id, &Currency::USD)
            .unwrap(),
        dec!(9900)
    );
    assert_eq!(
        database
            .transaction_read()
            .all_trade_funding_transactions(draft.id)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        fixture.facade.submit_trade(&funded).unwrap().0.status,
        Status::Submitted
    );
    assert_eq!(fixture.broker_calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_poisoned_cash_append_blocks_submission_without_broker_effects() {
    let (mut fixture, connection) = connected_fixture();
    let draft = fixture.trade(TradeCategory::Long, dec!(1));
    let funded = fixture.facade.fund_trade(&draft).unwrap().0;
    let invalid = uuid::Uuid::new_v4();
    connection.lock().unwrap().batch_execute(&format!("INSERT INTO transactions
      (id, created_at, updated_at, account_id, currency, category, amount)
      VALUES ('{invalid}', '9999-01-01 00:00:00', '9999-01-01 00:00:00', '{}', 'USD', 'deposit', 'invalid')", fixture.account.id)).unwrap();
    let before = fixture.tables();
    for _ in 0..2 {
        let error = fixture.facade.submit_trade(&funded).unwrap_err();
        let rejection = error
            .downcast_ref::<core::validation_errors::FundValidationError>()
            .unwrap();
        assert_eq!(
            rejection.code,
            core::validation_errors::FundValidationErrorCode::NotEnoughFunds
        );
        assert!(rejection.message.contains("Invalid decimal"), "{rejection}");
        assert_eq!(fixture.tables(), before);
        assert_eq!(fixture.broker_calls.load(Ordering::SeqCst), 0);
    }
    connection
        .lock()
        .unwrap()
        .batch_execute(&format!("DELETE FROM transactions WHERE id='{invalid}'"))
        .unwrap();
    assert_eq!(
        fixture.facade.submit_trade(&funded).unwrap().0.status,
        Status::Submitted
    );
    assert_eq!(fixture.broker_calls.load(Ordering::SeqCst), 1);
}
