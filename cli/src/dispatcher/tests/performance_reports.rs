#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::commands::GradeCommandBuilder;
use crate::commands::MetricsCommandBuilder;
use crate::dispatcher::output::ReportOutputFormat;
use clap::Arg;
use clap::Command;
use model::Currency;
use model::Environment;
use model::Status;
use rust_decimal_macros::dec;
use uuid::Uuid;

#[test]
fn test_reporting_methods_execute_text_paths_with_account_scope() {
    let mut dispatcher = test_dispatcher();
    let (account, vehicle) = seed_account_and_vehicle(&mut dispatcher);
    dispatcher
        .trust
        .create_trade(
            model::DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity: 10.into(),
                category: model::TradeCategory::Long,
                currency: Currency::USD,
                thesis: None,
                sector: Some("tech".to_string()),
                asset_class: Some("equity".to_string()),
                context: None,
            },
            dec!(95),
            dec!(100),
            dec!(110),
        )
        .expect("seed trade creation should succeed");

    let account_only = report_args_matches(&["--account", &account.id.to_string()]);
    let perf_with_days =
        report_args_matches(&["--account", &account.id.to_string(), "--days", "30"]);
    let concentration = report_args_matches(&["--account", &account.id.to_string()]);

    crate::dispatcher::performance_reports::drawdown_report(
        &mut dispatcher.trust,
        &account_only,
        ReportOutputFormat::Text,
    )
    .expect("drawdown text report should succeed");
    crate::dispatcher::performance_reports::performance_report(
        &mut dispatcher.trust,
        &perf_with_days,
        ReportOutputFormat::Text,
    )
    .expect("performance text report should succeed");
    crate::dispatcher::risk_report::risk_report(
        &mut dispatcher.trust,
        &account_only,
        ReportOutputFormat::Text,
    )
    .expect("risk text report should succeed");
    crate::dispatcher::concentration_report::concentration_report(
        &mut dispatcher.trust,
        &concentration,
        ReportOutputFormat::Text,
    )
    .expect("concentration text report should succeed");
    crate::dispatcher::summary_report::summary_report(
        &mut dispatcher.trust,
        &account_only,
        ReportOutputFormat::Text,
    )
    .expect("summary text report should succeed");
}

#[test]
fn test_reporting_methods_execute_aggregate_scope_paths_without_account_argument() {
    let mut dispatcher = test_dispatcher();
    let (_account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);
    let no_account = report_args_matches(&[]);
    let no_account_with_days = report_args_matches(&["--days", "7"]);
    let open_only = report_args_matches(&["--open-only"]);

    crate::dispatcher::performance_reports::drawdown_report(
        &mut dispatcher.trust,
        &no_account,
        ReportOutputFormat::Text,
    )
    .expect("drawdown aggregate report should succeed");
    crate::dispatcher::performance_reports::performance_report(
        &mut dispatcher.trust,
        &no_account_with_days,
        ReportOutputFormat::Text,
    )
    .expect("performance aggregate report should succeed");
    crate::dispatcher::risk_report::risk_report(
        &mut dispatcher.trust,
        &no_account,
        ReportOutputFormat::Text,
    )
    .expect("risk aggregate report should succeed");
    crate::dispatcher::concentration_report::concentration_report(
        &mut dispatcher.trust,
        &open_only,
        ReportOutputFormat::Text,
    )
    .expect("concentration open-only aggregate report should succeed");
    crate::dispatcher::summary_report::summary_report(
        &mut dispatcher.trust,
        &no_account,
        ReportOutputFormat::Text,
    )
    .expect("summary aggregate report should succeed");
}

#[test]
fn test_reports_reject_invalid_account_id_before_data_access() {
    let invalid_matches = Command::new("test")
        .arg(Arg::new("account").long("account").required(false))
        .arg(Arg::new("open-only").long("open-only"))
        .arg(
            Arg::new("days")
                .long("days")
                .value_parser(clap::value_parser!(u32)),
        )
        .get_matches_from(["test", "--account", "not-a-uuid"]);

    let mut dispatcher = test_dispatcher();
    let drawdown_error = crate::dispatcher::performance_reports::drawdown_report(
        &mut dispatcher.trust,
        &invalid_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid account id should fail");
    assert!(drawdown_error.to_string().contains("invalid_account_id"));

    let mut dispatcher = test_dispatcher();
    let performance_error = crate::dispatcher::performance_reports::performance_report(
        &mut dispatcher.trust,
        &invalid_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid account id should fail");
    assert!(performance_error.to_string().contains("invalid_account_id"));

    let mut dispatcher = test_dispatcher();
    let risk_error = crate::dispatcher::risk_report::risk_report(
        &mut dispatcher.trust,
        &invalid_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid account id should fail");
    assert!(risk_error.to_string().contains("invalid_account_id"));

    let mut dispatcher = test_dispatcher();
    let concentration_error = crate::dispatcher::concentration_report::concentration_report(
        &mut dispatcher.trust,
        &invalid_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid account id should fail");
    assert!(concentration_error
        .to_string()
        .contains("invalid_account_id"));

    let mut dispatcher = test_dispatcher();
    let summary_error = crate::dispatcher::summary_report::summary_report(
        &mut dispatcher.trust,
        &invalid_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("invalid account id should fail");
    assert!(summary_error.to_string().contains("invalid_account_id"));
}

#[test]
fn test_reporting_policy_and_onboarding_handlers_cover_text_and_json_paths() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();
    let (account, vehicle) = seed_account_and_vehicle(&mut dispatcher);
    let trade = dispatcher
        .trust
        .create_trade(
            model::DraftTrade {
                account: account.clone(),
                trading_vehicle: vehicle,
                quantity: 1.into(),
                category: model::TradeCategory::Long,
                currency: Currency::USD,
                thesis: Some("t".to_string()),
                sector: Some("Tech".to_string()),
                asset_class: Some("Stocks".to_string()),
                context: None,
            },
            dec!(90),
            dec!(100),
            dec!(120),
        )
        .expect("seed trade");
    let _ = dispatcher.trust.fund_trade(&trade).expect("seed funding");

    let report_matches = Command::new("report")
        .arg(Arg::new("account").long("account").required(false))
        .arg(
            Arg::new("days")
                .long("days")
                .required(false)
                .value_parser(clap::value_parser!(u32)),
        )
        .arg(
            Arg::new("open-only")
                .long("open-only")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches_from([
            "report",
            "--account",
            &account.id.to_string(),
            "--days",
            "30",
        ]);

    assert!(crate::dispatcher::performance_reports::drawdown_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Text
    )
    .is_ok());
    assert!(crate::dispatcher::performance_reports::drawdown_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Json
    )
    .is_ok());
    assert!(crate::dispatcher::performance_reports::performance_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Text
    )
    .is_ok());
    assert!(crate::dispatcher::performance_reports::performance_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Json
    )
    .is_ok());
    assert!(crate::dispatcher::risk_report::risk_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Text
    )
    .is_ok());
    assert!(crate::dispatcher::risk_report::risk_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Json
    )
    .is_ok());
    assert!(
        crate::dispatcher::concentration_report::concentration_report(
            &mut dispatcher.trust,
            &report_matches,
            ReportOutputFormat::Text
        )
        .is_ok()
    );
    assert!(
        crate::dispatcher::concentration_report::concentration_report(
            &mut dispatcher.trust,
            &report_matches,
            ReportOutputFormat::Json
        )
        .is_ok()
    );
    assert!(crate::dispatcher::summary_report::summary_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Text
    )
    .is_ok());
    assert!(crate::dispatcher::summary_report::summary_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Json
    )
    .is_ok());
    assert!(crate::dispatcher::metrics_report::metrics_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Text
    )
    .is_ok());
    assert!(crate::dispatcher::metrics_report::metrics_report(
        &mut dispatcher.trust,
        &report_matches,
        ReportOutputFormat::Json
    )
    .is_ok());

    assert!(crate::dispatcher::policy::policy_show(
        &mut dispatcher.trust,
        ReportOutputFormat::Text
    )
    .is_ok());
    assert!(crate::dispatcher::policy::policy_show(
        &mut dispatcher.trust,
        ReportOutputFormat::Json
    )
    .is_ok());
    assert!(crate::dispatcher::onboarding::onboarding_status(
        &mut dispatcher.trust,
        ReportOutputFormat::Text
    )
    .is_ok());
    assert!(crate::dispatcher::onboarding::onboarding_status(
        &mut dispatcher.trust,
        ReportOutputFormat::Json
    )
    .is_ok());

    let onboarding_matches = Command::new("onboarding")
        .arg(
            Arg::new("protected-keyword")
                .long("protected-keyword")
                .required(false),
        )
        .arg(
            Arg::new("overwrite")
                .long("overwrite")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches_from(["onboarding", "--protected-keyword", "secret", "--overwrite"]);
    assert!(crate::dispatcher::onboarding::onboarding_init(
        &mut dispatcher.trust,
        &onboarding_matches,
        ReportOutputFormat::Json
    )
    .is_err());

    let grade_summary_matches = Command::new("grade")
        .arg(Arg::new("account").long("account").required(false))
        .arg(
            Arg::new("days")
                .long("days")
                .required(false)
                .value_parser(clap::value_parser!(u32)),
        )
        .arg(Arg::new("weights").long("weights").required(false))
        .get_matches_from([
            "grade",
            "--account",
            &account.id.to_string(),
            "--days",
            "30",
        ]);
    assert!(crate::dispatcher::grades::grade_summary(
        &mut dispatcher.trust,
        &grade_summary_matches,
        ReportOutputFormat::Json
    )
    .is_ok());

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_report_handlers_surface_read_failures() {
    let account_id = Uuid::new_v4().to_string();
    let account_matches = report_args_matches(&["--account", &account_id]);
    let no_account_matches = report_args_matches(&[]);

    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::transactions());
    let error = crate::dispatcher::performance_reports::drawdown_report(
        &mut dispatcher.trust,
        &account_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("drawdown should surface account transaction read failures");
    assert_eq!(error.code, "transaction_data_unavailable");

    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
    let error = crate::dispatcher::performance_reports::drawdown_report(
        &mut dispatcher.trust,
        &no_account_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("drawdown should surface account list failures");
    assert_eq!(error.code, "transaction_data_unavailable");

    let mut dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::drawdown_equity_overflow(),
    );
    let error = crate::dispatcher::performance_reports::drawdown_report(
        &mut dispatcher.trust,
        &account_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("drawdown should surface equity curve calculation failures");
    assert_eq!(error.code, "drawdown_equity_curve_failed");

    let mut dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::drawdown_metrics_overflow(),
    );
    let error = crate::dispatcher::performance_reports::drawdown_report(
        &mut dispatcher.trust,
        &account_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("drawdown should surface drawdown metric calculation failures");
    assert_eq!(error.code, "drawdown_calculation_failed");

    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
    let error = crate::dispatcher::performance_reports::performance_report(
        &mut dispatcher.trust,
        &no_account_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("performance should surface account list failures");
    assert_eq!(error.code, "trading_data_unavailable");

    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
    let error = crate::dispatcher::summary_report::summary_report(
        &mut dispatcher.trust,
        &no_account_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("summary should surface account list failures");
    assert_eq!(error.code, "summary_generation_failed");

    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
    let error = crate::dispatcher::metrics_report::metrics_report(
        &mut dispatcher.trust,
        &no_account_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("metrics should surface account list failures");
    assert_eq!(error.code, "trading_data_unavailable");

    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
    let error = crate::dispatcher::risk_report::risk_report(
        &mut dispatcher.trust,
        &no_account_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("risk should surface open-position read failures");
    assert_eq!(error.code, "open_positions_failed");

    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
    let error = crate::dispatcher::concentration_report::concentration_report(
        &mut dispatcher.trust,
        &no_account_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("concentration should surface account list failures");
    assert_eq!(error.code, "accounts_fetch_failed");
    let open_trades =
        crate::dispatcher::report_calculations::open_trades_for_scope(&mut dispatcher.trust, None);
    assert!(open_trades.is_empty());

    let mut dispatcher = test_dispatcher();
    let account = dispatcher
        .trust
        .create_account(
            "missing-balance",
            "test",
            Environment::Paper,
            dec!(20),
            dec!(10),
        )
        .expect("account without balance");
    let account_matches = report_args_matches(&["--account", &account.id.to_string()]);
    let error = crate::dispatcher::risk_report::risk_report(
        &mut dispatcher.trust,
        &account_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("risk should surface missing account balance");
    assert_eq!(error.code, "account_balance_unavailable");

    let mut dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::capital_at_risk_overflow(),
    );
    let error = crate::dispatcher::risk_report::risk_report(
        &mut dispatcher.trust,
        &account_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("risk should surface capital-at-risk overflow");
    assert_eq!(error.code, "capital_at_risk_failed");

    let mut dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::open_positions_then_accounts_fail(),
    );
    crate::dispatcher::risk_report::risk_report(
        &mut dispatcher.trust,
        &no_account_matches,
        ReportOutputFormat::Json,
    )
    .expect("aggregate risk should fall back to zero equity when account totals fail");

    let grade_summary = GradeCommandBuilder::new()
        .summary()
        .build()
        .get_matches_from(["grade", "summary"]);
    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
    let error = crate::dispatcher::grades::grade_summary(
        &mut dispatcher.trust,
        grade_summary
            .subcommand_matches("summary")
            .expect("summary subcommand"),
        ReportOutputFormat::Json,
    )
    .expect_err("grade summary should surface closed-trade read failures");
    assert_eq!(error.code, "trading_data_unavailable");

    let grade_show = GradeCommandBuilder::new().show().build().get_matches_from([
        "grade",
        "show",
        &Uuid::new_v4().to_string(),
    ]);
    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::trade_grades());
    let error = crate::dispatcher::grades::grade_show(
        &mut dispatcher.trust,
        grade_show
            .subcommand_matches("show")
            .expect("show subcommand"),
        ReportOutputFormat::Json,
    )
    .expect_err("grade show should surface grade read failures");
    assert_eq!(error.code, "grade_read_failed");

    let mut dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::trades_after_account(),
    );
    let error = crate::dispatcher::context::find_trade_by_id(
        &mut dispatcher.trust,
        Uuid::new_v4(),
        &[Status::Submitted],
        ReportOutputFormat::Json,
    )
    .expect_err("trade lookup should continue after per-status read failures");
    assert_eq!(error.code, "trade_not_found");

    let advanced = MetricsCommandBuilder::new()
        .advanced()
        .build()
        .get_matches_from(["metrics", "advanced"]);
    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
    crate::dispatcher::advanced_metrics::metrics_advanced(
        &mut dispatcher.trust,
        advanced
            .subcommand_matches("advanced")
            .expect("advanced subcommand"),
    );

    let compare = MetricsCommandBuilder::new()
        .compare()
        .build()
        .get_matches_from([
            "metrics",
            "compare",
            "--period1",
            "all",
            "--period2",
            "last-7-days",
        ]);
    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
    crate::dispatcher::advanced_metrics::metrics_compare(
        &mut dispatcher.trust,
        compare
            .subcommand_matches("compare")
            .expect("compare subcommand"),
    );

    let bond = MetricsCommandBuilder::new()
        .bond()
        .build()
        .get_matches_from([
            "metrics",
            "bond",
            "--symbol",
            "BOND",
            "--broker",
            "ibkr",
            "--market-price",
            "95",
            "--quantity",
            "1",
        ]);
    let mut dispatcher = test_dispatcher_with_read_failures(
        crate::test_support::ReadFailureFactory::trading_vehicles(),
    );
    let error = crate::dispatcher::bond_metrics::metrics_bond(
        &mut dispatcher.trust,
        bond.subcommand_matches("bond").expect("bond subcommand"),
    )
    .expect_err("bond lookup should surface trading-vehicle read failures");
    assert_eq!(error.code, "bond_vehicle_lookup_failed");
}
