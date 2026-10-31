#![allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used
)]
use super::fixtures::*;
use crate::dispatcher::output::ReportOutputFormat;
use clap::Arg;
use clap::Command;
use model::Currency;
use model::Status;
use rust_decimal_macros::dec;
use uuid::Uuid;

#[test]
fn test_protected_keyword_validator() {
    assert!(crate::dispatcher::context::is_valid_protected_keyword(
        "abc", "abc"
    ));
    assert!(!crate::dispatcher::context::is_valid_protected_keyword(
        "abc", "abcd"
    ));
    assert!(!crate::dispatcher::context::is_valid_protected_keyword(
        "abc", "ABC"
    ));
}

#[test]
fn test_ensure_protected_keyword_requires_argument_and_validates_value() {
    let _guard = env_guard();
    crate::protected_keyword::delete().expect("reset protected keyword state");
    crate::protected_keyword::store("secret").expect("seed protected keyword");

    let mut dispatcher = test_dispatcher();
    let missing = account_matches(&[]);
    let missing_error = crate::dispatcher::context::ensure_protected_keyword(
        &mut dispatcher.trust,
        &missing,
        ReportOutputFormat::Text,
        "op",
    )
    .expect_err("missing keyword should fail");
    assert!(missing_error
        .to_string()
        .contains("protected_keyword_required"));

    let mut dispatcher = test_dispatcher();
    let invalid = account_matches(&["--confirm-protected", "wrong"]);
    let invalid_error = crate::dispatcher::context::ensure_protected_keyword(
        &mut dispatcher.trust,
        &invalid,
        ReportOutputFormat::Text,
        "op",
    )
    .expect_err("invalid keyword should fail");
    assert!(invalid_error
        .to_string()
        .contains("protected_keyword_invalid"));

    let mut dispatcher = test_dispatcher();
    let valid = account_matches(&["--confirm-protected", "secret"]);
    crate::dispatcher::context::ensure_protected_keyword(
        &mut dispatcher.trust,
        &valid,
        ReportOutputFormat::Text,
        "op",
    )
    .expect("valid keyword should authorize");

    crate::protected_keyword::delete().expect("clear protected keyword state");
}

#[test]
fn test_ensure_protected_keyword_reports_not_configured() {
    let _guard = env_guard();
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    crate::protected_keyword::delete().expect("reset protected keyword state");

    let mut dispatcher = test_dispatcher();
    let matches = account_matches(&["--confirm-protected", "secret"]);
    let error = crate::dispatcher::context::ensure_protected_keyword(
        &mut dispatcher.trust,
        &matches,
        ReportOutputFormat::Json,
        "op",
    )
    .expect_err("missing configured keyword should fail first");

    assert!(error
        .to_string()
        .contains("protected_keyword_not_configured"));
}

#[test]
fn test_account_and_trade_resolution_helpers_cover_success_and_not_found_paths() {
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
                thesis: None,
                sector: None,
                asset_class: None,
                context: None,
            },
            dec!(95),
            dec!(100),
            dec!(120),
        )
        .expect("seed trade");

    let by_name = crate::dispatcher::context::resolve_account_arg(
        &mut dispatcher.trust,
        &account.name,
        ReportOutputFormat::Json,
    )
    .expect("account by name");
    assert_eq!(by_name.id, account.id);

    let by_id = crate::dispatcher::context::resolve_account_arg(
        &mut dispatcher.trust,
        &account.id.to_string(),
        ReportOutputFormat::Json,
    )
    .expect("account by id");
    assert_eq!(by_id.id, account.id);

    let resolved_by_id = crate::dispatcher::context::account_by_id(
        &mut dispatcher.trust,
        account.id,
        ReportOutputFormat::Json,
    )
    .expect("account by id helper");
    assert_eq!(resolved_by_id.id, account.id);

    let found_trade = crate::dispatcher::context::find_trade_by_id(
        &mut dispatcher.trust,
        trade.id,
        &[Status::New],
        ReportOutputFormat::Json,
    )
    .expect("trade by id");
    assert_eq!(found_trade.id, trade.id);

    let missing_account_error = crate::dispatcher::context::resolve_account_arg(
        &mut dispatcher.trust,
        "does-not-exist",
        ReportOutputFormat::Json,
    )
    .expect_err("unknown account should fail");
    assert!(missing_account_error
        .to_string()
        .contains("account_not_found"));

    let missing_uuid_error = crate::dispatcher::context::resolve_account_arg(
        &mut dispatcher.trust,
        &Uuid::new_v4().to_string(),
        ReportOutputFormat::Json,
    )
    .expect_err("unknown account uuid should fail");
    assert!(missing_uuid_error.to_string().contains("account_not_found"));

    let missing_id_error = crate::dispatcher::context::account_by_id(
        &mut dispatcher.trust,
        Uuid::new_v4(),
        ReportOutputFormat::Json,
    )
    .expect_err("unknown account id should fail");
    assert!(missing_id_error.to_string().contains("account_not_found"));

    let missing_trade_error = crate::dispatcher::context::find_trade_by_id(
        &mut dispatcher.trust,
        Uuid::new_v4(),
        &[Status::Submitted],
        ReportOutputFormat::Json,
    )
    .expect_err("missing trade should fail");
    assert!(missing_trade_error.to_string().contains("trade_not_found"));
}

#[test]
fn test_resolution_helpers_surface_account_read_failures() {
    let mut dispatcher =
        test_dispatcher_with_read_failures(crate::test_support::ReadFailureFactory::accounts());
    let account_id = Uuid::new_v4();

    let account_arg_error = crate::dispatcher::context::resolve_account_arg(
        &mut dispatcher.trust,
        &account_id.to_string(),
        ReportOutputFormat::Json,
    )
    .expect_err("account list failure should fail UUID lookup");
    assert_eq!(account_arg_error.code, "accounts_unavailable");

    let account_by_id_error = crate::dispatcher::context::account_by_id(
        &mut dispatcher.trust,
        account_id,
        ReportOutputFormat::Json,
    )
    .expect_err("account list failure should fail account_by_id");
    assert_eq!(account_by_id_error.code, "accounts_unavailable");

    let trade_lookup_error = crate::dispatcher::context::find_trade_by_id(
        &mut dispatcher.trust,
        Uuid::new_v4(),
        &[Status::New],
        ReportOutputFormat::Json,
    )
    .expect_err("account list failure should fail trade lookup");
    assert_eq!(trade_lookup_error.code, "accounts_unavailable");

    let no_account_matches = Command::new("level")
        .arg(Arg::new("account").long("account"))
        .get_matches_from(["level"]);
    let level_account_error = crate::dispatcher::levels::resolve_level_account_id(
        &mut dispatcher.trust,
        &no_account_matches,
        ReportOutputFormat::Json,
    )
    .expect_err("account list failure should fail implicit level account resolution");
    assert_eq!(level_account_error.code, "accounts_unavailable");
}
