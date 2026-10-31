#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::TradingVehicleCommandBuilder;
use chrono::NaiveDate;
use clap::Arg;
use clap::Command;
use model::database::TradingVehicleUpsert;
use model::BrokerKind;
use model::Environment;
use model::FixedIncomeTerms;
use model::TradingVehicleCategory;
use rust_decimal_macros::dec;

#[test]
fn test_create_trading_vehicle_from_broker_validates_required_args_and_account_lookup() {
    let mut dispatcher = test_dispatcher();
    let missing_account = Command::new("test")
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("symbol").long("symbol"))
        .get_matches_from(["test", "--symbol", "AAPL"]);
    let error = crate::dispatcher::instruments::create_trading_vehicle_from_broker(
        &mut dispatcher.trust,
        &missing_account,
        BrokerKind::Alpaca,
    )
    .expect_err("missing account should fail");
    assert!(error.to_string().contains("broker_import_invalid_args"));
    assert!(error.to_string().contains("--account is required"));

    let missing_symbol = Command::new("test")
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("symbol").long("symbol"))
        .get_matches_from(["test", "--account", "paper-main"]);
    let error = crate::dispatcher::instruments::create_trading_vehicle_from_broker(
        &mut dispatcher.trust,
        &missing_symbol,
        BrokerKind::Alpaca,
    )
    .expect_err("missing symbol should fail");
    assert!(error.to_string().contains("broker_import_invalid_args"));
    assert!(error.to_string().contains("--symbol is required"));

    let unknown_account = Command::new("test")
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("symbol").long("symbol"))
        .get_matches_from(["test", "--account", "paper-main", "--symbol", "AAPL"]);
    let error = crate::dispatcher::instruments::create_trading_vehicle_from_broker(
        &mut dispatcher.trust,
        &unknown_account,
        BrokerKind::Alpaca,
    )
    .expect_err("unknown account should fail");
    assert!(error
        .to_string()
        .contains("broker_import_account_not_found"));

    let account = dispatcher
        .trust
        .create_account(
            "broker-import-account",
            "broker import",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("create account");
    let invalid_category = Command::new("test")
        .arg(Arg::new("account").long("account"))
        .arg(Arg::new("symbol").long("symbol"))
        .arg(Arg::new("category").long("category"))
        .get_matches_from([
            "test",
            "--account",
            &account.name,
            "--symbol",
            "AAPL",
            "--category",
            "option",
        ]);
    let error = crate::dispatcher::instruments::create_trading_vehicle_from_broker(
        &mut dispatcher.trust,
        &invalid_category,
        BrokerKind::Alpaca,
    )
    .expect_err("invalid category hint should fail before broker import");
    assert!(error
        .to_string()
        .contains("invalid_trading_vehicle_category"));
}

#[test]
fn test_create_trading_vehicle_dispatch_covers_interactive_and_invalid_broker_paths() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    crate::dialogs::scripted_reset();
    let mut dispatcher = test_dispatcher();

    let create_matches = |args: &[&str]| {
        let mut argv = vec!["trading-vehicle-create"];
        argv.extend_from_slice(args);
        Command::new("trading-vehicle-create")
            .arg(Arg::new("confirm-protected").long("confirm-protected"))
            .arg(
                Arg::new("from-alpaca")
                    .long("from-alpaca")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(Arg::new("from-broker").long("from-broker"))
            .arg(Arg::new("account").long("account"))
            .arg(Arg::new("symbol").long("symbol"))
            .arg(Arg::new("category").long("category"))
            .get_matches_from(argv)
    };

    let invalid_broker = create_matches(&["--from-broker", "bad", "--confirm-protected", "secret"]);
    let error = crate::dispatcher::instruments::create_trading_vehicle(
        &mut dispatcher.trust,
        &invalid_broker,
    )
    .expect_err("unsupported broker import should fail");
    assert!(error.to_string().contains("invalid_broker_kind"));

    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_input(Ok("DTVI".to_string()));
    crate::dialogs::scripted_push_input(Ok("paper-broker".to_string()));
    crate::dialogs::scripted_push_input(Ok("US0000000001".to_string()));
    let interactive = create_matches(&["--confirm-protected", "secret"]);
    crate::dispatcher::instruments::create_trading_vehicle(&mut dispatcher.trust, &interactive)
        .expect("scripted interactive trading vehicle creation should succeed");
    let created = dispatcher
        .trust
        .search_trading_vehicles()
        .expect("created vehicle should be searchable");
    assert!(created.iter().any(|vehicle| vehicle.symbol == "DTVI"));

    crate::dialogs::scripted_reset();
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_parse_fixed_income_terms_accepts_bond_metadata_only_for_bonds() {
    let cmd = TradingVehicleCommandBuilder::new()
        .create_trading_vehicle()
        .build();
    let matches = cmd
        .try_get_matches_from([
            "trading-vehicle",
            "create",
            "--from-broker",
            "ibkr",
            "--account",
            "paper-main",
            "--symbol",
            "9128285M8",
            "--category",
            "bond",
            "--face-value",
            "1000",
            "--coupon-rate",
            "4.625",
            "--maturity-date",
            "2034-05-15",
            "--coupon-frequency",
            "2",
        ])
        .unwrap();
    let create = matches.subcommand_matches("create").unwrap();

    let terms = crate::dispatcher::instruments::parse_fixed_income_terms(
        create,
        TradingVehicleCategory::Bond,
    )
    .unwrap();
    let terms = terms.unwrap();
    assert_eq!(terms.face_value, Some(dec!(1000)));
    assert_eq!(terms.annual_coupon_rate_pct, Some(dec!(4.625)));
    assert_eq!(
        terms.maturity_date,
        Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap())
    );
    assert_eq!(terms.coupon_frequency_per_year, Some(2));

    let error = crate::dispatcher::instruments::parse_fixed_income_terms(
        create,
        TradingVehicleCategory::Etf,
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid_fixed_income_terms"));
}

#[test]
fn test_store_imported_trading_vehicle_applies_bond_terms_and_surfaces_store_errors() {
    let mut dispatcher = test_dispatcher();
    let cmd = TradingVehicleCommandBuilder::new()
        .create_trading_vehicle()
        .build();
    let matches = cmd
        .try_get_matches_from([
            "trading-vehicle",
            "create",
            "--from-broker",
            "ibkr",
            "--account",
            "paper-main",
            "--symbol",
            "9128285M8",
            "--category",
            "bond",
            "--face-value",
            "1000",
            "--coupon-rate",
            "4.625",
            "--maturity-date",
            "2034-05-15",
            "--coupon-frequency",
            "2",
        ])
        .expect("broker import matches should parse");
    let create = matches.subcommand_matches("create").unwrap();

    let imported = crate::trading_vehicle_import::ImportedTradingVehicle {
        upsert: TradingVehicleUpsert {
            symbol: "9128285M8".to_string(),
            isin: None,
            category: TradingVehicleCategory::Bond,
            broker: "ibkr".to_string(),
            broker_asset_id: Some("bond-1".to_string()),
            exchange: Some("SMART".to_string()),
            broker_asset_class: Some("bond".to_string()),
            broker_asset_status: None,
            tradable: None,
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
            fixed_income: None,
        },
        summary: "Imported from test".to_string(),
    };

    crate::dispatcher::instruments::store_imported_trading_vehicle(
        &mut dispatcher.trust,
        create,
        imported,
    )
    .expect("imported bond should be stored");
    let stored = dispatcher
        .trust
        .search_trading_vehicles()
        .expect("stored vehicle should be searchable")
        .into_iter()
        .find(|vehicle| vehicle.symbol == "9128285M8")
        .expect("stored imported bond should exist");
    let terms = stored.fixed_income.expect("bond terms should be attached");
    assert_eq!(terms.face_value, Some(dec!(1000)));
    assert_eq!(terms.annual_coupon_rate_pct, Some(dec!(4.625)));

    let mut failing_dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::trading_vehicle_write(),
    );
    let failing_import = crate::trading_vehicle_import::ImportedTradingVehicle {
        upsert: TradingVehicleUpsert {
            symbol: "FAILBOND".to_string(),
            isin: None,
            category: TradingVehicleCategory::Bond,
            broker: "ibkr".to_string(),
            broker_asset_id: Some("fail-bond".to_string()),
            exchange: Some("SMART".to_string()),
            broker_asset_class: Some("bond".to_string()),
            broker_asset_status: None,
            tradable: None,
            marginable: None,
            shortable: None,
            easy_to_borrow: None,
            fractionable: None,
            fixed_income: None,
        },
        summary: "Imported from failing writer".to_string(),
    };
    let error = crate::dispatcher::instruments::store_imported_trading_vehicle(
        &mut failing_dispatcher.trust,
        create,
        failing_import,
    )
    .expect_err("writer failure should surface as broker import store failure");
    assert_eq!(error.code, "broker_import_store_failed");
}

#[test]
fn test_parse_fixed_income_terms_rejects_invalid_bond_metadata_inputs() {
    let parse_create = |args: &[&str]| {
        let mut argv = vec!["trading-vehicle", "create", "--category", "bond"];
        argv.extend_from_slice(args);
        TradingVehicleCommandBuilder::new()
            .create_trading_vehicle()
            .build()
            .try_get_matches_from(argv)
            .expect("bond metadata args should parse")
    };

    let invalid_decimal = parse_create(&["--coupon-rate", "not-a-decimal"]);
    let error = crate::dispatcher::instruments::parse_fixed_income_terms(
        invalid_decimal.subcommand_matches("create").unwrap(),
        TradingVehicleCategory::Bond,
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid_decimal"));
    assert!(error.to_string().contains("--coupon-rate"));

    let invalid_face_value = parse_create(&["--face-value", "not-a-decimal"]);
    let error = crate::dispatcher::instruments::parse_fixed_income_terms(
        invalid_face_value.subcommand_matches("create").unwrap(),
        TradingVehicleCategory::Bond,
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid_decimal"));
    assert!(error.to_string().contains("--face-value"));

    let invalid_maturity = parse_create(&["--maturity-date", "05/15/2034"]);
    let error = crate::dispatcher::instruments::parse_fixed_income_terms(
        invalid_maturity.subcommand_matches("create").unwrap(),
        TradingVehicleCategory::Bond,
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid_maturity_date"));

    let zero_frequency = parse_create(&["--coupon-frequency", "0"]);
    let error = crate::dispatcher::instruments::parse_fixed_income_terms(
        zero_frequency.subcommand_matches("create").unwrap(),
        TradingVehicleCategory::Bond,
    )
    .unwrap_err();
    assert!(error.to_string().contains("invalid_coupon_frequency"));

    let no_terms = parse_create(&[]);
    let parsed = crate::dispatcher::instruments::parse_fixed_income_terms(
        no_terms.subcommand_matches("create").unwrap(),
        TradingVehicleCategory::Bond,
    )
    .unwrap();
    assert!(parsed.is_none());
}

#[test]
fn test_update_bond_terms_preserves_existing_metadata_and_terms() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();
    dispatcher
        .trust
        .upsert_trading_vehicle(TradingVehicleUpsert {
            symbol: "9128285M8".to_string(),
            isin: Some("US9128285M81".to_string()),
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
                annual_coupon_rate_pct: Some(dec!(4.0)),
                maturity_date: Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap()),
                coupon_frequency_per_year: Some(2),
            }),
        })
        .unwrap();

    let matches = TradingVehicleCommandBuilder::new()
        .update_bond_terms()
        .build()
        .get_matches_from([
            "trading-vehicle",
            "update-bond-terms",
            "--symbol",
            "9128285M8",
            "--broker",
            "ibkr",
            "--coupon-rate",
            "4.625",
            "--confirm-protected",
            "secret",
        ]);
    let update = matches.subcommand_matches("update-bond-terms").unwrap();
    crate::dispatcher::instruments::update_bond_terms(&mut dispatcher.trust, update).unwrap();

    let updated = dispatcher
        .trust
        .search_trading_vehicles()
        .unwrap()
        .into_iter()
        .find(|vehicle| vehicle.symbol == "9128285M8")
        .unwrap();
    let terms = updated.fixed_income.unwrap();
    assert_eq!(updated.broker_asset_id.as_deref(), Some("123456"));
    assert_eq!(terms.face_value, Some(dec!(1000)));
    assert_eq!(terms.annual_coupon_rate_pct, Some(dec!(4.625)));
    assert_eq!(
        terms.maturity_date,
        Some(NaiveDate::from_ymd_opt(2034, 5, 15).unwrap())
    );
    assert_eq!(terms.coupon_frequency_per_year, Some(2));
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_update_bond_terms_validates_missing_not_found_and_wrong_category() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();
    dispatcher
        .trust
        .upsert_trading_vehicle(TradingVehicleUpsert {
            symbol: "AAPL".to_string(),
            isin: Some("US0378331005".to_string()),
            category: TradingVehicleCategory::Stock,
            broker: "alpaca".to_string(),
            broker_asset_id: None,
            exchange: Some("NASDAQ".to_string()),
            broker_asset_class: Some("us_equity".to_string()),
            broker_asset_status: Some("active".to_string()),
            tradable: Some(true),
            marginable: Some(true),
            shortable: Some(true),
            easy_to_borrow: Some(true),
            fractionable: Some(true),
            fixed_income: None,
        })
        .expect("stock vehicle");

    let missing_terms = TradingVehicleCommandBuilder::new()
        .update_bond_terms()
        .build()
        .get_matches_from([
            "trading-vehicle",
            "update-bond-terms",
            "--symbol",
            "AAPL",
            "--broker",
            "alpaca",
            "--confirm-protected",
            "secret",
        ]);
    let error = crate::dispatcher::instruments::update_bond_terms(
        &mut dispatcher.trust,
        missing_terms
            .subcommand_matches("update-bond-terms")
            .unwrap(),
    )
    .expect_err("missing terms should fail");
    assert!(error.to_string().contains("missing_bond_terms"));

    let missing_symbol = Command::new("update-bond-terms")
        .arg(Arg::new("symbol").long("symbol"))
        .arg(Arg::new("broker").long("broker"))
        .arg(Arg::new("face-value").long("face-value"))
        .arg(Arg::new("confirm-protected").long("confirm-protected"))
        .get_matches_from([
            "update-bond-terms",
            "--broker",
            "alpaca",
            "--face-value",
            "1000",
            "--confirm-protected",
            "secret",
        ]);
    let error =
        crate::dispatcher::instruments::update_bond_terms(&mut dispatcher.trust, &missing_symbol)
            .expect_err("missing symbol should fail");
    assert!(error.to_string().contains("missing_argument"));

    let not_found = TradingVehicleCommandBuilder::new()
        .update_bond_terms()
        .build()
        .get_matches_from([
            "trading-vehicle",
            "update-bond-terms",
            "--symbol",
            "MISSING",
            "--broker",
            "alpaca",
            "--face-value",
            "1000",
            "--confirm-protected",
            "secret",
        ]);
    let error = crate::dispatcher::instruments::update_bond_terms(
        &mut dispatcher.trust,
        not_found.subcommand_matches("update-bond-terms").unwrap(),
    )
    .expect_err("missing vehicle should fail");
    assert!(error.to_string().contains("trading_vehicle_not_found"));

    let wrong_category = TradingVehicleCommandBuilder::new()
        .update_bond_terms()
        .build()
        .get_matches_from([
            "trading-vehicle",
            "update-bond-terms",
            "--symbol",
            "AAPL",
            "--broker",
            "alpaca",
            "--face-value",
            "1000",
            "--confirm-protected",
            "secret",
        ]);
    let error = crate::dispatcher::instruments::update_bond_terms(
        &mut dispatcher.trust,
        wrong_category
            .subcommand_matches("update-bond-terms")
            .unwrap(),
    )
    .expect_err("stock vehicle should fail bond update");
    assert!(error.to_string().contains("bond_vehicle_wrong_category"));

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}
