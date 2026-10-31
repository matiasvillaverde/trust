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

#[test]
fn test_onboarding_init_and_status_cover_missing_and_success_paths() {
    let _guard = env_guard();
    std::env::remove_var("TRUST_PROTECTED_KEYWORD_EXPECTED");
    std::env::set_var("TRUST_DISABLE_KEYCHAIN", "1");
    crate::protected_keyword::delete().expect("reset protected keyword");
    let mut dispatcher = test_dispatcher();

    crate::dispatcher::onboarding::onboarding_status(
        &mut dispatcher.trust,
        ReportOutputFormat::Text,
    )
    .expect("missing protected keyword status should render text");
    crate::dispatcher::onboarding::onboarding_status(
        &mut dispatcher.trust,
        ReportOutputFormat::Json,
    )
    .expect("missing protected keyword status should render json");

    let missing_keyword = Command::new("onboarding")
        .arg(Arg::new("protected-keyword").long("protected-keyword"))
        .get_matches_from(["onboarding"]);
    let error = crate::dispatcher::onboarding::onboarding_init(
        &mut dispatcher.trust,
        &missing_keyword,
        ReportOutputFormat::Json,
    )
    .expect_err("missing onboarding keyword should fail");
    assert_eq!(error.code, "missing_keyword");

    let blank_keyword = Command::new("onboarding")
        .arg(Arg::new("protected-keyword").long("protected-keyword"))
        .get_matches_from(["onboarding", "--protected-keyword", "   "]);
    let error = crate::dispatcher::onboarding::onboarding_init(
        &mut dispatcher.trust,
        &blank_keyword,
        ReportOutputFormat::Json,
    )
    .expect_err("blank onboarding keyword should fail");
    assert_eq!(error.code, "onboarding_store_failed");

    let text_matches = Command::new("onboarding")
        .arg(Arg::new("protected-keyword").long("protected-keyword"))
        .get_matches_from(["onboarding", "--protected-keyword", "first-secret"]);
    crate::dispatcher::onboarding::onboarding_init(
        &mut dispatcher.trust,
        &text_matches,
        ReportOutputFormat::Text,
    )
    .expect("onboarding text init should store keyword");
    assert_eq!(
        crate::protected_keyword::read_expected().expect("keyword should be stored"),
        "first-secret"
    );

    crate::protected_keyword::delete().expect("clear protected keyword before json init");
    let json_matches = Command::new("onboarding")
        .arg(Arg::new("protected-keyword").long("protected-keyword"))
        .get_matches_from(["onboarding", "--protected-keyword", "second-secret"]);
    crate::dispatcher::onboarding::onboarding_init(
        &mut dispatcher.trust,
        &json_matches,
        ReportOutputFormat::Json,
    )
    .expect("onboarding json init should store keyword");
    assert_eq!(
        crate::protected_keyword::read_expected().expect("keyword should be stored"),
        "second-secret"
    );

    crate::protected_keyword::delete().expect("cleanup protected keyword");
    std::env::remove_var("TRUST_DISABLE_KEYCHAIN");
}
