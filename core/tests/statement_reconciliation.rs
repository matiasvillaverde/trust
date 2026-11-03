//! Independent arithmetic contracts. End-to-end persistence is covered by the
//! service's statement workflows; these isolate exact financial boundaries.
use chrono::{DateTime, Utc};
use core::statements::reconcile;
use model::statement::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

fn time(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}
fn fixture() -> (BrokerStatement, StatementLedger) {
    let account = Uuid::new_v4();
    let trade = Uuid::new_v4();
    let executions = vec![
        StatementExecution {
            execution_id: "buy-a".into(),
            instrument_id: "1234".into(),
            symbol: "TEST".into(),
            currency: "USD".into(),
            buy: true,
            quantity: dec!(4),
            price: dec!(100),
            occurred_at: time("2026-03-17T10:00:00Z"),
            commission: dec!(1),
        },
        StatementExecution {
            execution_id: "buy-b".into(),
            instrument_id: "1234".into(),
            symbol: "TEST".into(),
            currency: "USD".into(),
            buy: true,
            quantity: dec!(6),
            price: dec!(101),
            occurred_at: time("2026-03-17T11:00:00Z"),
            commission: dec!(1.5),
        },
        StatementExecution {
            execution_id: "sell-a".into(),
            instrument_id: "1234".into(),
            symbol: "TEST".into(),
            currency: "USD".into(),
            buy: false,
            quantity: dec!(3),
            price: dec!(110),
            occurred_at: time("2026-03-17T15:00:00Z"),
            commission: dec!(0.75),
        },
    ];
    let ledger = StatementLedger {
        trades: vec![RecordedTrade {
            account_id: account,
            trade_id: trade,
            entry_buy: true,
            entry_filled: dec!(10),
            exit_filled: dec!(3),
        }],
        executions: executions
            .iter()
            .cloned()
            .map(|execution| RecordedExecution {
                account_id: account,
                trade_id: trade,
                execution,
            })
            .collect(),
        fees: executions
            .iter()
            .map(|e| RecordedFee {
                account_id: account,
                trade_id: Some(trade),
                activity_id: format!("{}:commission", e.execution_id),
                amount: e.commission,
                occurred_at: e.occurred_at,
            })
            .collect(),
        movements: vec![
            RecordedMovement {
                account_id: account,
                trade_id: None,
                category: "deposit".into(),
                currency: "USD".into(),
                amount: dec!(10000),
                occurred_at: time("2026-03-16T00:00:00Z"),
            },
            // Booked late, but the broker's economic occurrence date is authoritative.
            RecordedMovement {
                account_id: account,
                trade_id: Some(trade),
                category: "fee_open".into(),
                currency: "USD".into(),
                amount: dec!(2.5),
                occurred_at: time("2026-03-19T00:00:00Z"),
            },
            RecordedMovement {
                account_id: account,
                trade_id: Some(trade),
                category: "fee_close".into(),
                currency: "USD".into(),
                amount: dec!(0.75),
                occurred_at: time("2026-03-19T00:00:00Z"),
            },
        ],
    };
    let report = BrokerStatement {
        broker_account_id: "DU_TEST".into(),
        from_date: time("2026-03-17T00:00:00Z").date_naive(),
        to_date: time("2026-03-17T00:00:00Z").date_naive(),
        timezone: "UTC".into(),
        start: time("2026-03-17T00:00:00Z"),
        end_exclusive: time("2026-03-18T00:00:00Z"),
        generated_at: time("2026-03-18T06:00:00Z"),
        cash: [(
            "USD".into(),
            StatementCash {
                opening: dec!(10000),
                closing: dec!(9320.75),
                commissions: dec!(3.25),
                net_deposits: dec!(0),
            },
        )]
        .into(),
        executions,
        positions: vec![StatementPosition {
            instrument_id: "1234".into(),
            currency: "USD".into(),
            quantity: dec!(7),
        }],
        cash_flows: vec![],
        blockers: Default::default(),
    };
    (report, ledger)
}

fn matched(statement: &BrokerStatement, ledger: &StatementLedger, closing: Decimal) {
    let report = reconcile(statement, ledger).unwrap();
    assert_eq!(report.status, ReconciliationStatus::Matched, "{report:?}");
    assert_eq!(report.issue_count, 0);
    assert!(report.issues.is_empty());
    assert!(!report.permits_trading);
    assert_eq!(report.cash[0].trust_closing, closing);
    assert_eq!(report.cash[0].broker_closing, closing);
}

#[test]
fn independent_cash_handles_partial_fills_partial_exits_and_late_fee_booking() {
    let (s, l) = fixture();
    // $10,000 - $400 - $606 + $330 - $3.25; seven shares remain.
    matched(&s, &l, dec!(9320.75));
}

#[test]
fn short_cash_uses_sale_proceeds_and_buy_to_cover_not_margin_reservations() {
    let (mut s, mut l) = fixture();
    l.trades[0].entry_buy = false;
    for e in &mut s.executions {
        e.buy = !e.buy;
    }
    for e in &mut l.executions {
        e.execution.buy = !e.execution.buy;
    }
    s.positions[0].quantity = dec!(-7);
    s.cash.get_mut("USD").unwrap().closing = dec!(10672.75);
    // $10,000 + $400 + $606 - $330 - $3.25. Cash is not equity or available funds.
    matched(&s, &l, dec!(10672.75));
}

#[test]
fn missing_manual_and_missing_statement_fills_are_detected_even_if_total_cash_cancels() {
    let (mut s, l) = fixture();
    s.executions[0].execution_id = "manual-not-in-trust".into();
    let report = reconcile(&s, &l).unwrap();
    assert_eq!(report.status, ReconciliationStatus::Discrepancies);
    for code in [
        "execution_missing_in_trust",
        "execution_missing_in_statement",
        "commission_mismatch",
    ] {
        assert!(report.issues.iter().any(|i| i.code == code), "{report:?}");
    }
    assert_eq!(report.cash[0].trust_closing, dec!(9320.75));
}

#[test]
fn every_execution_identity_and_economic_field_is_checked_independently() {
    for changed in [
        "instrument",
        "currency",
        "side",
        "quantity",
        "price",
        "time",
    ] {
        let (mut s, l) = fixture();
        let e = &mut s.executions[0];
        match changed {
            "instrument" => e.instrument_id = "5678".into(),
            "currency" => e.currency = "EUR".into(),
            "side" => e.buy = false,
            "quantity" => e.quantity = dec!(4.000001),
            "price" => e.price = dec!(100.000001),
            "time" => e.occurred_at = time("2026-03-17T10:00:01Z"),
            _ => unreachable!(),
        }
        let result = reconcile(&s, &l).unwrap();
        assert!(
            result.issues.iter().any(|i| i.code == "execution_mismatch"),
            "{changed}: {result:?}"
        );
    }
}

#[test]
fn cash_and_fee_tolerance_is_inclusive_at_one_cent_but_quantities_are_exact() {
    let (mut s, l) = fixture();
    s.cash.get_mut("USD").unwrap().closing = dec!(9320.76);
    assert_eq!(
        reconcile(&s, &l).unwrap().status,
        ReconciliationStatus::Matched
    );
    s.cash.get_mut("USD").unwrap().closing = dec!(9320.760001);
    let report = reconcile(&s, &l).unwrap();
    let issue = report
        .issues
        .iter()
        .find(|i| i.code == "closing_cash_mismatch")
        .unwrap();
    assert_eq!(issue.broker_amount, Some(dec!(9320.760001)));
    assert_eq!(issue.trust_amount, Some(dec!(9320.75)));
    let (mut s, l) = fixture();
    s.positions[0].quantity = dec!(7.00000001);
    assert!(reconcile(&s, &l)
        .unwrap()
        .issues
        .iter()
        .any(|i| i.code == "position_mismatch"));
    let (mut s, l) = fixture();
    s.executions[0].commission += dec!(0.01);
    assert_eq!(
        reconcile(&s, &l).unwrap().status,
        ReconciliationStatus::Matched
    );
    s.executions[0].commission += dec!(0.00001);
    assert!(reconcile(&s, &l)
        .unwrap()
        .issues
        .iter()
        .any(|i| i.code == "commission_mismatch"));
}

#[test]
fn ledger_missing_fee_allocation_duplicates_wrong_accounts_and_overflow_never_match() {
    for bad in [
        "unallocated",
        "missing_receipt",
        "duplicate_execution",
        "duplicate_fee",
        "wrong_account",
        "overflow",
    ] {
        let (s, mut l) = fixture();
        match bad {
            "unallocated" => l.fees[0].trade_id = None,
            "missing_receipt" => {
                l.movements.pop();
            }
            "duplicate_execution" => l.executions.push(l.executions[0].clone()),
            "duplicate_fee" => l.fees.push(l.fees[0].clone()),
            "wrong_account" => l.fees[0].account_id = Uuid::new_v4(),
            "overflow" => l.executions[0].execution.price = Decimal::MAX,
            _ => unreachable!(),
        }
        assert!(reconcile(&s, &l).is_err(), "{bad}");
    }
}

#[test]
fn all_profiles_share_cash_while_internal_transfers_net_to_zero() {
    let (mut s, mut l) = fixture();
    let other = Uuid::new_v4();
    let template = l.movements[0].clone();
    for (account, category, amount) in [
        (template.account_id, "withdrawal", dec!(500)),
        (other, "deposit", dec!(500)),
        (other, "deposit", dec!(1000)),
    ] {
        l.movements.push(RecordedMovement {
            account_id: account,
            category: category.into(),
            amount,
            occurred_at: time("2026-03-17T17:00:00Z"),
            ..template.clone()
        });
    }
    let cash = s.cash.get_mut("USD").unwrap();
    cash.net_deposits = dec!(1000);
    cash.closing = dec!(10320.75);
    matched(&s, &l, dec!(10320.75));
    s.cash.get_mut("USD").unwrap().net_deposits = dec!(1500);
    let result = reconcile(&s, &l).unwrap();
    let issue = result
        .issues
        .iter()
        .find(|i| i.code == "net_cash_flow_mismatch")
        .unwrap();
    assert_eq!(issue.broker_amount, Some(dec!(1500)));
    assert_eq!(issue.trust_amount, Some(dec!(1000)));
}

#[test]
fn unsupported_reports_are_inconclusive_and_issue_output_is_bounded_without_losing_total() {
    let (mut s, l) = fixture();
    s.blockers.insert("unsupported_cash_component".into());
    assert_eq!(
        reconcile(&s, &l).unwrap().status,
        ReconciliationStatus::Inconclusive
    );
    s.blockers.clear();
    for i in 0..250 {
        s.positions.push(StatementPosition {
            instrument_id: format!("external-{i}"),
            currency: "USD".into(),
            quantity: dec!(1),
        });
    }
    let report = reconcile(&s, &l).unwrap();
    assert_eq!(report.status, ReconciliationStatus::Discrepancies);
    assert_eq!(report.issue_count, 250);
    assert_eq!(report.issues.len(), 200);
    assert!(!report.permits_trading);
}
