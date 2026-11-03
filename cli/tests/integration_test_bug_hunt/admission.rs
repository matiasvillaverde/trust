use super::*;
use core::validation_errors::{FundValidationError, FundValidationErrorCode};
use funding_fixture::Fixture;

fn assert_valid_neighbor(f: &mut Fixture, side: TradeCategory) {
    let (stop, target) = match side {
        TradeCategory::Long => (dec!(38), dec!(50)),
        TradeCategory::Short => (dec!(45), dec!(30)),
    };
    let trade = f
        .trust
        .create_trade(f.draft(side, dec!(100)), stop, dec!(40), target)
        .unwrap();
    let (funded, receipt, balance, _) = f.trust.fund_trade(&trade).unwrap();
    let required = if side == TradeCategory::Long {
        dec!(4000)
    } else {
        dec!(4500)
    };
    assert_eq!(funded.status, Status::Funded);
    assert_eq!(receipt.category, TransactionCategory::FundTrade(trade.id));
    assert_eq!(receipt.amount, required);
    assert_eq!(balance.total_available, dec!(50000) - required);
    assert_eq!(balance.total_in_trade, required);
    assert_eq!(f.trust.read_trade(trade.id).unwrap(), funded);
    f.assert_no_broker_calls();
}

#[test]
fn nonpositive_quantities_are_rejected_before_any_rows_are_written() {
    for quantity in [dec!(0), dec!(-100), Decimal::from(i64::MIN)] {
        for side in [TradeCategory::Long, TradeCategory::Short] {
            let mut f = Fixture::new();
            let before = f.tables();
            let error = f
                .trust
                .create_trade(f.draft(side, quantity), dec!(38), dec!(40), dec!(50))
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                format!("Trade quantity must be greater than zero, got {quantity}")
            );
            assert_eq!(
                f.tables(),
                before,
                "rejection must preserve all financial rows"
            );
            f.assert_no_broker_calls();
            assert_valid_neighbor(&mut f, side);
        }
    }
}

#[test]
fn a_negative_price_at_each_order_boundary_rolls_back_the_entire_draft() {
    // Later failures must also remove any previously inserted stop/entry orders.
    for (stop, entry, target) in [
        (dec!(-5), dec!(40), dec!(50)),
        (dec!(38), dec!(-3), dec!(50)),
        (dec!(38), dec!(40), dec!(-1)),
    ] {
        let mut f = Fixture::new();
        let before = f.tables();
        let error = f
            .trust
            .create_trade(f.draft(TradeCategory::Long, dec!(100)), stop, entry, target)
            .unwrap_err();
        assert!(error.to_string().contains("unit_price"), "{error}");
        assert!(
            error.to_string().contains("Negative financial value"),
            "{error}"
        );
        assert_eq!(f.tables(), before, "no orphan orders, balances or trades");
        f.assert_no_broker_calls();
        assert_valid_neighbor(&mut f, TradeCategory::Long);
    }
}

#[test]
fn invalid_brackets_remain_editable_drafts_but_cannot_reserve_money() {
    use TradeCategory::{Long, Short};
    for (name, side, stop, entry, target) in [
        ("long stop above entry", Long, dec!(45), dec!(40), dec!(50)),
        (
            "long target below entry",
            Long,
            dec!(38),
            dec!(40),
            dec!(35),
        ),
        ("long no stop distance", Long, dec!(40), dec!(40), dec!(50)),
        ("long no reward", Long, dec!(38), dec!(40), dec!(40)),
        ("zero prices", Long, dec!(0), dec!(0), dec!(0)),
        ("zero stop", Long, dec!(0), dec!(40), dec!(50)),
        ("zero entry", Long, dec!(38), dec!(0), dec!(50)),
        ("zero target", Long, dec!(38), dec!(40), dec!(0)),
        ("all prices equal", Long, dec!(100), dec!(100), dec!(100)),
        (
            "short stop below entry",
            Short,
            dec!(45),
            dec!(50),
            dec!(40),
        ),
        (
            "short no stop distance",
            Short,
            dec!(50),
            dec!(50),
            dec!(40),
        ),
        (
            "short target above entry",
            Short,
            dec!(60),
            dec!(50),
            dec!(55),
        ),
        ("short no reward", Short, dec!(60), dec!(50), dec!(50)),
    ] {
        let mut f = Fixture::new();
        let before_create = f.tables();
        let trade = f
            .trust
            .create_trade(f.draft(side, dec!(100)), stop, entry, target)
            .expect(name);
        assert_eq!(trade.status, Status::New);
        let created = f.tables();
        for table in ["transactions", "accounts_balances"] {
            assert_eq!(
                created.get(table).expect(table),
                before_create.get(table).expect(table),
                "{name}: creating a draft cannot move money"
            );
        }
        assert_eq!(created["trades"].as_array().unwrap().len(), 1);
        assert_eq!(created["orders"].as_array().unwrap().len(), 3);
        let error = f.trust.fund_trade(&trade).unwrap_err();
        assert_eq!(
            error
                .downcast_ref::<FundValidationError>()
                .expect("typed funding rejection")
                .code,
            FundValidationErrorCode::InvalidPriceDifference,
            "{name}: {error}"
        );
        assert_eq!(
            f.tables(),
            created,
            "{name}: rejected funding must change no rows"
        );
        assert_eq!(f.trust.read_trade(trade.id).unwrap(), trade);
        f.assert_no_broker_calls();
        assert_valid_neighbor(&mut f, side);
    }
}

#[test]
fn huge_positive_quantity_is_a_draft_but_fails_the_capital_gate_without_writes() {
    let mut f = Fixture::new();
    let before_create = f
        .trust
        .search_balance(f.account.id, &Currency::USD)
        .unwrap();
    let trade = f
        .trust
        .create_trade(
            f.draft(TradeCategory::Long, Decimal::from(i64::MAX)),
            dec!(38),
            dec!(40),
            dec!(50),
        )
        .unwrap();
    assert_eq!(
        f.trust
            .search_balance(f.account.id, &Currency::USD)
            .unwrap(),
        before_create
    );
    let before_funding = f.tables();
    let error = f.trust.fund_trade(&trade).unwrap_err();
    assert_eq!(
        error.downcast_ref::<FundValidationError>().unwrap().code,
        FundValidationErrorCode::NotEnoughFunds
    );
    assert_eq!(f.tables(), before_funding);
    f.assert_no_broker_calls();
    assert_valid_neighbor(&mut f, TradeCategory::Long);
}
