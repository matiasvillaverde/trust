#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::MetricsCommandBuilder;
use crate::dispatcher::output::ReportOutputFormat;
use crate::dispatcher::ArgDispatcher;
use chrono::NaiveDate;
use clap::Arg;
use clap::Command;
use model::database::TradingVehicleUpsert;
use model::FixedIncomeTerms;
use model::TradingVehicleCategory;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn test_parse_bond_analytics_input_loads_stored_fixed_income_terms() {
    let mut dispatcher = test_dispatcher();
    dispatcher
        .trust
        .upsert_trading_vehicle(TradingVehicleUpsert {
            symbol: "9128285M8".to_string(),
            isin: None,
            category: TradingVehicleCategory::Bond,
            broker: "ibkr".to_string(),
            broker_asset_id: Some("123456".to_string()),
            exchange: Some("SMART".to_string()),
            broker_asset_class: Some("bond".to_string()),
            broker_asset_status: None,
            tradable: None,
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(4.625)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                coupon_frequency_per_year: Some(2),
            }),
        })
        .unwrap();

    let matches = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--symbol",
            "9128285M8",
            "--broker",
            "ibkr",
            "--market-price",
            "997.50",
            "--quantity",
            "5",
            "--years-to-maturity",
            "7",
        ]);
    let bond = matches.subcommand_matches("bond").unwrap();
    let input = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
        &mut dispatcher.trust,
        bond,
        ReportOutputFormat::Text,
    )
    .unwrap();

    assert_eq!(input.face_value, dec!(1000));
    assert_eq!(input.annual_coupon_rate_pct, dec!(4.625));
    assert_eq!(input.market_price, dec!(997.50));
    assert_eq!(input.quantity, 5);
    assert_eq!(input.years_to_maturity, dec!(7));
    assert_eq!(input.accrued_interest, None);
}

#[test]
fn test_parse_bond_analytics_input_builds_accrued_interest_from_stored_frequency() {
    let mut dispatcher = test_dispatcher();
    dispatcher
        .trust
        .upsert_trading_vehicle(TradingVehicleUpsert {
            symbol: "9128285M8".to_string(),
            isin: None,
            category: TradingVehicleCategory::Bond,
            broker: "ibkr".to_string(),
            broker_asset_id: Some("123456".to_string()),
            exchange: Some("SMART".to_string()),
            broker_asset_class: Some("bond".to_string()),
            broker_asset_status: None,
            tradable: None,
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(6)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                coupon_frequency_per_year: Some(2),
            }),
        })
        .unwrap();

    let matches = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--symbol",
            "9128285M8",
            "--broker",
            "ibkr",
            "--market-price",
            "1000",
            "--quantity",
            "10",
            "--years-to-maturity",
            "5",
            "--settlement-date",
            "2026-04-01",
            "--last-coupon-date",
            "2026-01-01",
            "--next-coupon-date",
            "2026-07-01",
            "--day-count",
            "actual-360",
        ]);
    let bond = matches.subcommand_matches("bond").unwrap();
    let input = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
        &mut dispatcher.trust,
        bond,
        ReportOutputFormat::Text,
    )
    .unwrap();
    let accrued = input.accrued_interest.unwrap();

    assert_eq!(
        accrued.settlement_date,
        NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()
    );
    assert_eq!(accrued.coupon_frequency_per_year, 2);
    assert_eq!(
        accrued.day_count_basis,
        core::calculators_fixed_income::DayCountBasis::Actual360
    );
}

#[test]
fn test_parse_bond_analytics_input_requires_frequency_for_manual_accrual() {
    let mut dispatcher = test_dispatcher();
    let matches = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--face-value",
            "1000",
            "--market-price",
            "1000",
            "--coupon-rate",
            "6",
            "--quantity",
            "10",
            "--years-to-maturity",
            "5",
            "--settlement-date",
            "2026-04-01",
            "--last-coupon-date",
            "2026-01-01",
            "--next-coupon-date",
            "2026-07-01",
        ]);
    let bond = matches.subcommand_matches("bond").unwrap();
    let error = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
        &mut dispatcher.trust,
        bond,
        ReportOutputFormat::Text,
    )
    .unwrap_err();

    assert!(error.to_string().contains("missing_argument"));
}

#[test]
fn test_bond_metrics_cover_error_wrapping_and_stored_term_edges() {
    let mut dispatcher = test_dispatcher();
    let upsert_bond =
        |dispatcher: &mut ArgDispatcher, symbol: &str, fixed_income: FixedIncomeTerms| {
            dispatcher
                .trust
                .upsert_trading_vehicle(TradingVehicleUpsert {
                    symbol: symbol.to_string(),
                    isin: None,
                    category: TradingVehicleCategory::Bond,
                    broker: "ibkr".to_string(),
                    broker_asset_id: Some(format!("asset-{symbol}")),
                    exchange: Some("SMART".to_string()),
                    broker_asset_class: Some("bond".to_string()),
                    broker_asset_status: None,
                    tradable: None,
                    marginable: None,
                    shortable: None,
                    easy_to_borrow: None,
                    fractionable: None,
                    fixed_income: Some(fixed_income),
                })
                .expect("seed bond vehicle");
        };

    upsert_bond(
        &mut dispatcher,
        "OVERRIDE",
        FixedIncomeTerms {
            face_value: Some(dec!(1000)),
            annual_coupon_rate_pct: Some(dec!(4.25)),
            maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
            coupon_frequency_per_year: Some(2),
        },
    );
    let override_matches = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--symbol",
            "OVERRIDE",
            "--broker",
            "ibkr",
            "--face-value",
            "1001",
            "--market-price",
            "995.25",
            "--coupon-rate",
            "4.5",
            "--quantity",
            "3",
        ]);
    let input = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
        &mut dispatcher.trust,
        override_matches
            .subcommand_matches("bond")
            .expect("bond subcommand"),
        ReportOutputFormat::Text,
    )
    .expect("stored bond should accept manual overrides");
    assert_eq!(input.face_value, dec!(1001));
    assert_eq!(input.annual_coupon_rate_pct, dec!(4.5));
    assert!(input.years_to_maturity > Decimal::ZERO);

    upsert_bond(
        &mut dispatcher,
        "MATURED",
        FixedIncomeTerms {
            face_value: Some(dec!(1000)),
            annual_coupon_rate_pct: Some(dec!(3.75)),
            maturity_date: Some(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()),
            coupon_frequency_per_year: Some(2),
        },
    );
    let matured_matches = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--symbol",
            "MATURED",
            "--broker",
            "ibkr",
            "--market-price",
            "1000",
            "--quantity",
            "1",
        ]);
    let matured = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
        &mut dispatcher.trust,
        matured_matches
            .subcommand_matches("bond")
            .expect("bond subcommand"),
        ReportOutputFormat::Text,
    )
    .expect("past stored maturity should clamp to zero years");
    assert_eq!(matured.years_to_maturity, Decimal::ZERO);

    upsert_bond(
        &mut dispatcher,
        "NOCOUPON",
        FixedIncomeTerms {
            face_value: Some(dec!(1000)),
            annual_coupon_rate_pct: None,
            maturity_date: Some(NaiveDate::from_ymd_opt(2030, 1, 1).unwrap()),
            coupon_frequency_per_year: Some(2),
        },
    );
    let no_coupon = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--symbol",
            "NOCOUPON",
            "--broker",
            "ibkr",
            "--market-price",
            "1000",
            "--quantity",
            "1",
            "--years-to-maturity",
            "1",
        ]);
    let error = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
        &mut dispatcher.trust,
        no_coupon
            .subcommand_matches("bond")
            .expect("bond subcommand"),
        ReportOutputFormat::Json,
    )
    .expect_err("missing stored coupon rate should fail");
    assert!(error.to_string().contains("bond_terms_missing"));

    upsert_bond(
        &mut dispatcher,
        "NOFREQ",
        FixedIncomeTerms {
            face_value: Some(dec!(1000)),
            annual_coupon_rate_pct: Some(dec!(5)),
            maturity_date: Some(NaiveDate::from_ymd_opt(2030, 1, 1).unwrap()),
            coupon_frequency_per_year: None,
        },
    );
    let no_frequency = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--symbol",
            "NOFREQ",
            "--broker",
            "ibkr",
            "--market-price",
            "1000",
            "--quantity",
            "1",
            "--years-to-maturity",
            "1",
            "--settlement-date",
            "2026-04-01",
            "--last-coupon-date",
            "2026-01-01",
            "--next-coupon-date",
            "2026-07-01",
        ]);
    let error = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
        &mut dispatcher.trust,
        no_frequency
            .subcommand_matches("bond")
            .expect("bond subcommand"),
        ReportOutputFormat::Json,
    )
    .expect_err("missing stored coupon frequency should fail for accrual");
    assert!(error.to_string().contains("missing_argument"));

    let invalid_calculation = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--face-value",
            "1000",
            "--market-price",
            "1000",
            "--coupon-rate",
            "5",
            "--quantity",
            "0",
            "--years-to-maturity",
            "1",
        ]);
    let error = crate::dispatcher::bond_metrics::metrics_bond(
        &mut dispatcher.trust,
        invalid_calculation
            .subcommand_matches("bond")
            .expect("bond subcommand"),
    )
    .expect_err("invalid quantity should be wrapped as bond analytics failure");
    assert!(error.to_string().contains("bond_analytics_failed"));
}

#[test]
fn test_parse_bond_analytics_input_reaches_lax_accrual_validation_edges() {
    fn lax_bond_matches(args: &[&str]) -> clap::ArgMatches {
        let mut argv = vec!["bond"];
        argv.extend_from_slice(args);
        Command::new("bond")
            .arg(Arg::new("symbol").long("symbol"))
            .arg(Arg::new("broker").long("broker"))
            .arg(Arg::new("face-value").long("face-value"))
            .arg(Arg::new("market-price").long("market-price"))
            .arg(Arg::new("coupon-rate").long("coupon-rate"))
            .arg(
                Arg::new("quantity")
                    .long("quantity")
                    .value_parser(clap::value_parser!(i64)),
            )
            .arg(Arg::new("years-to-maturity").long("years-to-maturity"))
            .arg(
                Arg::new("coupon-frequency")
                    .long("coupon-frequency")
                    .value_parser(clap::value_parser!(u16)),
            )
            .arg(Arg::new("settlement-date").long("settlement-date"))
            .arg(Arg::new("last-coupon-date").long("last-coupon-date"))
            .arg(Arg::new("next-coupon-date").long("next-coupon-date"))
            .arg(Arg::new("day-count").long("day-count"))
            .get_matches_from(argv)
    }

    let mut dispatcher = test_dispatcher();
    let missing_quantity = lax_bond_matches(&[
        "--face-value",
        "1000",
        "--market-price",
        "1000",
        "--coupon-rate",
        "5",
        "--years-to-maturity",
        "1",
    ]);
    let error = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
        &mut dispatcher.trust,
        &missing_quantity,
        ReportOutputFormat::Json,
    )
    .expect_err("missing quantity should be reported by dispatcher");
    assert!(error.to_string().contains("missing_argument"));

    for args in [
        vec![
            "--face-value",
            "1000",
            "--market-price",
            "1000",
            "--quantity",
            "1",
            "--years-to-maturity",
            "1",
        ],
        vec![
            "--face-value",
            "1000",
            "--market-price",
            "1000",
            "--coupon-rate",
            "5",
            "--quantity",
            "1",
        ],
    ] {
        let matches = lax_bond_matches(&args);
        let error = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
            &mut dispatcher.trust,
            &matches,
            ReportOutputFormat::Json,
        )
        .expect_err("missing manual bond analytics argument should fail");
        assert!(error.to_string().contains("missing_argument"));
    }

    let invalid_day_count = lax_bond_matches(&[
        "--face-value",
        "1000",
        "--market-price",
        "1000",
        "--coupon-rate",
        "5",
        "--quantity",
        "1",
        "--years-to-maturity",
        "1",
        "--coupon-frequency",
        "2",
        "--settlement-date",
        "2026-04-01",
        "--last-coupon-date",
        "2026-01-01",
        "--next-coupon-date",
        "2026-07-01",
        "--day-count",
        "30-360",
    ]);
    let error = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
        &mut dispatcher.trust,
        &invalid_day_count,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid day-count should be reported by dispatcher");
    assert!(error.to_string().contains("invalid_day_count"));

    for (args, expected) in [
        (
            vec![
                "--face-value",
                "1000",
                "--market-price",
                "1000",
                "--coupon-rate",
                "5",
                "--quantity",
                "1",
                "--years-to-maturity",
                "1",
                "--coupon-frequency",
                "2",
                "--settlement-date",
                "2026-04-01",
                "--last-coupon-date",
                "bad-date",
                "--next-coupon-date",
                "2026-07-01",
            ],
            "invalid_date",
        ),
        (
            vec![
                "--face-value",
                "1000",
                "--market-price",
                "1000",
                "--coupon-rate",
                "5",
                "--quantity",
                "1",
                "--years-to-maturity",
                "1",
                "--coupon-frequency",
                "2",
                "--settlement-date",
                "2026-04-01",
                "--last-coupon-date",
                "2026-01-01",
                "--next-coupon-date",
                "bad-date",
            ],
            "invalid_date",
        ),
    ] {
        let matches = lax_bond_matches(&args);
        let error = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
            &mut dispatcher.trust,
            &matches,
            ReportOutputFormat::Json,
        )
        .expect_err("invalid coupon date should be reported by dispatcher");
        assert!(error.to_string().contains(expected));
    }
}
