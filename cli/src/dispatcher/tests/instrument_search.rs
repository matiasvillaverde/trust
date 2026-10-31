#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::TradingVehicleCommandBuilder;
use crate::dispatcher::instrument_search::TradingVehicleSearchFilters;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::NaiveDate;
use model::database::TradingVehicleUpsert;
use model::FixedIncomeTerms;
use model::TradingVehicleCategory;
use rust_decimal_macros::dec;

#[test]
fn test_trading_vehicle_search_filters_find_incomplete_bonds() {
    let vehicles = vec![
        model::TradingVehicle {
            symbol: "AAPL".to_string(),
            broker: "ibkr".to_string(),
            category: TradingVehicleCategory::Stock,
            ..Default::default()
        },
        model::TradingVehicle {
            symbol: "9128285M8".to_string(),
            broker: "ibkr".to_string(),
            isin: Some("US9128285M81".to_string()),
            category: TradingVehicleCategory::Bond,
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: None,
                maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                coupon_frequency_per_year: Some(2),
            }),
            ..Default::default()
        },
        model::TradingVehicle {
            symbol: "912810TL2".to_string(),
            broker: "ibkr".to_string(),
            category: TradingVehicleCategory::Bond,
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(4.625)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                coupon_frequency_per_year: Some(2),
            }),
            ..Default::default()
        },
    ];
    let filters = TradingVehicleSearchFilters {
        category: Some(TradingVehicleCategory::Bond),
        broker: Some("ibkr".to_string()),
        symbol: Some("912".to_string()),
        isin: None,
        missing_bond_terms: true,
        list_all: false,
        format: ReportOutputFormat::Text,
    };

    let filtered =
        crate::dispatcher::instrument_search::filter_trading_vehicles(vehicles, &filters);

    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].symbol, "9128285M8");
}

#[test]
fn test_trading_vehicle_search_filters_cover_all_predicates() {
    let base_filters = TradingVehicleSearchFilters {
        category: None,
        broker: None,
        symbol: None,
        isin: None,
        missing_bond_terms: false,
        list_all: false,
        format: ReportOutputFormat::Text,
    };
    assert!(!base_filters.has_non_interactive_filters());

    let mut list_all = base_filters.clone();
    list_all.list_all = true;
    assert!(list_all.has_non_interactive_filters());

    let mut by_category = base_filters.clone();
    by_category.category = Some(TradingVehicleCategory::Stock);
    assert!(by_category.has_non_interactive_filters());

    let mut by_broker = base_filters.clone();
    by_broker.broker = Some("ibkr".to_string());
    assert!(by_broker.has_non_interactive_filters());

    let mut by_symbol = base_filters.clone();
    by_symbol.symbol = Some("AAPL".to_string());
    assert!(by_symbol.has_non_interactive_filters());

    let mut by_isin = base_filters.clone();
    by_isin.isin = Some("US0378331005".to_string());
    assert!(by_isin.has_non_interactive_filters());

    let mut missing_terms = base_filters.clone();
    missing_terms.missing_bond_terms = true;
    assert!(missing_terms.has_non_interactive_filters());

    let mut json = base_filters.clone();
    json.format = ReportOutputFormat::Json;
    assert!(json.has_non_interactive_filters());

    let stock = model::TradingVehicle {
        symbol: "AAPL".to_string(),
        broker: "ibkr".to_string(),
        isin: Some("US0378331005".to_string()),
        category: TradingVehicleCategory::Stock,
        ..Default::default()
    };

    assert!(
        crate::dispatcher::instrument_search::trading_vehicle_matches_filters(&stock, &by_category)
    );
    assert!(
        crate::dispatcher::instrument_search::trading_vehicle_matches_filters(&stock, &by_broker)
    );

    let mut wrong_broker = base_filters.clone();
    wrong_broker.broker = Some("alpaca".to_string());
    assert!(
        !crate::dispatcher::instrument_search::trading_vehicle_matches_filters(
            &stock,
            &wrong_broker
        )
    );

    let mut wrong_symbol = base_filters.clone();
    wrong_symbol.symbol = Some("MSFT".to_string());
    assert!(
        !crate::dispatcher::instrument_search::trading_vehicle_matches_filters(
            &stock,
            &wrong_symbol
        )
    );

    let mut wrong_isin = base_filters.clone();
    wrong_isin.isin = Some("US999".to_string());
    assert!(
        !crate::dispatcher::instrument_search::trading_vehicle_matches_filters(&stock, &wrong_isin)
    );

    let stock_without_isin = model::TradingVehicle {
        isin: None,
        ..stock.clone()
    };
    assert!(
        !crate::dispatcher::instrument_search::trading_vehicle_matches_filters(
            &stock_without_isin,
            &wrong_isin
        )
    );

    assert!(
        !crate::dispatcher::instrument_search::trading_vehicle_matches_filters(
            &stock,
            &missing_terms
        )
    );

    let incomplete_bond = model::TradingVehicle {
        symbol: "9128285M8".to_string(),
        broker: "ibkr".to_string(),
        category: TradingVehicleCategory::Bond,
        fixed_income: None,
        ..Default::default()
    };
    assert!(
        crate::dispatcher::instrument_search::trading_vehicle_matches_filters(
            &incomplete_bond,
            &missing_terms
        )
    );
}

#[test]
fn test_trading_vehicle_search_json_payload_includes_fixed_income_terms() {
    let vehicle = model::TradingVehicle {
        symbol: "912810TL2".to_string(),
        broker: "ibkr".to_string(),
        category: TradingVehicleCategory::Bond,
        fixed_income: Some(FixedIncomeTerms {
            face_value: Some(dec!(1000)),
            annual_coupon_rate_pct: Some(dec!(4.625)),
            maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
            coupon_frequency_per_year: Some(2),
        }),
        ..Default::default()
    };
    let filters = TradingVehicleSearchFilters {
        category: Some(TradingVehicleCategory::Bond),
        broker: None,
        symbol: None,
        isin: None,
        missing_bond_terms: false,
        list_all: true,
        format: ReportOutputFormat::Json,
    };

    let payload =
        crate::dispatcher::instrument_search::trading_vehicle_search_payload(&[vehicle], &filters);

    assert_eq!(payload["report"], "trading_vehicle_search");
    assert_eq!(payload["count"], 1);
    assert_eq!(payload["data"][0]["category"], "bond");
    assert_eq!(
        payload["data"][0]["fixed_income"]["annual_coupon_rate_pct"],
        "4.625"
    );
    assert_eq!(
        payload["data"][0]["fixed_income"]["coupon_frequency_per_year"],
        2
    );
}

#[test]
fn test_trading_vehicle_search_dispatch_covers_interactive_text_json_and_invalid_filters() {
    let mut dispatcher = test_dispatcher();
    dispatcher
        .trust
        .create_trading_vehicle("AAPL", None, &TradingVehicleCategory::Stock, "alpaca")
        .expect("seed stock");
    dispatcher
        .trust
        .upsert_trading_vehicle(TradingVehicleUpsert {
            symbol: "912810TL2".to_string(),
            isin: Some("US912810TL26".to_string()),
            category: TradingVehicleCategory::Bond,
            broker: "ibkr".to_string(),
            broker_asset_id: None,
            exchange: None,
            broker_asset_class: None,
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
        .expect("seed first bond");
    dispatcher
        .trust
        .upsert_trading_vehicle(TradingVehicleUpsert {
            symbol: "912810TL2".to_string(),
            isin: Some("US912810TL27".to_string()),
            category: TradingVehicleCategory::Bond,
            broker: "alpaca".to_string(),
            broker_asset_id: None,
            exchange: None,
            broker_asset_class: None,
            broker_asset_status: None,
            tradable: None,
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
            fixed_income: Some(FixedIncomeTerms {
                face_value: Some(dec!(1000)),
                annual_coupon_rate_pct: Some(dec!(4.75)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2035, 5, 15).unwrap()),
                coupon_frequency_per_year: Some(2),
            }),
        })
        .expect("seed duplicate-symbol bond");

    crate::dialogs::scripted_reset();
    crate::dialogs::scripted_push_select(Ok(Some(0)));
    let interactive = TradingVehicleCommandBuilder::new()
        .search_trading_vehicle()
        .build()
        .get_matches_from(["trading-vehicle", "search"]);
    crate::dispatcher::instrument_search::search_trading_vehicle(
        &mut dispatcher.trust,
        interactive.subcommand_matches("search").unwrap(),
    )
    .expect("interactive search should display selected vehicle");

    let text_all = TradingVehicleCommandBuilder::new()
        .search_trading_vehicle()
        .build()
        .get_matches_from(["trading-vehicle", "search", "--all"]);
    crate::dispatcher::instrument_search::search_trading_vehicle(
        &mut dispatcher.trust,
        text_all.subcommand_matches("search").unwrap(),
    )
    .expect("text search should display filtered vehicles");

    let no_matches = TradingVehicleCommandBuilder::new()
        .search_trading_vehicle()
        .build()
        .get_matches_from(["trading-vehicle", "search", "--symbol", "MISSING"]);
    crate::dispatcher::instrument_search::search_trading_vehicle(
        &mut dispatcher.trust,
        no_matches.subcommand_matches("search").unwrap(),
    )
    .expect("empty text search should render no-match message");

    let json_filtered = TradingVehicleCommandBuilder::new()
        .search_trading_vehicle()
        .build()
        .get_matches_from([
            "trading-vehicle",
            "search",
            "--category",
            "bond",
            "--broker",
            " IBKR ",
            "--symbol",
            " 912 ",
            "--isin",
            " us912 ",
            "--format",
            "json",
        ]);
    crate::dispatcher::instrument_search::search_trading_vehicle(
        &mut dispatcher.trust,
        json_filtered.subcommand_matches("search").unwrap(),
    )
    .expect("json search should render filtered payload");

    let invalid_category = TradingVehicleCommandBuilder::new()
        .search_trading_vehicle()
        .build()
        .get_matches_from(["trading-vehicle", "search", "--category", "option"]);
    let error = crate::dispatcher::instrument_search::search_trading_vehicle(
        &mut dispatcher.trust,
        invalid_category.subcommand_matches("search").unwrap(),
    )
    .expect_err("invalid category should fail");
    assert!(error
        .to_string()
        .contains("invalid_trading_vehicle_category"));

    crate::dialogs::scripted_reset();
}

#[test]
fn test_category_to_str_maps_expected_values() {
    assert_eq!(
        crate::dispatcher::instrument_search::category_to_str(
            model::TradingVehicleCategory::Crypto
        ),
        "crypto"
    );
    assert_eq!(
        crate::dispatcher::instrument_search::category_to_str(model::TradingVehicleCategory::Fiat),
        "fiat"
    );
    assert_eq!(
        crate::dispatcher::instrument_search::category_to_str(model::TradingVehicleCategory::Stock),
        "stock"
    );
    assert_eq!(
        crate::dispatcher::instrument_search::category_to_str(model::TradingVehicleCategory::Etf),
        "etf"
    );
    assert_eq!(
        crate::dispatcher::instrument_search::category_to_str(model::TradingVehicleCategory::Bond),
        "bond"
    );
}
