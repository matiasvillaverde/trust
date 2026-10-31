#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::TradingVehicleCommandBuilder;
use crate::dispatcher::instrument_stats::BondInventoryStats;
use chrono::NaiveDate;
use model::database::TradingVehicleUpsert;
use model::FixedIncomeTerms;
use model::TradingVehicleCategory;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[test]
fn test_trading_vehicle_inventory_stats_summarize_bonds_and_brokers() {
    let vehicles = vec![
        model::TradingVehicle {
            symbol: "AAPL".to_string(),
            broker: "alpaca".to_string(),
            category: TradingVehicleCategory::Stock,
            ..Default::default()
        },
        model::TradingVehicle {
            symbol: "SPY".to_string(),
            broker: "ibkr".to_string(),
            category: TradingVehicleCategory::Etf,
            ..Default::default()
        },
        model::TradingVehicle {
            symbol: "912810TL2".to_string(),
            broker: "ibkr".to_string(),
            category: TradingVehicleCategory::Bond,
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(4)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2030, 1, 15).unwrap()),
                coupon_frequency_per_year: Some(2),
            }),
            ..Default::default()
        },
        model::TradingVehicle {
            symbol: "9128285M8".to_string(),
            broker: "ibkr".to_string(),
            category: TradingVehicleCategory::Bond,
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(6)),
                maturity_date: None,
                coupon_frequency_per_year: Some(2),
            }),
            ..Default::default()
        },
        model::TradingVehicle {
            symbol: "912810TM0".to_string(),
            broker: "ibkr".to_string(),
            category: TradingVehicleCategory::Bond,
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(8)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2040, 1, 15).unwrap()),
                coupon_frequency_per_year: Some(2),
            }),
            ..Default::default()
        },
    ];

    let stats =
        crate::dispatcher::instrument_stats::calculate_trading_vehicle_inventory_stats(&vehicles)
            .expect("inventory stats");
    let payload = crate::dispatcher::instrument_stats::trading_vehicle_stats_payload(&stats);

    assert_eq!(stats.total, 5);
    assert_eq!(stats.by_category.get("stock"), Some(&1));
    assert_eq!(stats.by_category.get("etf"), Some(&1));
    assert_eq!(stats.by_category.get("bond"), Some(&3));
    assert_eq!(stats.by_broker.get("ibkr"), Some(&4));
    assert_eq!(stats.bonds.total, 3);
    assert_eq!(stats.bonds.complete_terms, 2);
    assert_eq!(stats.bonds.missing_terms, 1);
    assert_eq!(stats.bonds.average_coupon_rate_pct, Some(dec!(6)));
    assert_eq!(
        crate::dispatcher::instrument_stats::bond_maturity_range_text(&stats.bonds),
        "2030-01-15..2040-01-15"
    );
    assert_eq!(payload["bonds"]["earliest_maturity"], "2030-01-15");
    assert_eq!(payload["bonds"]["latest_maturity"], "2040-01-15");
    assert_eq!(payload["bonds"]["average_coupon_rate_pct"], "6");
    crate::dispatcher::instrument_stats::print_trading_vehicle_stats(&stats);

    let empty_stats =
        crate::dispatcher::instrument_stats::calculate_trading_vehicle_inventory_stats(&[])
            .expect("empty stats");
    assert_eq!(
        crate::dispatcher::instrument_stats::bond_maturity_range_text(&empty_stats.bonds),
        "n/a"
    );
    crate::dispatcher::instrument_stats::print_trading_vehicle_stats(&empty_stats);
}

#[test]
fn test_trading_vehicle_inventory_stats_report_overflow_edges() {
    let coupon_bond = model::TradingVehicle {
        symbol: "BIGCOUPON".to_string(),
        broker: "ibkr".to_string(),
        category: TradingVehicleCategory::Bond,
        fixed_income: Some(FixedIncomeTerms {
            face_value: Some(dec!(1000)),
            annual_coupon_rate_pct: Some(dec!(1)),
            maturity_date: None,
            coupon_frequency_per_year: Some(2),
        }),
        ..Default::default()
    };

    let mut stats = BondInventoryStats {
        total: 0,
        complete_terms: 0,
        missing_terms: 0,
        coupon_rate_count: 0,
        average_coupon_rate_pct: None,
        earliest_maturity: None,
        latest_maturity: None,
    };
    let mut coupon_rate_sum = Decimal::MAX;
    let mut coupon_rate_divisor = Decimal::ZERO;
    let error = crate::dispatcher::instrument_stats::add_bond_inventory_stats(
        &mut stats,
        &coupon_bond,
        &mut coupon_rate_sum,
        &mut coupon_rate_divisor,
    )
    .expect_err("coupon sum overflow should be reported");
    assert!(error.to_string().contains("coupon sum overflow"));

    let mut stats = BondInventoryStats {
        total: 0,
        complete_terms: 0,
        missing_terms: 0,
        coupon_rate_count: 0,
        average_coupon_rate_pct: None,
        earliest_maturity: None,
        latest_maturity: None,
    };
    let mut coupon_rate_sum = Decimal::ZERO;
    let mut coupon_rate_divisor = Decimal::MAX;
    let error = crate::dispatcher::instrument_stats::add_bond_inventory_stats(
        &mut stats,
        &coupon_bond,
        &mut coupon_rate_sum,
        &mut coupon_rate_divisor,
    )
    .expect_err("coupon count overflow should be reported");
    assert!(error.to_string().contains("coupon count overflow"));

    let error = crate::dispatcher::instrument_stats::checked_increment_usize(usize::MAX)
        .expect_err("usize increment overflow should be reported");
    assert!(error.to_string().contains("count overflow"));

    let mut counts = std::collections::BTreeMap::from([("bond".to_string(), usize::MAX)]);
    let error =
        crate::dispatcher::instrument_stats::increment_count(&mut counts, "bond".to_string())
            .expect_err("map count overflow should be reported");
    assert!(error.to_string().contains("count overflow"));
}

#[test]
fn test_trading_vehicle_stats_dispatch_supports_text_and_json() {
    let mut dispatcher = test_dispatcher();
    dispatcher
        .trust
        .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
        .expect("seed stock vehicle");
    dispatcher
        .trust
        .upsert_trading_vehicle(TradingVehicleUpsert {
            symbol: "912810TL2".to_string(),
            isin: Some("US912810TL26".to_string()),
            category: TradingVehicleCategory::Bond,
            broker: "ibkr".to_string(),
            broker_asset_id: Some("123456".to_string()),
            exchange: Some("SMART".to_string()),
            broker_asset_class: Some("bond".to_string()),
            broker_asset_status: Some("active".to_string()),
            tradable: Some(true),
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(4.25)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2031, 2, 15).unwrap()),
                coupon_frequency_per_year: Some(2),
            }),
        })
        .expect("seed bond vehicle");

    let text = TradingVehicleCommandBuilder::new()
        .stats()
        .build()
        .get_matches_from(["trading-vehicle", "stats"]);
    crate::dispatcher::instrument_stats::trading_vehicle_stats(
        &mut dispatcher.trust,
        text.subcommand_matches("stats").unwrap(),
    )
    .expect("text stats should render");

    let json = TradingVehicleCommandBuilder::new()
        .stats()
        .build()
        .get_matches_from(["trading-vehicle", "stats", "--format", "json"]);
    crate::dispatcher::instrument_stats::trading_vehicle_stats(
        &mut dispatcher.trust,
        json.subcommand_matches("stats").unwrap(),
    )
    .expect("json stats should render");
}
