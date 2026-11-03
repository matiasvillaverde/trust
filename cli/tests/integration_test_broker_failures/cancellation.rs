use super::*;
struct CancelFailsBroker(Arc<AtomicU32>);
impl Broker for CancelFailsBroker {
    fn kind(&self) -> model::BrokerKind {
        model::BrokerKind::Alpaca
    }
    fn submit_trade(
        &self,
        trade: &Trade,
        _: &Account,
    ) -> Result<(BrokerLog, OrderIds), Box<dyn Error>> {
        Ok((
            BrokerLog {
                trade_id: trade.id,
                ..BrokerLog::default()
            },
            OrderIds {
                entry: "entry".into(),
                stop: "stop".into(),
                target: "target".into(),
            },
        ))
    }
    fn cancel_trade(&self, _: &Trade, _: &Account) -> Result<(), Box<dyn Error>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err("Broker cancel failed: order in transition state".into())
    }
    fn sync_trade(
        &self,
        _: &Trade,
        _: &Account,
    ) -> Result<(Status, Vec<Order>, BrokerLog), Box<dyn Error>> {
        panic!("cancellation must not poll the broker")
    }
    fn close_trade(&self, _: &Trade, _: &Account) -> Result<(Order, BrokerLog), Box<dyn Error>> {
        panic!("cancellation must not submit an exit")
    }
    fn modify_stop(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        panic!("cancellation must not modify protection")
    }
    fn modify_target(&self, _: &Trade, _: &Account, _: Decimal) -> Result<String, Box<dyn Error>> {
        panic!("cancellation must not modify protection")
    }
}

#[test]
fn unsubmitted_cancellation_refunds_exactly_once_without_broker_io() {
    let calls = Arc::new(AtomicU32::new(0));
    let mut trust = new_facade(CancelFailsBroker(calls.clone()));
    let account = setup_account(&mut trust, dec!(50000));
    let vehicle = create_vehicle(&mut trust, "TSLA");
    let trade = trust
        .create_trade(
            DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity: dec!(500),
                currency: Currency::USD,
                category: TradeCategory::Long,
                thesis: None,
                sector: None,
                asset_class: None,
                context: None,
            },
            dec!(38),
            dec!(40),
            dec!(50),
        )
        .unwrap();
    let (funded, ..) = trust.fund_trade(&trade).unwrap();
    assert_eq!(
        trust
            .search_balance(account.id, &Currency::USD)
            .unwrap()
            .total_available,
        dec!(30000)
    );
    let (_, balance, refund) = trust.cancel_funded_trade(&funded).unwrap();
    assert_eq!(refund.amount, dec!(20000));
    assert_eq!(balance.total_balance, dec!(50000));
    assert_eq!(balance.total_available, dec!(50000));
    assert_eq!(balance.total_in_trade, dec!(0));
    assert_eq!(trust.read_trade(trade.id).unwrap().status, Status::Canceled);
    let receipts = trust.get_account_transactions(account.id).unwrap();
    assert!(trust.cancel_funded_trade(&funded).is_err());
    assert_eq!(
        trust.search_balance(account.id, &Currency::USD).unwrap(),
        balance
    );
    assert_eq!(
        trust.get_account_transactions(account.id).unwrap(),
        receipts
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn failed_submitted_cancellation_preserves_reserved_capital_and_records_uncertainty() {
    let calls = Arc::new(AtomicU32::new(0));
    let mut trust = new_facade(CancelFailsBroker(calls.clone()));
    let account = setup_account(&mut trust, dec!(50000));
    let submitted = create_submitted_trade(&mut trust, &account);
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    let receipts = trust.get_account_transactions(account.id).unwrap();
    let error = trust.cancel_submitted_trade(&submitted).unwrap_err();
    assert!(error.to_string().contains("order in transition state"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(trust.read_trade(submitted.id).unwrap(), submitted);
    let after = trust.search_balance(account.id, &Currency::USD).unwrap();
    assert_eq!(after, before);
    assert_eq!(after.total_available, dec!(30000));
    assert_eq!(submitted.balance.funding, dec!(20000));
    assert_eq!(submitted.balance.capital_out_market, dec!(20000));
    assert_eq!(
        trust.get_account_transactions(account.id).unwrap(),
        receipts
    );
    let events = trust.trade_audit_events(submitted.id).unwrap();
    let cancel: Vec<_> = events
        .iter()
        .filter(|event| event.event_type == "trade.cancel")
        .collect();
    assert_eq!(cancel.len(), 2);
    assert_eq!(cancel[0].outcome, "attempted");
    assert_eq!(cancel[1].outcome, "outcome_unknown");
    assert_eq!(cancel[0].operation_id, cancel[1].operation_id);
    assert_eq!(cancel[1].details["capital_released"], false);
    assert_eq!(
        trust.verify_trade_audit_chain(submitted.id).unwrap(),
        events.len()
    );
}
