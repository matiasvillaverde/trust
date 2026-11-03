use super::*;
use core::validation_errors::{FundValidationError, FundValidationErrorCode};
use funding_fixture::Fixture;

fn assert_risk_boundary(quantity: Decimal, permitted: bool) {
    for side in [TradeCategory::Long, TradeCategory::Short] {
        let mut f = Fixture::new();
        let (stop, target, funding_per_unit) = if side == TradeCategory::Long {
            (dec!(38), dec!(50), dec!(40))
        } else {
            (dec!(42), dec!(30), dec!(42))
        };
        // $50,000 cash, $1,000 per-trade loss budget, $10,000 monthly budget.
        // Every proposal costs at most $21,000; only the per-trade cap differs.
        let trade = f
            .trust
            .create_trade(f.draft(side, quantity), stop, dec!(40), target)
            .unwrap();
        let before = f.tables();
        let result = f.trust.fund_trade(&trade);
        if permitted {
            let (funded, tx, balance, trade_balance) = result.expect("quantity within loss budget");
            let required = quantity * funding_per_unit;
            assert_eq!(funded.status, Status::Funded);
            assert_eq!(tx.category, TransactionCategory::FundTrade(trade.id));
            assert_eq!(tx.amount, required);
            assert_eq!(balance.total_available, dec!(50000) - required);
            assert_eq!(balance.total_in_trade, required);
            assert_eq!(balance.total_balance, dec!(50000));
            assert_eq!(trade_balance.funding, required);
            assert_eq!(trade_balance.capital_out_market, required);
            assert_eq!(trade_balance.capital_in_market, Decimal::ZERO);
            assert_eq!(f.trust.read_trade(trade.id).unwrap(), funded);
            assert_eq!(
                f.trust
                    .search_balance(f.account.id, &Currency::USD)
                    .unwrap(),
                balance
            );
            let receipts = f.trust.get_account_transactions(f.account.id).unwrap();
            assert_eq!(receipts.len(), 2, "deposit plus exactly one reservation");
            assert_eq!(
                receipts
                    .iter()
                    .filter(|r| r.category == TransactionCategory::FundTrade(trade.id))
                    .collect::<Vec<_>>(),
                vec![&tx]
            );
        } else {
            let error = result.expect_err("loss exceeds $1,000 budget by $0.02");
            assert_eq!(
                error
                    .downcast_ref::<FundValidationError>()
                    .expect("funding validation error")
                    .code,
                FundValidationErrorCode::RiskPerTradeExceeded
            );
            assert_eq!(
                f.tables(),
                before,
                "risk rejection must preserve every financial row"
            );
            assert_eq!(f.trust.read_trade(trade.id).unwrap(), trade);
        }
        f.assert_no_broker_calls();
    }
}

#[test]
fn loss_budget_rejects_two_cents_over_limit_without_writes_or_broker_calls() {
    assert_risk_boundary(dec!(500.01), false);
}
#[test]
fn loss_budget_accepts_exact_limit_with_exact_reservation() {
    assert_risk_boundary(dec!(500), true);
}
#[test]
fn loss_budget_accepts_two_cents_below_limit_with_exact_reservation() {
    assert_risk_boundary(dec!(499.99), true);
}
