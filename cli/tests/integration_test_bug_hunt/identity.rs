use super::*;

#[test]
fn duplicate_account_names_cannot_overwrite_identity_policy_or_cash() {
    let mut trust = create_trust();
    let account = trust
        .create_account("test", "first", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    trust
        .create_transaction(
            &account,
            &TransactionCategory::Deposit,
            dec!(123.45),
            &Currency::USD,
        )
        .unwrap();
    let before = trust.search_balance(account.id, &Currency::USD).unwrap();
    let receipts = trust.get_account_transactions(account.id).unwrap();
    for name in ["test", "TEST"] {
        assert!(trust
            .create_account(name, "replacement", Environment::Paper, dec!(30), dec!(15))
            .is_err());
        assert_eq!(trust.search_account("test").unwrap(), account);
        assert_eq!(
            trust.search_balance(account.id, &Currency::USD).unwrap(),
            before
        );
        assert_eq!(
            trust.get_account_transactions(account.id).unwrap(),
            receipts
        );
    }
    let other = trust
        .create_account("other", "distinct", Environment::Paper, dec!(20), dec!(10))
        .unwrap();
    assert_ne!(other.id, account.id);
    assert_eq!(trust.search_all_accounts().unwrap().len(), 2);
}

#[test]
fn instrument_identity_is_unique_within_broker_and_isolated_across_brokers() {
    let mut trust = create_trust();
    let first = trust
        .create_trading_vehicle(
            "AAPL",
            Some("US0378331005"),
            &TradingVehicleCategory::Stock,
            "alpaca",
        )
        .unwrap();
    assert!(trust
        .create_trading_vehicle(
            "aapl",
            Some("US0378331005"),
            &TradingVehicleCategory::Stock,
            "ALPACA"
        )
        .is_err());
    assert_eq!(
        trust.search_trading_vehicles().unwrap(),
        vec![first.clone()]
    );
    let other = trust
        .create_trading_vehicle(
            "AAPL",
            Some("US0378331005"),
            &TradingVehicleCategory::Stock,
            "ibkr",
        )
        .unwrap();
    assert_ne!(first.id, other.id);
    assert_eq!(other.broker, "ibkr");
    assert_eq!(trust.search_trading_vehicles().unwrap().len(), 2);
}
