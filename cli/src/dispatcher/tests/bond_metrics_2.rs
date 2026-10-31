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
use model::database::TradingVehicleUpsert;
use model::FixedIncomeTerms;
use model::TradingVehicleCategory;
use rust_decimal_macros::dec;

#[test]
fn test_metrics_bond_executes_text_and_validates_lookup_and_accrual_edges() {
    let mut dispatcher = test_dispatcher();
    let upsert_vehicle = |dispatcher: &mut ArgDispatcher,
                          symbol: &str,
                          category: TradingVehicleCategory,
                          fixed_income: Option<FixedIncomeTerms>| {
        dispatcher
            .trust
            .upsert_trading_vehicle(TradingVehicleUpsert {
                symbol: symbol.to_string(),
                isin: None,
                category,
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
                fixed_income,
            })
            .expect("upsert vehicle");
    };

    let text = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--face-value",
            "1000",
            "--market-price",
            "980",
            "--coupon-rate",
            "5",
            "--quantity",
            "2",
            "--years-to-maturity",
            "4",
            "--coupon-frequency",
            "2",
            "--settlement-date",
            "2026-04-01",
            "--last-coupon-date",
            "2026-01-01",
            "--next-coupon-date",
            "2026-07-01",
            "--day-count",
            "actual-365",
        ]);
    crate::dispatcher::bond_metrics::metrics_bond(
        &mut dispatcher.trust,
        text.subcommand_matches("bond").expect("bond subcommand"),
    )
    .expect("bond metrics text output should render");

    for args in [
        vec![
            "metrics",
            "bond",
            "--symbol",
            "BOND1",
            "--market-price",
            "1000",
            "--quantity",
            "1",
            "--years-to-maturity",
            "1",
        ],
        vec![
            "metrics",
            "bond",
            "--broker",
            "ibkr",
            "--market-price",
            "1000",
            "--quantity",
            "1",
            "--years-to-maturity",
            "1",
        ],
        vec![
            "metrics",
            "bond",
            "--symbol",
            "MISSING",
            "--broker",
            "ibkr",
            "--market-price",
            "1000",
            "--quantity",
            "1",
            "--years-to-maturity",
            "1",
        ],
    ] {
        let matches = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from(args);
        let error = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
            &mut dispatcher.trust,
            matches.subcommand_matches("bond").expect("bond subcommand"),
            ReportOutputFormat::Json,
        )
        .expect_err("invalid lookup shape should fail");
        assert!(
            error.to_string().contains("missing_argument")
                || error.to_string().contains("bond_vehicle_not_found")
        );
    }

    upsert_vehicle(&mut dispatcher, "ETF1", TradingVehicleCategory::Etf, None);
    let wrong_category = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--symbol",
            "ETF1",
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
        wrong_category
            .subcommand_matches("bond")
            .expect("bond subcommand"),
        ReportOutputFormat::Json,
    )
    .expect_err("non-bond vehicle should fail");
    assert!(error.to_string().contains("bond_vehicle_wrong_category"));

    upsert_vehicle(
        &mut dispatcher,
        "NOTERMS",
        TradingVehicleCategory::Bond,
        None,
    );
    let no_terms = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--symbol",
            "NOTERMS",
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
        no_terms
            .subcommand_matches("bond")
            .expect("bond subcommand"),
        ReportOutputFormat::Json,
    )
    .expect_err("bond without fixed-income terms should fail");
    assert!(error.to_string().contains("bond_terms_missing"));

    upsert_vehicle(
        &mut dispatcher,
        "NOFACE",
        TradingVehicleCategory::Bond,
        Some(FixedIncomeTerms {
            face_value: None,
            annual_coupon_rate_pct: Some(dec!(5)),
            maturity_date: Some(NaiveDate::from_ymd_opt(2030, 1, 1).unwrap()),
            coupon_frequency_per_year: Some(2),
        }),
    );
    let no_face = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--symbol",
            "NOFACE",
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
        no_face.subcommand_matches("bond").expect("bond subcommand"),
        ReportOutputFormat::Json,
    )
    .expect_err("missing stored face value should fail");
    assert!(error.to_string().contains("bond_terms_missing"));

    upsert_vehicle(
        &mut dispatcher,
        "NOMATURITY",
        TradingVehicleCategory::Bond,
        Some(FixedIncomeTerms {
            face_value: Some(dec!(1000)),
            annual_coupon_rate_pct: Some(dec!(5)),
            maturity_date: None,
            coupon_frequency_per_year: Some(2),
        }),
    );
    let no_maturity = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--symbol",
            "NOMATURITY",
            "--broker",
            "ibkr",
            "--market-price",
            "1000",
            "--quantity",
            "1",
        ]);
    let error = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
        &mut dispatcher.trust,
        no_maturity
            .subcommand_matches("bond")
            .expect("bond subcommand"),
        ReportOutputFormat::Json,
    )
    .expect_err("missing maturity should require explicit years");
    assert!(error.to_string().contains("bond_terms_missing"));

    for (args, expected) in [
        (
            vec![
                "metrics",
                "bond",
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
                "0",
                "--settlement-date",
                "2026-04-01",
                "--last-coupon-date",
                "2026-01-01",
                "--next-coupon-date",
                "2026-07-01",
            ],
            "invalid_coupon_frequency",
        ),
        (
            vec![
                "metrics",
                "bond",
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
                "--last-coupon-date",
                "2026-01-01",
                "--next-coupon-date",
                "2026-07-01",
            ],
            "missing_argument",
        ),
        (
            vec![
                "metrics",
                "bond",
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
                "bad-date",
                "--last-coupon-date",
                "2026-01-01",
                "--next-coupon-date",
                "2026-07-01",
            ],
            "invalid_date",
        ),
    ] {
        let matches = MetricsCommandBuilder::new()
            .bond()
            .build()
            .get_matches_from(args);
        let error = crate::dispatcher::bond_metrics::parse_bond_analytics_input(
            &mut dispatcher.trust,
            matches.subcommand_matches("bond").expect("bond subcommand"),
            ReportOutputFormat::Json,
        )
        .expect_err("invalid accrued-interest inputs should fail");
        assert!(error.to_string().contains(expected));
    }
}

#[test]
fn test_years_to_maturity_uses_decimal_day_count() {
    let maturity = NaiveDate::from_ymd_opt(2027, 1, 1).unwrap();
    let as_of = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let years = crate::dispatcher::bond_metrics::years_to_maturity(
        maturity,
        as_of,
        ReportOutputFormat::Text,
    )
    .unwrap();
    assert_eq!(years, dec!(1));
}
