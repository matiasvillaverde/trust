use super::*;

#[test]
fn funding_applies_the_level_cap_to_long_and_short_after_resolving_risk() {
    for side in [TradeCategory::Long, TradeCategory::Short] {
        let mut database = SqliteDatabase::new_in_memory();
        let account = create_test_account(&database, "level-cap");
        create_test_balance(&database, &account, dec!(10_000));
        create_test_deposit(&database, &account, dec!(10_000));
        create_test_rule(&database, &account, RuleName::RiskPerTrade(2.0), 1);
        create_test_rule(&database, &account, RuleName::RiskPerMonth(20.0), 2);
        let mut level = database
            .level_write()
            .create_default_level(&account)
            .unwrap();
        level.risk_multiplier = dec!(0.5);
        database.level_write().update_level(&level).unwrap();
        let stop = if side == TradeCategory::Long {
            dec!(90)
        } else {
            dec!(110)
        };
        let mut trade = long_trade(account.id, 11, dec!(100), stop);
        trade.category = side;
        if side == TradeCategory::Short {
            trade.target.unit_price = dec!(90);
        }
        let before = database
            .transaction_read()
            .all_transactions(account.id, &Currency::USD)
            .unwrap();
        let error = can_fund(&trade, &mut database).unwrap_err();
        assert_eq!(
            error.code,
            FundValidationErrorCode::LevelAdjustedQuantityExceeded
        );
        for order in [&mut trade.entry, &mut trade.safety_stop, &mut trade.target] {
            order.quantity = dec!(10);
        }
        can_fund(&trade, &mut database).unwrap();
        assert_eq!(
            database
                .transaction_read()
                .all_transactions(account.id, &Currency::USD)
                .unwrap(),
            before
        );
    }
}

#[test]
fn funding_still_verifies_the_ledger_when_the_cash_projection_looks_sufficient() {
    let mut database = SqliteDatabase::new_in_memory();
    let account = create_test_account(&database, "ledger-mismatch");
    create_test_balance(&database, &account, dec!(1000));
    create_test_deposit(&database, &account, dec!(1000));
    create_default_level(&database, &account);
    database
        .transaction_write()
        .create_transaction(
            &account,
            dec!(1001),
            &Currency::USD,
            TransactionCategory::Withdrawal,
        )
        .unwrap();
    let trade = long_trade(account.id, 1, dec!(100), dec!(90));
    let before = database
        .transaction_read()
        .all_transactions(account.id, &Currency::USD)
        .unwrap();
    let error = can_fund(&trade, &mut database).unwrap_err();
    assert_eq!(error.code, FundValidationErrorCode::NotEnoughFunds);
    assert!(error.message.contains("total available is negative"));
    assert_eq!(
        database
            .transaction_read()
            .all_transactions(account.id, &Currency::USD)
            .unwrap(),
        before
    );
}
