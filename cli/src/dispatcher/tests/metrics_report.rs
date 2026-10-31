#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use crate::dispatcher::output::ReportOutputFormat;
use chrono::NaiveDate;
use clap::Arg;
use clap::Command;
use model::Currency;
use rust_decimal_macros::dec;
use uuid::Uuid;

#[test]
fn test_parse_required_date_range_validates_format_and_order() {
    let valid = Command::new("test")
        .arg(Arg::new("from").long("from").required(true))
        .arg(Arg::new("to").long("to").required(true))
        .get_matches_from(["test", "--from", "2026-01-01", "--to", "2026-01-31"]);
    let (from, to) = crate::dispatcher::metrics_report::parse_required_date_range(
        &valid,
        ReportOutputFormat::Json,
    )
    .expect("valid range should parse");
    assert_eq!(
        from,
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid literal date")
    );
    assert_eq!(
        to,
        NaiveDate::from_ymd_opt(2026, 1, 31).expect("valid literal date")
    );

    let invalid = Command::new("test")
        .arg(Arg::new("from").long("from").required(true))
        .arg(Arg::new("to").long("to").required(true))
        .get_matches_from(["test", "--from", "bad", "--to", "2026-01-31"]);
    let err = crate::dispatcher::metrics_report::parse_required_date_range(
        &invalid,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid date should fail");
    assert!(err.to_string().contains("invalid_date"));

    let reversed = Command::new("test")
        .arg(Arg::new("from").long("from").required(true))
        .arg(Arg::new("to").long("to").required(true))
        .get_matches_from(["test", "--from", "2026-02-01", "--to", "2026-01-31"]);
    let err = crate::dispatcher::metrics_report::parse_required_date_range(
        &reversed,
        ReportOutputFormat::Json,
    )
    .expect_err("reversed date range should fail");
    assert!(err.to_string().contains("invalid_date_range"));
}

#[test]
fn test_transaction_cashflow_delta_sign_rules() {
    let account_id = Uuid::new_v4();
    let trade_id = Uuid::new_v4();
    let amount = dec!(10);
    let mk = |category: model::TransactionCategory| {
        model::Transaction::new(account_id, category, &Currency::USD, amount)
    };

    assert_eq!(
        crate::dispatcher::metrics_report::transaction_cashflow_delta(&mk(
            model::TransactionCategory::Deposit
        )),
        dec!(10)
    );
    assert_eq!(
        crate::dispatcher::metrics_report::transaction_cashflow_delta(&mk(
            model::TransactionCategory::CloseTarget(trade_id)
        )),
        dec!(10)
    );
    assert_eq!(
        crate::dispatcher::metrics_report::transaction_cashflow_delta(&mk(
            model::TransactionCategory::Withdrawal
        )),
        dec!(-10)
    );
    assert_eq!(
        crate::dispatcher::metrics_report::transaction_cashflow_delta(&mk(
            model::TransactionCategory::FeeClose(trade_id)
        )),
        dec!(-10)
    );
    assert_eq!(
        crate::dispatcher::metrics_report::transaction_cashflow_delta(&mk(
            model::TransactionCategory::FundTrade(trade_id)
        )),
        dec!(0)
    );
    assert_eq!(
        crate::dispatcher::metrics_report::transaction_cashflow_delta(&mk(
            model::TransactionCategory::PaymentFromTrade(trade_id)
        )),
        dec!(0)
    );
}
