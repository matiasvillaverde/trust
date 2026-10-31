#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::dispatcher::output::ReportOutputFormat;
use uuid::Uuid;

#[test]
fn test_reports_surface_missing_account_data_for_valid_unknown_account_id() {
    let missing_account = Uuid::new_v4().to_string();
    let matches = report_args_matches(&["--account", &missing_account]);

    let mut dispatcher = test_dispatcher();
    let risk_error = crate::dispatcher::risk_report::risk_report(
        &mut dispatcher.trust,
        &matches,
        ReportOutputFormat::Json,
    )
    .expect_err("missing account balance should fail risk report");
    assert!(risk_error
        .to_string()
        .contains("account_balance_unavailable"));

    let mut dispatcher = test_dispatcher();
    let summary_error = crate::dispatcher::summary_report::summary_report(
        &mut dispatcher.trust,
        &matches,
        ReportOutputFormat::Json,
    )
    .expect_err("missing account should fail summary report");
    assert!(summary_error
        .to_string()
        .contains("summary_generation_failed"));
}
