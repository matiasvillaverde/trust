#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::dispatcher::output::ReportOutputFormat;
use chrono::Utc;
use model::Currency;
use model::SessionRegime;
use rust_decimal_macros::dec;

#[test]
fn test_session_open_interactive_persists_plan_and_blocks_duplicate_open() {
    let mut dispatcher = test_dispatcher();
    let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

    crate::dialogs::scripted_reset();
    crate::dialogs::scripted_push_select(Ok(Some(1)));
    crate::dialogs::scripted_push_input(Ok("breakout, pullback".to_string()));
    crate::dialogs::scripted_push_input(Ok("2".to_string()));
    crate::dialogs::scripted_push_input(Ok("Wait for A-grade continuation setups".to_string()));
    crate::dialogs::scripted_push_input(Ok("Take one clean setup and honor stops".to_string()));
    crate::dialogs::scripted_push_input(Ok("Chase a low-quality trade".to_string()));

    crate::dispatcher::sessions::session_open(
        &mut dispatcher.trust,
        &session_matches(&["--account", &account.name]),
        ReportOutputFormat::Text,
    )
    .expect("session open should persist plan");

    let open = dispatcher
        .trust
        .open_session_for_account(account.id)
        .expect("open session lookup should succeed")
        .expect("session should be open");
    assert_eq!(open.regime, SessionRegime::Normal);
    assert_eq!(open.permitted_setups, vec!["breakout", "pullback"]);
    assert_eq!(open.max_positions, 2);
    assert!(open.closed_at.is_none());

    let duplicate = crate::dispatcher::sessions::session_open(
        &mut dispatcher.trust,
        &session_matches(&["--account", &account.name]),
        ReportOutputFormat::Text,
    )
    .expect_err("duplicate open session should fail before dialog");
    assert_eq!(duplicate.code, "session_already_open");

    crate::dialogs::scripted_reset();
}

#[test]
fn test_session_close_reviews_unplanned_trades_and_list_formats() {
    let mut dispatcher = test_dispatcher();
    let (account, vehicle) = seed_account_and_vehicle(&mut dispatcher);

    crate::dialogs::scripted_reset();
    crate::dialogs::scripted_push_select(Ok(Some(1)));
    crate::dialogs::scripted_push_input(Ok("breakout, pullback".to_string()));
    crate::dialogs::scripted_push_input(Ok("1".to_string()));
    crate::dialogs::scripted_push_input(Ok("Only take planned setups".to_string()));
    crate::dialogs::scripted_push_input(Ok("One A setup maximum".to_string()));
    crate::dialogs::scripted_push_input(Ok("Any unplanned setup".to_string()));
    crate::dispatcher::sessions::session_open(
        &mut dispatcher.trust,
        &session_matches(&["--account", &account.name]),
        ReportOutputFormat::Text,
    )
    .expect("session should open");

    for setup in ["breakout", "mean reversion"] {
        dispatcher
            .trust
            .create_trade(
                model::DraftTrade {
                    account: account.clone(),
                    trading_vehicle: vehicle.clone(),
                    quantity: 1.into(),
                    category: model::TradeCategory::Long,
                    currency: Currency::USD,
                    thesis: Some(format!("{setup} thesis")),
                    sector: Some("technology".to_string()),
                    asset_class: Some("equity".to_string()),
                    context: Some(setup.to_string()),
                },
                dec!(95),
                dec!(100),
                dec!(110),
            )
            .expect("session trade should create");
    }

    let open = dispatcher
        .trust
        .open_session_for_account(account.id)
        .expect("open session lookup")
        .expect("open session");
    let trades = dispatcher
        .trust
        .trades_for_account_in_period(account.id, open.opened_at, Utc::now().naive_utc())
        .expect("session trades should load");
    let rows = crate::dispatcher::sessions::session_trade_review_rows(&open, &trades);
    assert_eq!(rows.iter().filter(|row| row.planned).count(), 1);
    assert_eq!(rows.iter().filter(|row| !row.planned).count(), 1);

    crate::dialogs::scripted_push_select(Ok(Some(0)));
    crate::dialogs::scripted_push_input(Ok("One unplanned trade slipped in.".to_string()));
    crate::dispatcher::sessions::session_close(
        &mut dispatcher.trust,
        &session_matches(&[]),
        ReportOutputFormat::Text,
    )
    .expect("session close should persist review");

    assert!(dispatcher
        .trust
        .open_session_for_account(account.id)
        .expect("open session lookup after close")
        .is_none());
    let sessions = dispatcher
        .trust
        .session_plans_for_account(
            account.id,
            chrono::NaiveDateTime::MIN,
            Utc::now().naive_utc(),
        )
        .expect("session history should load");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_grade.as_deref(), Some("A"));
    assert_eq!(
        sessions[0].adherence_notes.as_deref(),
        Some("One unplanned trade slipped in.")
    );

    crate::dispatcher::sessions::session_list(
        &mut dispatcher.trust,
        &session_matches(&["--account", &account.name, "--format", "json"]),
        ReportOutputFormat::Json,
    )
    .expect("json session list should render");
    crate::dispatcher::sessions::session_list(
        &mut dispatcher.trust,
        &session_matches(&["--account", &account.name, "--format", "human"]),
        ReportOutputFormat::Text,
    )
    .expect("human session list should render");

    crate::dialogs::scripted_reset();
}

#[test]
fn test_session_close_fails_when_no_session_is_open() {
    let mut dispatcher = test_dispatcher();
    let (account, _vehicle) = seed_account_and_vehicle(&mut dispatcher);

    let no_account_error = crate::dispatcher::sessions::session_close(
        &mut dispatcher.trust,
        &session_matches(&[]),
        ReportOutputFormat::Text,
    )
    .expect_err("close without an open session should fail");
    assert_eq!(no_account_error.code, "no_open_session");

    let account_error = crate::dispatcher::sessions::session_close(
        &mut dispatcher.trust,
        &session_matches(&["--account", &account.name]),
        ReportOutputFormat::Text,
    )
    .expect_err("close for an account without a session should fail");
    assert_eq!(account_error.code, "no_open_session");
}
