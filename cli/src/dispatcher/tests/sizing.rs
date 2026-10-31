#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::dispatcher::output::ReportOutputFormat;
use crate::dispatcher::sizing::TradeHypothesisArgs;
use crate::dispatcher::sizing::TradeHypothesisReport;
use crate::dispatcher::sizing::TradePreviewArgs;
use model::Currency;
use model::Environment;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

#[test]
fn test_trade_size_preview_supports_text_output_and_rejects_invalid_currency() {
    let mut dispatcher = test_dispatcher();
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);

    let ok_matches = size_preview_matches(&[
        "--account",
        &account.id.to_string(),
        "--entry",
        "150",
        "--stop",
        "147.5",
        "--currency",
        "usd",
    ]);
    crate::dispatcher::sizing::trade_size_preview(
        &mut dispatcher.trust,
        &ok_matches,
        ReportOutputFormat::Text,
    )
    .expect("text preview should succeed");

    let invalid_currency = size_preview_matches(&[
        "--account",
        &account.id.to_string(),
        "--entry",
        "150",
        "--stop",
        "147.5",
        "--currency",
        "jpy",
    ]);
    let error = crate::dispatcher::sizing::trade_size_preview(
        &mut dispatcher.trust,
        &invalid_currency,
        ReportOutputFormat::Json,
    )
    .expect_err("unsupported currency should fail");
    assert!(error.to_string().contains("invalid_currency"));

    let mut empty_dispatcher = test_dispatcher();
    let account = empty_dispatcher
        .trust
        .create_account(
            "size-no-balance",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account without balance");
    let no_balance = size_preview_matches(&[
        "--account",
        &account.id.to_string(),
        "--entry",
        "0",
        "--stop",
        "147.5",
        "--currency",
        "usd",
    ]);
    let error = crate::dispatcher::sizing::trade_size_preview(
        &mut empty_dispatcher.trust,
        &no_balance,
        ReportOutputFormat::Json,
    )
    .expect_err("size preview should surface calculation failures");
    assert!(error.to_string().contains("size_preview_failed"));

    let mut level_failure_dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::level_fails_after_quantity(),
    );
    let level_failure = size_preview_matches(&[
        "--account",
        &Uuid::new_v4().to_string(),
        "--entry",
        "150",
        "--stop",
        "147.5",
        "--currency",
        "usd",
    ]);
    let error = crate::dispatcher::sizing::trade_size_preview(
        &mut level_failure_dispatcher.trust,
        &level_failure,
        ReportOutputFormat::Json,
    )
    .expect_err("size preview should surface post-calculation level load failures");
    assert_eq!(error.code, "size_preview_level_load_failed");
}

#[test]
fn test_trade_hypothesis_supports_text_output_and_rejects_invalid_inputs() {
    let mut dispatcher = test_dispatcher();
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);

    let ok_matches = hypothesis_matches(&[
        "--account",
        &account.id.to_string(),
        "--entry",
        "150",
        "--stop",
        "147.5",
        "--target",
        "160",
        "--quantity",
        "20",
        "--currency",
        "usd",
    ]);
    crate::dispatcher::sizing::trade_hypothesis(
        &mut dispatcher.trust,
        &ok_matches,
        ReportOutputFormat::Text,
    )
    .expect("text hypothesis should succeed");

    let invalid_quantity = hypothesis_matches(&[
        "--account",
        &account.id.to_string(),
        "--entry",
        "150",
        "--stop",
        "147.5",
        "--target",
        "160",
        "--quantity",
        "oops",
        "--currency",
        "usd",
    ]);
    let quantity_error = crate::dispatcher::sizing::trade_hypothesis(
        &mut dispatcher.trust,
        &invalid_quantity,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid quantity should fail");
    assert!(quantity_error.to_string().contains("invalid_quantity"));

    let missing_quantity = hypothesis_matches(&[
        "--account",
        &account.id.to_string(),
        "--entry",
        "150",
        "--stop",
        "147.5",
        "--target",
        "160",
        "--currency",
        "usd",
    ]);
    let missing_quantity_error = crate::dispatcher::sizing::trade_hypothesis(
        &mut dispatcher.trust,
        &missing_quantity,
        ReportOutputFormat::Json,
    )
    .expect_err("missing quantity should fail");
    assert!(missing_quantity_error
        .to_string()
        .contains("missing_argument"));

    let zero_quantity = hypothesis_matches(&[
        "--account",
        &account.id.to_string(),
        "--entry",
        "150",
        "--stop",
        "147.5",
        "--target",
        "160",
        "--quantity",
        "0",
        "--currency",
        "usd",
    ]);
    let zero_quantity_error = crate::dispatcher::sizing::trade_hypothesis(
        &mut dispatcher.trust,
        &zero_quantity,
        ReportOutputFormat::Json,
    )
    .expect_err("zero quantity should fail");
    assert!(zero_quantity_error.to_string().contains("invalid_quantity"));

    let invalid_currency = hypothesis_matches(&[
        "--account",
        &account.id.to_string(),
        "--entry",
        "150",
        "--stop",
        "147.5",
        "--target",
        "160",
        "--quantity",
        "20",
        "--currency",
        "jpy",
    ]);
    let currency_error = crate::dispatcher::sizing::trade_hypothesis(
        &mut dispatcher.trust,
        &invalid_currency,
        ReportOutputFormat::Json,
    )
    .expect_err("unsupported currency should fail");
    assert!(currency_error.to_string().contains("invalid_currency"));
}

#[test]
fn test_size_preview_levels_marks_current_level_and_scales_risk() {
    let levels = crate::dispatcher::sizing::size_preview_levels(dec!(100), 3, dec!(2.5));
    assert_eq!(levels.len(), 5);
    assert_eq!(levels[0]["quantity"], "10");
    assert_eq!(levels[2]["quantity"], "50");
    assert_eq!(levels[3]["current"], true);
    assert_eq!(levels[4]["quantity"], "150");
    assert_eq!(levels[4]["risk_amount"], "375");
}

#[test]
fn test_trade_size_preview_text_lines_include_current_marker() {
    let preview = TradePreviewArgs {
        account_id: Uuid::nil(),
        entry_price: dec!(200),
        stop_price: dec!(190),
        currency: Currency::USD,
    };
    let levels = crate::dispatcher::sizing::size_preview_levels(dec!(100), 4, dec!(10));
    let lines = crate::dispatcher::sizing::trade_size_preview_text_lines(
        &preview,
        dec!(100),
        dec!(150),
        dec!(1.5),
        &levels,
    );

    assert_eq!(lines[0], "Position Size Preview");
    assert!(lines[3].contains("Risk/Share: 10 USD"));
    assert!(lines
        .iter()
        .any(|line| line.contains("L4 (1.5x): qty 150 | risk 1500 USD  <- current")));
}

#[test]
fn test_trade_hypothesis_formatters_render_na_for_missing_values() {
    assert_eq!(
        crate::dispatcher::sizing::format_optional_decimal(None),
        "n/a"
    );
    assert_eq!(
        crate::dispatcher::sizing::format_optional_percentage(None),
        "n/a"
    );
    assert_eq!(
        crate::dispatcher::sizing::format_optional_decimal(Some(dec!(4.250000000))),
        "4.25"
    );
    assert_eq!(
        crate::dispatcher::sizing::format_optional_percentage(Some(dec!(1.600000000))),
        "1.6%"
    );
}

#[test]
fn test_trade_hypothesis_text_lines_render_optional_values() {
    let args = TradeHypothesisArgs {
        preview: TradePreviewArgs {
            account_id: Uuid::nil(),
            entry_price: dec!(40),
            stop_price: dec!(38),
            currency: Currency::USD,
        },
        target_price: dec!(48),
        quantity: 100.into(),
    };
    let report = TradeHypothesisReport {
        available_capital: Decimal::ZERO,
        capital_required: dec!(4000),
        capital_required_pct_of_available: None,
        risk_per_share: dec!(2),
        reward_per_share: dec!(8),
        max_loss: dec!(200),
        max_loss_pct_of_available: None,
        max_gain: dec!(800),
        max_gain_pct_of_available: None,
        risk_reward_ratio: Some(dec!(4)),
    };

    let lines = crate::dispatcher::sizing::trade_hypothesis_text_lines(&args, &report);
    assert_eq!(lines[0], "Trade Hypothesis");
    assert!(lines[4].contains("Available capital: 0 USD"));
    assert!(lines[5].contains("Capital required: 4000 USD (n/a)"));
    assert!(lines[6].contains("Risk/share: 2 USD | Reward/share: 8 USD | R:R 4"));
    assert!(lines[7].contains("Max loss: 200 USD (n/a)"));
    assert!(lines[8].contains("Max gain: 800 USD (n/a)"));
}

#[test]
fn test_trade_hypothesis_payload_preserves_optional_fields() {
    let args = TradeHypothesisArgs {
        preview: TradePreviewArgs {
            account_id: Uuid::nil(),
            entry_price: dec!(40),
            stop_price: dec!(38),
            currency: Currency::USD,
        },
        target_price: dec!(48),
        quantity: 100.into(),
    };
    let report = TradeHypothesisReport {
        available_capital: dec!(50_000),
        capital_required: dec!(4000),
        capital_required_pct_of_available: Some(dec!(8)),
        risk_per_share: dec!(2),
        reward_per_share: dec!(8),
        max_loss: dec!(200),
        max_loss_pct_of_available: Some(dec!(0.4)),
        max_gain: dec!(800),
        max_gain_pct_of_available: None,
        risk_reward_ratio: Some(dec!(4)),
    };

    let payload = crate::dispatcher::sizing::trade_hypothesis_payload(&args, &report);
    assert_eq!(payload["report"], "trade_hypothesis");
    assert_eq!(payload["scope"]["account_id"], Uuid::nil().to_string());
    assert_eq!(payload["data"]["capital_required_pct_of_available"], "8");
    assert_eq!(payload["data"]["max_loss_pct_of_available"], "0.4");
    assert!(payload["data"]["max_gain_pct_of_available"].is_null());
    assert_eq!(payload["data"]["risk_reward_ratio"], "4");
}
