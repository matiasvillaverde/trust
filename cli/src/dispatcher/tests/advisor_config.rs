#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::broker::*;
use super::fixtures::*;

#[test]
fn test_advisor_commands_validate_inputs_and_history_defaults() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher = test_dispatcher();
    let (account, _) = seed_account_and_vehicle(&mut dispatcher);

    let bad_uuid = advisor_configure_matches(&[
        "--confirm-protected",
        "secret",
        "--account",
        "bad",
        "--sector-limit",
        "40",
        "--asset-class-limit",
        "40",
        "--single-position-limit",
        "25",
    ]);
    let error =
        crate::dispatcher::advisor_config::advisor_configure(&mut dispatcher.trust, &bad_uuid)
            .expect_err("invalid uuid should fail");
    assert!(error.to_string().contains("invalid_uuid"));

    let bad_decimal = advisor_configure_matches(&[
        "--confirm-protected",
        "secret",
        "--account",
        &account.id.to_string(),
        "--sector-limit",
        "bad",
        "--asset-class-limit",
        "40",
        "--single-position-limit",
        "25",
    ]);
    let error =
        crate::dispatcher::advisor_config::advisor_configure(&mut dispatcher.trust, &bad_decimal)
            .expect_err("invalid decimal should fail");
    assert!(error.to_string().contains("invalid_decimal"));

    let bad_provider = advisor_configure_matches(&["--calendar-provider", "bad"]);
    let error =
        crate::dispatcher::advisor_config::advisor_configure(&mut dispatcher.trust, &bad_provider)
            .expect_err("invalid provider should fail");
    assert!(error.to_string().contains("invalid_calendar_provider"));

    let mixed_modes = advisor_configure_matches(&[
        "--calendar-api-key",
        "secret",
        "--account",
        &account.id.to_string(),
        "--sector-limit",
        "40",
        "--asset-class-limit",
        "40",
        "--single-position-limit",
        "25",
    ]);
    let error =
        crate::dispatcher::advisor_config::advisor_configure(&mut dispatcher.trust, &mixed_modes)
            .expect_err("mixed config modes should fail");
    assert!(error
        .to_string()
        .contains("advisor_configure_mode_conflict"));

    let check_bad_decimal = advisor_check_matches(&[
        "--account",
        &account.id.to_string(),
        "--symbol",
        "AAPL",
        "--entry",
        "bad",
        "--quantity",
        "10",
    ]);
    let error =
        crate::dispatcher::advisor_config::advisor_check(&mut dispatcher.trust, &check_bad_decimal)
            .expect_err("invalid entry should fail");
    assert!(error.to_string().contains("invalid_decimal"));

    let status_bad_uuid = advisor_check_matches(&["--account", "bad"]);
    let error =
        crate::dispatcher::advisor_config::advisor_status(&mut dispatcher.trust, &status_bad_uuid)
            .expect_err("invalid status uuid should fail");
    assert!(error.to_string().contains("invalid_uuid"));

    let history_default_days = advisor_history_matches(&["--account", &account.id.to_string()]);
    crate::dispatcher::advisor_config::advisor_history(
        &mut dispatcher.trust,
        &history_default_days,
    )
    .expect("history should support default days when no entries exist");

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}

#[test]
fn test_advisor_success_paths_emit_warnings_status_and_history() {
    let _guard = env_guard();
    std::env::set_var("TRUST_PROTECTED_KEYWORD_EXPECTED", "secret");
    let mut dispatcher =
        test_dispatcher_with_broker(Box::new(StubBroker::submitted_fill_fixture()));
    let (account, _) = seed_filled_trade(&mut dispatcher);
    let account_id = account.id.to_string();

    let configure = advisor_configure_matches(&[
        "--confirm-protected",
        "secret",
        "--account",
        &account_id,
        "--sector-limit",
        "50",
        "--asset-class-limit",
        "50",
        "--single-position-limit",
        "50",
    ]);
    crate::dispatcher::advisor_config::advisor_configure(&mut dispatcher.trust, &configure)
        .expect("advisor thresholds should configure");

    let check = advisor_check_matches(&[
        "--account",
        &account_id,
        "--symbol",
        "MSFT",
        "--entry",
        "100",
        "--quantity",
        "2",
        "--sector",
        "technology",
        "--asset-class",
        "equity",
    ]);
    crate::dispatcher::advisor_config::advisor_check(&mut dispatcher.trust, &check)
        .expect("concentrated proposal should produce advisory output");

    let history = dispatcher
        .trust
        .advisory_history_for_account(account.id, 30);
    assert_eq!(history.len(), 1);
    assert!(history[0].summary.contains("sector concentration"));

    let status = advisor_check_matches(&["--account", &account_id]);
    crate::dispatcher::advisor_config::advisor_status(&mut dispatcher.trust, &status)
        .expect("filled concentrated portfolio should render status warnings");

    let history_matches = advisor_history_matches(&["--account", &account_id, "--days", "30"]);
    crate::dispatcher::advisor_config::advisor_history(&mut dispatcher.trust, &history_matches)
        .expect("non-empty advisor history should render entries");

    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
}
