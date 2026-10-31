#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use chrono::NaiveDate;
use chrono::TimeZone;
use chrono::Utc;
use model::Currency;
use model::Status;
use model::Trade;
use model::TransactionCategory;
use rust_decimal_macros::dec;
use uuid::Uuid;

#[test]
fn test_parse_period_window_last_days() {
    let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
    let (from, to) =
        crate::dispatcher::report_calculations::parse_period_window("last-7-days", now).unwrap();
    assert_eq!(to.unwrap().to_string(), "2026-02-18");
    assert_eq!(from.unwrap().to_string(), "2026-02-12");
}

#[test]
fn test_parse_period_window_previous_days() {
    let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
    let (from, to) =
        crate::dispatcher::report_calculations::parse_period_window("previous-7-days", now)
            .unwrap();
    assert_eq!(to.unwrap().to_string(), "2026-02-11");
    assert_eq!(from.unwrap().to_string(), "2026-02-05");
}

#[test]
fn test_parse_period_window_range() {
    let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
    let (from, to) =
        crate::dispatcher::report_calculations::parse_period_window("2026-01-01..2026-01-31", now)
            .unwrap();
    assert_eq!(from.unwrap().to_string(), "2026-01-01");
    assert_eq!(to.unwrap().to_string(), "2026-01-31");
}

#[test]
fn test_parse_period_window_all() {
    let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
    let (from, to) =
        crate::dispatcher::report_calculations::parse_period_window("all", now).unwrap();
    assert!(from.is_none());
    assert!(to.is_none());
}

#[test]
fn test_parse_period_window_invalid_range_order() {
    let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
    let error =
        crate::dispatcher::report_calculations::parse_period_window("2026-02-10..2026-01-01", now)
            .unwrap_err();
    assert_eq!(error, "Invalid period range: start date is after end date");
}

#[test]
fn test_parse_period_window_invalid_format() {
    let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
    let error = crate::dispatcher::report_calculations::parse_period_window("last-seven-days", now)
        .unwrap_err();
    assert!(
        error.contains("Invalid period format"),
        "unexpected error message: {error}"
    );
}

#[test]
fn test_parse_period_window_invalid_date() {
    let now = Utc.with_ymd_and_hms(2026, 2, 18, 0, 0, 0).unwrap();
    let error =
        crate::dispatcher::report_calculations::parse_period_window("2026-01-40..2026-01-31", now)
            .unwrap_err();
    assert_eq!(error, "Invalid period start date (expected YYYY-MM-DD)");
}

#[test]
fn test_trade_in_window() {
    let trade = Trade {
        updated_at: Utc
            .with_ymd_and_hms(2026, 2, 10, 12, 0, 0)
            .unwrap()
            .naive_utc(),
        ..Trade::default()
    };
    let from = Some(
        Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0)
            .unwrap()
            .date_naive(),
    );
    let to = Some(
        Utc.with_ymd_and_hms(2026, 2, 20, 0, 0, 0)
            .unwrap()
            .date_naive(),
    );
    assert!(crate::dispatcher::report_calculations::trade_in_window(
        &trade, from, to
    ));
}

#[test]
fn test_trade_in_window_rejects_before_from_and_after_to() {
    let trade = Trade {
        updated_at: Utc
            .with_ymd_and_hms(2026, 2, 10, 12, 0, 0)
            .unwrap()
            .naive_utc(),
        ..Trade::default()
    };
    let after_trade = Some(
        Utc.with_ymd_and_hms(2026, 2, 11, 0, 0, 0)
            .unwrap()
            .date_naive(),
    );
    let before_trade = Some(
        Utc.with_ymd_and_hms(2026, 2, 9, 0, 0, 0)
            .unwrap()
            .date_naive(),
    );
    assert!(!crate::dispatcher::report_calculations::trade_in_window(
        &trade,
        after_trade,
        None
    ));
    assert!(!crate::dispatcher::report_calculations::trade_in_window(
        &trade,
        None,
        before_trade
    ));
    assert!(!crate::dispatcher::report_calculations::trade_in_window(
        &trade,
        Some(NaiveDate::from_ymd_opt(2026, 2, 1).unwrap()),
        before_trade
    ));
}

#[test]
fn test_max_drawdown_amount() {
    let mut t1 = Trade::default();
    t1.balance.total_performance = dec!(100);
    let mut t2 = Trade::default();
    t2.balance.total_performance = dec!(-40);
    let mut t3 = Trade::default();
    t3.balance.total_performance = dec!(-30);
    let dd = crate::dispatcher::report_calculations::max_drawdown_amount(&[t1, t2, t3]);
    assert_eq!(dd, dec!(70));
}

#[test]
fn test_max_drawdown_amount_no_drawdown_case() {
    let mut t1 = Trade::default();
    t1.balance.total_performance = dec!(10);
    let mut t2 = Trade::default();
    t2.balance.total_performance = dec!(15);
    let mut t3 = Trade::default();
    t3.balance.total_performance = dec!(5);
    let dd = crate::dispatcher::report_calculations::max_drawdown_amount(&[t1, t2, t3]);
    assert_eq!(dd, dec!(0));
}

#[test]
fn test_report_calmar_ratio_uses_realized_transaction_baseline() {
    let account_id = Uuid::new_v4();
    let trade_id = Uuid::new_v4();
    let base = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid timestamp")
        .naive_utc();

    let mut deposit = model::transaction::Transaction::new(
        account_id,
        TransactionCategory::Deposit,
        &Currency::USD,
        dec!(1000),
    );
    deposit.created_at = base;
    deposit.updated_at = base;

    let mut payout = model::transaction::Transaction::new(
        account_id,
        TransactionCategory::PaymentFromTrade(trade_id),
        &Currency::USD,
        dec!(120),
    );
    payout.created_at = base + chrono::Duration::days(30);
    payout.updated_at = payout.created_at;

    let transactions = vec![deposit, payout];
    let drawdown_metrics =
        crate::dispatcher::report_calculations::drawdown_metrics_for_transactions(&transactions);

    let mut trade = Trade {
        created_at: base,
        updated_at: base + chrono::Duration::days(30),
        status: Status::ClosedTarget,
        ..Trade::default()
    };
    trade.balance.total_performance = dec!(120);

    let calmar = crate::dispatcher::report_calculations::report_calmar_ratio(
        &[trade],
        &transactions,
        &drawdown_metrics,
    );
    assert_eq!(calmar, None);

    let calmar = crate::dispatcher::report_calculations::report_calmar_ratio(
        &[Trade {
            balance: model::trade::TradeBalance {
                total_performance: dec!(120),
                ..Trade::default().balance
            },
            created_at: base,
            updated_at: base + chrono::Duration::days(30),
            status: Status::ClosedTarget,
            ..Trade::default()
        }],
        &transactions,
        &core::calculators_drawdown::DrawdownMetrics {
            current_equity: dec!(1120),
            peak_equity: dec!(1120),
            current_drawdown: dec!(0),
            current_drawdown_percentage: dec!(0),
            max_drawdown: dec!(80),
            max_drawdown_percentage: dec!(8),
            max_drawdown_date: None,
            recovery_from_max: dec!(0),
            recovery_percentage: dec!(0),
            days_since_peak: 0,
            days_in_drawdown: 0,
        },
    )
    .expect("calmar ratio");

    assert!((calmar - dec!(18.25)).abs() < dec!(0.0001));
}

#[test]
fn test_report_calmar_ratio_returns_none_without_realized_equity_baseline() {
    let drawdown_metrics = core::calculators_drawdown::DrawdownMetrics {
        current_equity: dec!(0),
        peak_equity: dec!(0),
        current_drawdown: dec!(0),
        current_drawdown_percentage: dec!(0),
        max_drawdown: dec!(0),
        max_drawdown_percentage: dec!(0),
        max_drawdown_date: None,
        recovery_from_max: dec!(0),
        recovery_percentage: dec!(0),
        days_since_peak: 0,
        days_in_drawdown: 0,
    };
    let mut trade = Trade {
        status: Status::ClosedTarget,
        ..Trade::default()
    };
    trade.balance.total_performance = dec!(100);

    let calmar = crate::dispatcher::report_calculations::report_calmar_ratio(
        &[trade],
        &[],
        &drawdown_metrics,
    );

    assert_eq!(calmar, None);
}
