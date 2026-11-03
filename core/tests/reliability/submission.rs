//! Submission must authorize current loss budgets while preserving its reservation.
use super::*;

#[test]
fn realized_loss_after_funding_cannot_be_hidden_by_its_reservation() {
    use core::validation_errors::{FundValidationError, FundValidationErrorCode};
    use std::sync::atomic::Ordering;

    for side in [TradeCategory::Long, TradeCategory::Short] {
        for (loss, may_submit) in [(dec!(300), true), (dec!(495), false)] {
            let mut f = Fixture::new();
            f.rule(RuleName::RiskPerTrade(2.0));
            f.rule(RuleName::RiskPerMonth(5.0));
            let draft = f.trade(side, dec!(2));
            let pending = f.facade.fund_trade(&draft).unwrap().0;
            let reserved = if side == TradeCategory::Long {
                dec!(200)
            } else {
                dec!(220)
            };
            assert_eq!(pending.balance.funding, reserved);

            // Another real lifecycle suffers an adverse exit after this trade was funded.
            // Its loss must consume the original $500 monthly budget at submission.
            let draft = f.trade(side, dec!(5));
            let mut losing = f.facade.fund_trade(&draft).unwrap().0;
            losing.entry.average_filled_price = Some(dec!(100));
            let mut losing = f.facade.fill_trade(&losing, Decimal::ZERO).unwrap().0;
            let exit = match side {
                TradeCategory::Long => dec!(100) - loss / dec!(5),
                TradeCategory::Short => dec!(100) + loss / dec!(5),
            };
            losing.safety_stop.average_filled_price = Some(exit);
            f.facade.stop_trade(&losing, Decimal::ZERO).unwrap();
            let closed = f.facade.read_trade(losing.id).unwrap();
            assert_eq!(closed.status, Status::ClosedStopLoss);
            assert_eq!(closed.balance.total_performance, -loss);
            let equity = dec!(10000) - loss;
            let balance =
                f.db.account_balance_read()
                    .for_currency(f.account.id, &Currency::USD)
                    .unwrap();
            assert_eq!(balance.total_available, equity - reserved);
            assert!(
                balance.total_available > reserved,
                "cash must not explain rejection"
            );
            let level = f.db.level_read().level_for_account(f.account.id).unwrap();
            assert!(
                equity * dec!(0.02) / dec!(10) * level.risk_multiplier >= dec!(2),
                "the current level must independently permit this quantity"
            );
            assert_eq!(f.broker_calls.load(Ordering::SeqCst), 0);
            let before = f.tables();

            // A $300 loss leaves $200; a $495 loss leaves only $5 for a $20 bracket.
            // Re-adding its $200/$220 reservation twice must never hide that loss.
            if may_submit {
                let submitted = f.facade.submit_trade(&pending).unwrap().0;
                assert_eq!(submitted.status, Status::Submitted);
                assert_eq!(submitted.balance.funding, reserved);
                assert_eq!(submitted.balance.capital_out_market, reserved);
                assert_eq!(f.broker_calls.load(Ordering::SeqCst), 1);
                let after = f.tables();
                assert_eq!(after["transactions"], before["transactions"]);
                let balance =
                    f.db.account_balance_read()
                        .for_currency(f.account.id, &Currency::USD)
                        .unwrap();
                assert_eq!(balance.total_available, equity - reserved);
                assert_eq!(balance.total_balance, equity);
            } else {
                let error = f.facade.submit_trade(&pending).unwrap_err();
                assert_eq!(
                    error.downcast_ref::<FundValidationError>().unwrap().code,
                    FundValidationErrorCode::RiskPerMonthExceeded,
                    "{side}: {error}"
                );
                assert_eq!(
                    f.facade.read_trade(pending.id).unwrap().status,
                    Status::Funded
                );
                assert_eq!(f.broker_calls.load(Ordering::SeqCst), 0);
                assert_eq!(f.tables(), before);
            }
        }
    }
}

#[test]
fn submission_revalidates_changed_trade_and_monthly_caps_without_broker_effects() {
    for side in [TradeCategory::Long, TradeCategory::Short] {
        for replacement in [
            RuleName::RiskPerTrade(0.0),
            RuleName::RiskPerTrade(0.5),
            RuleName::RiskPerMonth(0.0),
            RuleName::RiskPerMonth(0.5),
        ] {
            let mut f = Fixture::new();
            f.rule(RuleName::RiskPerTrade(2.0));
            f.rule(RuleName::RiskPerMonth(5.0));
            let trade = f.trade(side, dec!(10));
            let funded = f.facade.fund_trade(&trade).unwrap().0;
            let old = f
                .facade
                .search_rules(f.account.id)
                .unwrap()
                .into_iter()
                .find(|rule| {
                    std::mem::discriminant(&rule.name) == std::mem::discriminant(&replacement)
                })
                .unwrap();
            f.facade.deactivate_rule(&old).unwrap();
            f.rule(replacement);
            let before = f.tables();
            let error = f.facade.submit_trade(&funded).unwrap_err();
            assert!(
                error
                    .downcast_ref::<core::validation_errors::FundValidationError>()
                    .is_some(),
                "{error}"
            );
            assert_eq!(f.broker_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
            assert_eq!(f.tables(), before);
            assert_eq!(
                f.facade.read_trade(funded.id).unwrap().status,
                Status::Funded
            );
        }
    }
}

#[test]
fn submission_reuses_only_its_own_reservation_and_preserves_monthly_boundary() {
    for side in [TradeCategory::Long, TradeCategory::Short] {
        for quantity in [dec!(1), dec!(10), dec!(50)] {
            let mut f = Fixture::new();
            f.rule(RuleName::RiskPerTrade(5.0));
            f.rule(RuleName::RiskPerMonth(5.0));
            let trade = f.trade(side, quantity);
            let funded = f.facade.fund_trade(&trade).unwrap().0;
            let before = f.facade.get_all_transactions().unwrap().len();
            assert_eq!(
                f.facade.submit_trade(&funded).unwrap().0.status,
                Status::Submitted
            );
            assert_eq!(f.facade.get_all_transactions().unwrap().len(), before);
            assert_eq!(f.broker_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        }
    }
    let mut f = Fixture::new();
    f.rule(RuleName::RiskPerTrade(100.0));
    f.rule(RuleName::RiskPerMonth(100.0));
    let trade = f.trade(TradeCategory::Long, dec!(100));
    let funded = f.facade.fund_trade(&trade).unwrap().0;
    f.facade.submit_trade(&funded).unwrap();
    assert_eq!(f.broker_calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn submission_fails_closed_when_reservation_or_rule_reads_are_corrupt() {
    for sql in [
        "UPDATE trades_balances SET capital_out_market='0'",
        "DROP TABLE rules",
    ] {
        let mut f = Fixture::new();
        f.rule(RuleName::RiskPerTrade(2.0));
        let trade = f.trade(TradeCategory::Long, dec!(10));
        let funded = f.facade.fund_trade(&trade).unwrap().0;
        diesel::sql_query(sql).execute(&mut f.connection).unwrap();
        assert!(f.facade.submit_trade(&funded).is_err());
        assert_eq!(f.broker_calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}
